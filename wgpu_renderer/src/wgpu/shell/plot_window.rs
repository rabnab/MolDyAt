// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plot window's OS window (#371): created, placed, retitled and closed
//! here as `thermo_ui` asks ([`PlotWindowRequest`]); the renderer draws into
//! it through [`PlotSurfaces`], which knows it only by an opaque id (#372).

use std::sync::Arc;
use thermo_ui::graphics::energy_plot::{
    DEFAULT_HEIGHT, DEFAULT_WIDTH, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, PlotWindowRequest,
    WindowGeometry, split_screen_layout,
};
use thermo_ui::graphics::renderer::event_loop::{
    EnergyWindowManager, PlotFrame, UnifiedRenderState,
};
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::winit_window::surface_id;
use crate::wgpu::render::plot_surface::PlotSurfaces;
use crate::wgpu::render::surface_window::RenderWindow;
use crate::wgpu::render::wgpu_context::WgpuContext;

/// What the request handling needs of the plot window's OS window, so it
/// can be tested with a fake one.
pub(crate) trait PlotOsWindow {
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
pub(crate) fn handle_plot_requests<W: PlotOsWindow>(
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

/// The plot window (#371: the shell owns it) and, through the renderer,
/// its surface (keyed by the window's opaque id, #372).
#[derive(Default)]
pub(crate) struct PlotTarget {
    window: Option<Arc<Window>>,
    surfaces: PlotSurfaces,
}

impl PlotTarget {
    /// The plot window's id while it is open, to route its events.
    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    /// Carry out the plot window requests of `state` (see
    /// [`handle_plot_requests`]). The surface goes with its window.
    pub(crate) fn apply_requests(
        &mut self,
        state: &mut UnifiedRenderState,
        event_loop: &ActiveEventLoop,
        main_window: Option<&Window>,
    ) {
        handle_plot_requests(&mut self.window, state.energy_manager_mut(), |title| {
            create_plot_window(event_loop, main_window, title)
        });
        // The surface goes with its window.
        self.surfaces.sync(self.window_id().map(surface_id));
    }

    /// Draw `frame` into the plot window, creating its surface first if
    /// needed.
    pub(crate) fn draw(&mut self, ctx: &WgpuContext, frame: PlotFrame) {
        let Some(window) = &self.window else {
            return;
        };
        let render_window: Arc<dyn RenderWindow> = Arc::clone(window) as Arc<dyn RenderWindow>;
        self.surfaces
            .draw(ctx, surface_id(window.id()), render_window, &frame.scene);
    }
}
