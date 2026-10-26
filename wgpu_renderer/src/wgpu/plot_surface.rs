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

/// A format that stores the colours as given (no sRGB encoding), so the
/// plot's colours match their values; else the first the surface offers.
pub(super) fn plot_surface_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .or_else(|| formats.first().copied())
}

pub(super) struct PlotSurface {
    /// Keeps the window alive as long as the surface: drop the surface (see
    /// [`sync_plot_surface`]) when the energy window closes.
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
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: size.width.max(1),
            height: size.height.max(1),
            // Every surface supports Fifo; the plot redraws at most ~12 Hz.
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
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

/// Drop the surface once the energy window it was made for is closed or
/// replaced (`open` is the window open now), so the window can go away.
pub(super) fn sync_plot_surface(plot: &mut Option<PlotSurface>, open: Option<WindowId>) {
    if plot.as_ref().map(PlotSurface::window_id) != open {
        *plot = None;
    }
}

/// Draw `frame`, creating the surface for its window first if needed.
pub(super) fn draw_plot_frame(plot: &mut Option<PlotSurface>, ctx: &WgpuContext, frame: PlotFrame) {
    sync_plot_surface(plot, Some(frame.window.id()));
    if plot.is_none() {
        match PlotSurface::new(ctx, frame.window) {
            Ok(surface) => *plot = Some(surface),
            Err(error) => {
                log::error!("{error}");
                return;
            }
        }
    }
    if let Some(surface) = plot {
        surface.render(&ctx.device, &ctx.queue, &frame.scene);
    }
}
