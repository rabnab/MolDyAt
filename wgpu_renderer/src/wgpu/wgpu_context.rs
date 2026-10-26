// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! WGPU rendering context
//!
//! Contains the core WGPU resources: Device, Queue, Surface, and Adapter.
//! This struct manages the GPU connection and rendering surface lifecycle.

use std::sync::Arc;
use std::time::Instant;
use wgpu::{
    Adapter, Device, Instance, PresentMode, Queue, Surface, SurfaceConfiguration, TextureFormat,
};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

/// Overrides the present mode: `fifo`, `fifo_relaxed`, `mailbox`,
/// `immediate`, `auto_vsync` or `auto_no_vsync` (default: see
/// [`default_present_mode`]). With `WGPU_BACKEND` (e.g. `vulkan`, `dx12`)
/// and [`FRAME_LATENCY_VAR`] it lets presentation be compared without a
/// rebuild (see issue #217).
const PRESENT_MODE_VAR: &str = "THERMO_PRESENT_MODE";

/// Overrides `desired_maximum_frame_latency` (1 to 3, default 2).
const FRAME_LATENCY_VAR: &str = "THERMO_FRAME_LATENCY";

const DEFAULT_FRAME_LATENCY: u32 = 2;

/// The present mode named `name`, as accepted in [`PRESENT_MODE_VAR`].
pub(super) fn parse_present_mode(name: &str) -> Option<PresentMode> {
    match name.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "fifo" => Some(PresentMode::Fifo),
        "fifo_relaxed" => Some(PresentMode::FifoRelaxed),
        "mailbox" => Some(PresentMode::Mailbox),
        "immediate" => Some(PresentMode::Immediate),
        "auto_vsync" => Some(PresentMode::AutoVsync),
        "auto_no_vsync" => Some(PresentMode::AutoNoVsync),
        _ => None,
    }
}

/// The frame latency in `value`, as accepted in [`FRAME_LATENCY_VAR`].
pub(super) fn parse_frame_latency(value: &str) -> Option<u32> {
    value
        .trim()
        .parse()
        .ok()
        .filter(|latency| (1..=3).contains(latency))
}

/// Mailbox where the surface supports it, else AutoVsync (Fifo).
///
/// With Fifo, every resize step waits on presentation, and on Vulkan under
/// Wayland dragging the window border feels sluggish; Mailbox (like
/// Immediate) follows the cursor, without Immediate's tearing (issue #217).
/// Frames are still paced by physics snapshots and input, not by present.
pub(super) fn default_present_mode(supported: &[PresentMode]) -> PresentMode {
    if supported.contains(&PresentMode::Mailbox) {
        PresentMode::Mailbox
    } else {
        PresentMode::AutoVsync
    }
}

/// The present mode for `requested` (the value of [`PRESENT_MODE_VAR`], if
/// set) on a surface supporting `supported`. A mode the surface can't use,
/// or an unknown name, falls back to the default with a warning.
pub(super) fn choose_present_mode(
    requested: Option<&str>,
    supported: &[PresentMode],
) -> PresentMode {
    let default = default_present_mode(supported);
    let Some(name) = requested else {
        return default;
    };
    match parse_present_mode(name) {
        // The auto modes fall back to a supported mode by themselves.
        Some(mode @ (PresentMode::AutoVsync | PresentMode::AutoNoVsync)) => mode,
        Some(mode) if supported.contains(&mode) => mode,
        Some(mode) => {
            log::warn!(
                "{PRESENT_MODE_VAR}={name}: {mode:?} is not supported here ({supported:?}), using {default:?}"
            );
            default
        }
        None => {
            log::warn!("{PRESENT_MODE_VAR}={name} is not a present mode, using {default:?}");
            default
        }
    }
}

fn present_mode_from_env(supported: &[PresentMode]) -> PresentMode {
    let requested = std::env::var(PRESENT_MODE_VAR).ok();
    choose_present_mode(requested.as_deref(), supported)
}

fn frame_latency_from_env() -> u32 {
    let Ok(value) = std::env::var(FRAME_LATENCY_VAR) else {
        return DEFAULT_FRAME_LATENCY;
    };
    parse_frame_latency(&value).unwrap_or_else(|| {
        log::warn!("{FRAME_LATENCY_VAR}={value} is not 1, 2 or 3, using {DEFAULT_FRAME_LATENCY}");
        DEFAULT_FRAME_LATENCY
    })
}

#[cfg(target_os = "windows")]
const PREFERRED_BACKENDS: wgpu::Backends = wgpu::Backends::DX12;

#[cfg(target_os = "linux")]
const PREFERRED_BACKENDS: wgpu::Backends = wgpu::Backends::VULKAN;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
const PREFERRED_BACKENDS: wgpu::Backends = wgpu::Backends::PRIMARY;

/// The newest surface size asked for that the surface was not configured for
/// yet.
///
/// Dragging a window border delivers many `Resized` events per frame, and
/// configuring the surface recreates the swapchain (waiting for the GPU). So
/// sizes are only recorded as they arrive and the surface is configured once,
/// for the newest one, before the next frame is drawn.
#[derive(Debug, Default)]
pub(super) struct PendingSurfaceSize(Option<PendingResize>);

/// A surface size waiting to be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingResize {
    pub size: (u32, u32),
    /// When the first of the requests it merges came in.
    pub since: Instant,
    /// How many requests it merges.
    pub requests: u32,
}

impl PendingSurfaceSize {
    /// Records `width` x `height` as the size wanted, replacing an earlier
    /// one. An empty size (a minimised window) is ignored.
    pub(super) fn request(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        match &mut self.0 {
            Some(pending) => {
                pending.size = (width, height);
                pending.requests += 1;
            }
            None => {
                self.0 = Some(PendingResize {
                    size: (width, height),
                    since: Instant::now(),
                    requests: 1,
                });
            }
        }
    }

    /// The size to configure the surface for, once.
    pub(super) fn take(&mut self) -> Option<PendingResize> {
        self.0.take()
    }
}

/// Core WGPU rendering context containing GPU resources.
///
/// This struct owns the fundamental WGPU objects needed for rendering:
/// - `Instance`: Entry point to wgpu, used to create adapters and surfaces
/// - `Adapter`: Handle to a physical graphics device
/// - `Device`: Logical connection to the GPU
/// - `Queue`: Command submission queue
/// - `Surface`: Rendering target (window surface)
/// - `SurfaceConfiguration`: Current surface settings
pub struct WgpuContext {
    /// Also creates the energy plot window's surface (`plot_surface.rs`).
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'static>,
    pub config: SurfaceConfiguration,
    pub format: TextureFormat,
    pending_size: PendingSurfaceSize,
}

impl WgpuContext {
    /// Create a new WGPU context for the given window.
    ///
    /// This initializes all GPU resources synchronously using pollster.
    ///
    /// # Arguments
    /// * `window` - The winit window to create a rendering surface for
    /// * `display` - The event loop's display handle, needed by the GL backend
    ///
    /// # Panics
    /// Panics if no compatible GPU adapter is found or device creation fails.
    pub fn new(window: Arc<Window>, display: OwnedDisplayHandle) -> Self {
        pollster::block_on(Self::new_async(window, display))
    }

    /// Async initialization of WGPU context.
    async fn new_async(window: Arc<Window>, display: OwnedDisplayHandle) -> Self {
        // Create the instance with an explicit native backend per platform,
        // unless `WGPU_BACKEND` names another.
        let backends = wgpu::Backends::from_env().unwrap_or(PREFERRED_BACKENDS);
        log::info!("WGPU backends: {:?}", backends);
        // The display handle is required by the GL backend (`WGPU_BACKEND=gl`)
        // and unused by Vulkan and DX12.
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(display))
        });

        // Create surface from window
        // SAFETY: The window handle is valid for the lifetime of the surface
        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        // Request an adapter (GPU)
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("Failed to find a compatible GPU adapter");

        log::info!(
            "WGPU adapter: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        // Request device and queue
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Primary Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("Failed to create device");

        // Get window size for surface configuration
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        // Get preferred surface format
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        log::debug!("Surface format: {:?}", format);

        let present_mode = present_mode_from_env(&surface_caps.present_modes);
        let frame_latency = frame_latency_from_env();
        log::info!(
            "Surface: present mode {:?} (supported {:?}), frame latency {}",
            present_mode,
            surface_caps.present_modes,
            frame_latency
        );

        // Configure the surface
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width,
            height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: frame_latency,
        };
        surface.configure(&device, &config);

        Self {
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
            format,
            pending_size: PendingSurfaceSize::default(),
        }
    }

    /// Ask for the rendering surface to be resized.
    ///
    /// Call this when the window is resized. The surface is not configured
    /// here: that happens in [`Self::apply_pending_resize`], once for the
    /// newest size however many calls came before it.
    ///
    /// # Arguments
    /// * `width` - New width in pixels (ignored if 0)
    /// * `height` - New height in pixels (ignored if 0)
    pub fn resize(&mut self, width: u32, height: u32) {
        self.pending_size.request(width, height);
    }

    /// Configure the surface for the size last asked for with
    /// [`Self::resize`], if any. Call before acquiring a frame. This also
    /// reconfigures a lost or outdated surface at an unchanged size.
    ///
    /// Returns the resize that was applied, if any.
    pub fn apply_pending_resize(&mut self) -> Option<PendingResize> {
        let pending = self.pending_size.take()?;
        (self.config.width, self.config.height) = pending.size;
        self.surface.configure(&self.device, &self.config);
        Some(pending)
    }

    /// Get the dimensions the surface is configured for.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}
