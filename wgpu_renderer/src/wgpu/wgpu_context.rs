//! WGPU rendering context
//!
//! Contains the core WGPU resources: Device, Queue, Surface, and Adapter.
//! This struct manages the GPU connection and rendering surface lifecycle.

use std::sync::Arc;
use wgpu::{Adapter, Device, Instance, Queue, Surface, SurfaceConfiguration, TextureFormat};
use winit::window::Window;

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
        // Create wgpu instance with default backends
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
            flags: wgpu::InstanceFlags::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
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
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Primary Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
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
        }
    }

    /// Resize the rendering surface.
    ///
    /// Call this when the window is resized to update the surface dimensions.
    ///
    /// # Arguments
    /// * `width` - New width in pixels (must be > 0)
    /// * `height` - New height in pixels (must be > 0)
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Get the current surface dimensions.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}
