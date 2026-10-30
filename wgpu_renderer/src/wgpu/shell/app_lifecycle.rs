// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::wgpu_window::UserEvent;
use crate::wgpu::margins_for_scale_factor;
use crate::wgpu::render::surface_window::RenderWindow;
use crate::wgpu::render::wgpu_context::WgpuContext;
use crate::wgpu::render::wgpu_pipelines::WgpuPipelines;
use crate::wgpu::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};
use std::sync::Arc;
use thermo_physics::application::MainContent;
use thermo_ui::graphics::renderer::event_loop::{KeyMap, UnifiedRenderState};
use winit::dpi::LogicalSize;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

pub(crate) struct InitializedApp {
    pub(crate) window: Arc<Window>,
    pub(crate) ctx: WgpuContext,
    pub(crate) pipelines: WgpuPipelines,
    pub(crate) state: UnifiedRenderState,
}

/// Bootstraps the WGPU window, GPU context, and shared renderer state.
pub(crate) struct AppLifecycle;

impl AppLifecycle {
    pub(crate) fn initialize(
        event_loop: &ActiveEventLoop,
        application: MainContent,
        key_map: KeyMap,
        proxy: EventLoopProxy<UserEvent>,
    ) -> InitializedApp {
        let window_attrs = Window::default_attributes()
            .with_title(format!("moldyat – v{}", crate::cli::VERSION))
            .with_inner_size(LogicalSize::new(
                DEFAULT_WINDOW_WIDTH,
                DEFAULT_WINDOW_HEIGHT,
            ))
            .with_min_inner_size(LogicalSize::new(320.0, 240.0));
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("Failed to create window"),
        );

        window.set_cursor_visible(false);

        // The renderer takes the window as a `RenderWindow` (raw handles,
        // size): it never names winit (#372).
        let render_window: Arc<dyn RenderWindow> = Arc::clone(&window) as Arc<dyn RenderWindow>;
        let ctx = WgpuContext::new(render_window, event_loop.owned_display_handle());
        log::debug!("WGPU context initialized, window size: {:?}", ctx.size());

        let pipelines = WgpuPipelines::new(&ctx.device, &ctx.queue, ctx.format, &ctx.glyphs);

        let initial_world_bounds = application.world().world_region();
        let window_size = (ctx.size().0 as f32, ctx.size().1 as f32);
        let margins = margins_for_scale_factor(window.scale_factor());

        let proxy = Arc::new(proxy);
        let wakeup_fn = move || {
            // Once the window is closed the event loop is gone, but the
            // physics thread may still publish a snapshot before it is told
            // to stop. Nobody is left to draw it, so the failure is ignored.
            let _ = proxy.send_event(UserEvent::NewSnapshot);
        };

        let state = UnifiedRenderState::new(
            application,
            initial_world_bounds,
            window_size,
            margins,
            key_map,
            Box::new(wakeup_fn),
        );
        log::info!("Physics thread spawned, main thread: rendering (WGPU)");

        InitializedApp {
            window,
            ctx,
            pipelines,
            state,
        }
    }
}
