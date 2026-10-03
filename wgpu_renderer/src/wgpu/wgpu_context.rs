//! WGPU rendering context
//!
//! Contains the core WGPU resources: Device, Queue, Surface, and Adapter.
//! This struct manages the GPU connection and rendering surface lifecycle.

use std::sync::Arc;
use wgpu::{Adapter, Device, Instance, Queue, Surface, SurfaceConfiguration, TextureFormat};
use winit::window::Window;

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
pub(super) struct PendingSurfaceSize(Option<(u32, u32)>);

impl PendingSurfaceSize {
    /// Records `width` x `height` as the size wanted, replacing an earlier
    /// one. An empty size (a minimised window) is ignored.
    pub(super) fn request(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.0 = Some((width, height));
        }
    }

    /// The size to configure the surface for, once.
    pub(super) fn take(&mut self) -> Option<(u32, u32)> {
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
    ///
    /// # Panics
    /// Panics if no compatible GPU adapter is found or device creation fails.
    pub fn new(window: Arc<Window>) -> Self {
        pollster::block_on(Self::new_async(window))
    }

    /// Async initialization of WGPU context.
    async fn new_async(window: Arc<Window>) -> Self {
        // Create the instance with an explicit native backend per platform.
        log::info!("WGPU preferred backend set: {:?}", PREFERRED_BACKENDS);
        let instance = Instance::new(&wgpu::InstanceDescriptor {
            backends: PREFERRED_BACKENDS,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: Default::default(),
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

        // Configure the surface
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
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
    pub fn apply_pending_resize(&mut self) {
        if let Some((width, height)) = self.pending_size.take() {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Get the dimensions the surface is configured for.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}
