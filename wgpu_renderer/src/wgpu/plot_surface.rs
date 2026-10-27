// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plot window (#371): its OS window, created, placed and closed here as
//! `thermo_ui` asks ([`PlotWindowRequest`]), and its surface, created on the
//! main window's device (shared instance, adapter, device and queue) with
//! its own swapchain and [`PlotPipeline`]. `thermo_ui` keeps what the window
//! shows and lays the plot out; this draws the [`PlotFrame`]s it hands out.

use std::sync::Arc;
use thermo_ui::graphics::energy_plot::{
    DEFAULT_HEIGHT, DEFAULT_WIDTH, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, PlotScene,
    PlotWindowRequest, WindowGeometry, split_screen_layout,
};
use thermo_ui::graphics::renderer::event_loop::{
    EnergyWindowManager, PlotFrame, UnifiedRenderState,
};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::pipelines::plot_pipeline::PlotPipeline;
use super::wgpu_context::WgpuContext;

/// The surface format for the plot: 8-bit without sRGB encoding, so the
/// plot's colours appear as given (`Bgra8Unorm`, then `Rgba8Unorm`), else
/// any format without sRGB encoding, else the first the surface offers. A
/// wider format (e.g. `Rgba16Float`) would only cost memory for a line plot.
pub(super) fn plot_surface_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    use wgpu::TextureFormat::{Bgra8Unorm, Rgba8Unorm};
    [Bgra8Unorm, Rgba8Unorm]
        .into_iter()
        .find(|preferred| formats.contains(preferred))
        .or_else(|| formats.iter().copied().find(|format| !format.is_srgb()))
        .or_else(|| formats.first().copied())
}

pub(super) struct PlotSurface {
    /// Keeps the window alive as long as the surface: [`PlotTarget`] drops
    /// the surface with the window.
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: PlotPipeline,
}

impl PlotSurface {
    pub(super) fn new(ctx: &WgpuContext, window: Arc<Window>) -> Result<Self, String> {
        let surface = ctx
            .instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| format!("cannot create the energy window surface: {e}"))?;
        let caps = surface.get_capabilities(&ctx.adapter);
        let format = plot_surface_format(&caps.formats)
            .ok_or("the energy window surface supports no format on this adapter")?;
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or("the energy window surface supports no alpha mode on this adapter")?;
        let size = Window::inner_size(&window);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: size.width.max(1),
            height: size.height.max(1),
            // Every surface supports Fifo; the plot redraws at most ~12 Hz.
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&ctx.device, &config);
        log::debug!("Energy window surface: {format:?}");
        Ok(Self {
            window,
            surface,
            pipeline: PlotPipeline::new(&ctx.device, &ctx.queue, format),
            config,
        })
    }

    fn configure(&mut self, device: &wgpu::Device, (width, height): (u32, u32)) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(device, &self.config);
    }

    /// Draw `scene` and present it. A frame that cannot be acquired is
    /// skipped; the next redraw tries again.
    pub(super) fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &PlotScene) {
        if scene.size != (self.config.width, self.config.height) {
            self.configure(device, scene.size);
        }
        let (output, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure(device, scene.size);
                self.window.request_redraw();
                return;
            }
            status => {
                log::debug!("Energy window frame skipped: {status:?}");
                return;
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.pipeline.prepare(device, queue, scene);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Plot Encoder"),
        });
        {
            let [r, g, b, a] = scene.background.map(f64::from);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Plot Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            self.pipeline.render(&mut pass);
        }
        queue.submit(std::iter::once(encoder.finish()));
        self.window.pre_present_notify();
        queue.present(output);
        if suboptimal {
            self.configure(device, scene.size);
        }
    }
}

/// What the request handling needs of the plot window's OS window, so it
/// can be tested with a fake one.
pub(super) trait PlotOsWindow {
    fn inner_size(&self) -> (u32, u32);
    fn scale_factor(&self) -> f64;
    fn set_title(&self, title: &str);
    fn request_redraw(&self);
}

impl PlotOsWindow for Arc<Window> {
    fn inner_size(&self) -> (u32, u32) {
        let size = Window::inner_size(self);
        (size.width, size.height)
    }
    fn scale_factor(&self) -> f64 {
        Window::scale_factor(self)
    }
    fn set_title(&self, title: &str) {
        Window::set_title(self, title);
    }
    fn request_redraw(&self) {
        Window::request_redraw(self);
    }
}

/// Carry out the plot window requests `thermo_ui` queued (#371) on
/// `window`, creating it with `open` for [`PlotWindowRequest::Open`], and
/// report back to `manager`: the size of a new window, or that it could not
/// be made. Reports can queue more requests (a new window is drawn at
/// once); those are carried out too.
pub(super) fn handle_plot_requests<W: PlotOsWindow>(
    window: &mut Option<W>,
    manager: &mut EnergyWindowManager,
    mut open: impl FnMut(&str) -> Result<W, String>,
) {
    // A report queues at most a redraw, so two rounds are enough; the
    // bound only guards against a loop.
    for _ in 0..4 {
        let requests = manager.take_requests();
        if requests.is_empty() {
            return;
        }
        for request in requests {
            match request {
                PlotWindowRequest::Open { title } => match open(&title) {
                    Ok(new) => {
                        let (size, scale) = (new.inner_size(), new.scale_factor());
                        *window = Some(new);
                        manager.plot_window_opened(size, scale);
                    }
                    Err(error) => {
                        log::error!("Failed to create the plot window: {error}");
                        manager.plot_window_failed();
                    }
                },
                PlotWindowRequest::Close => *window = None,
                PlotWindowRequest::SetTitle(title) => {
                    if let Some(window) = window {
                        window.set_title(&title);
                    }
                }
                PlotWindowRequest::Redraw => {
                    if let Some(window) = window {
                        window.request_redraw();
                    }
                }
            }
        }
    }
}

/// Where the plot window goes: the energy half of the main window's
/// monitor (else the primary one), so the simulation window keeps its size
/// (#167); `None` without a monitor.
fn plot_window_geometry(
    event_loop: &ActiveEventLoop,
    main_window: Option<&Window>,
) -> Option<WindowGeometry> {
    main_window
        .and_then(Window::current_monitor)
        .or_else(|| event_loop.primary_monitor())
        .map(|monitor| split_screen_layout(monitor.position(), monitor.size()).energy)
}

/// Create the plot window with `title`, placed by [`plot_window_geometry`].
fn create_plot_window(
    event_loop: &ActiveEventLoop,
    main_window: Option<&Window>,
    title: &str,
) -> Result<Arc<Window>, String> {
    let geometry = plot_window_geometry(event_loop, main_window);
    let size = geometry.map_or(PhysicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT), |g| g.size);
    let mut attributes = Window::default_attributes()
        .with_title(title)
        .with_inner_size(size)
        .with_resizable(true)
        .with_min_inner_size(LogicalSize::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT));
    if let Some(geometry) = geometry {
        attributes = attributes.with_position(geometry.position);
    }
    event_loop
        .create_window(attributes)
        .map(Arc::new)
        .map_err(|e| e.to_string())
}

/// The plot window (#371: the backend owns it) and its surface while it is
/// open. A surface that could not be made is logged once and not retried
/// until the window is replaced.
#[derive(Default)]
pub(super) struct PlotTarget {
    window: Option<Arc<Window>>,
    surface: Option<PlotSurface>,
    surface_failed: bool,
}

impl PlotTarget {
    /// The plot window's id while it is open, to route its events.
    pub(super) fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    /// Carry out the plot window requests of `state` (see
    /// [`handle_plot_requests`]). The surface goes with its window.
    pub(super) fn apply_requests(
        &mut self,
        state: &mut UnifiedRenderState,
        event_loop: &ActiveEventLoop,
        main_window: Option<&Window>,
    ) {
        let before = self.window_id();
        handle_plot_requests(&mut self.window, state.energy_manager_mut(), |title| {
            create_plot_window(event_loop, main_window, title)
        });
        if self.window_id() != before {
            self.surface = None;
            self.surface_failed = false;
        }
    }

    /// Draw `frame` into the plot window, creating its surface first if
    /// needed.
    pub(super) fn draw(&mut self, ctx: &WgpuContext, frame: PlotFrame) {
        let Some(window) = &self.window else {
            return;
        };
        if self.surface.is_none() && !self.surface_failed {
            match PlotSurface::new(ctx, Arc::clone(window)) {
                Ok(surface) => self.surface = Some(surface),
                Err(error) => {
                    log::error!("{error}; the energy plot stays empty");
                    self.surface_failed = true;
                }
            }
        }
        if let Some(surface) = &mut self.surface {
            surface.render(&ctx.device, &ctx.queue, &frame.scene);
        }
    }
}
