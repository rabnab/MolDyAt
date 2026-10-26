// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The energy plot window's surface: created on the main window's device
//! (shared instance, adapter, device and queue), with its own swapchain and
//! [`PlotPipeline`]. `thermo_ui` owns the window and lays the plot out; this
//! draws the [`PlotFrame`]s it hands out.

use std::sync::Arc;
use thermo_ui::graphics::energy_plot::PlotScene;
use thermo_ui::graphics::renderer::event_loop::PlotFrame;
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
    /// Keeps the window alive as long as the surface: drop the surface (see
    /// [`PlotTarget::sync`]) when the energy window closes.
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
        let size = window.inner_size();
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

    pub(super) fn window_id(&self) -> WindowId {
        self.window.id()
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

/// The energy window's surface while it is open, and the window it could
/// not be created for (if any), so a failure is logged once and not retried
/// at every redraw until the window is replaced.
#[derive(Default)]
pub(super) struct PlotTarget {
    surface: Option<PlotSurface>,
    failed: Option<WindowId>,
}

impl PlotTarget {
    /// Forget the surface and a failure once the energy window they belong
    /// to is closed or replaced (`open` is the window open now), so the
    /// window can go away and a new one gets its own try.
    pub(super) fn sync(&mut self, open: Option<WindowId>) {
        if self.surface.as_ref().map(PlotSurface::window_id) != open {
            self.surface = None;
        }
        if self.failed.is_some() && self.failed != open {
            self.failed = None;
        }
    }

    /// Whether a surface for `window` is still to be created: none exists
    /// for it, and creating one has not failed for it before.
    pub(super) fn should_create(&self, window: WindowId) -> bool {
        self.surface.is_none() && self.failed != Some(window)
    }

    /// Record that no surface could be made for `window`.
    pub(super) fn mark_failed(&mut self, window: WindowId) {
        self.failed = Some(window);
    }

    /// Draw `frame`, creating the surface for its window first if needed.
    pub(super) fn draw(&mut self, ctx: &WgpuContext, frame: PlotFrame) {
        let window = frame.window.id();
        self.sync(Some(window));
        if self.should_create(window) {
            match PlotSurface::new(ctx, frame.window) {
                Ok(surface) => self.surface = Some(surface),
                Err(error) => {
                    log::error!("{error}; the energy plot stays empty");
                    self.mark_failed(window);
                }
            }
        }
        if let Some(surface) = &mut self.surface {
            surface.render(&ctx.device, &ctx.queue, &frame.scene);
        }
    }
}
