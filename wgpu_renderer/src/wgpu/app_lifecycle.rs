use super::wgpu_context::WgpuContext;
use super::wgpu_pipelines::WgpuPipelines;
use super::wgpu_window::UserEvent;
use crate::wgpu::margins_for_scale_factor;
use crate::wgpu::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};
use rust_thermo_core::application::MainContent;
use rust_thermo_core::graphics::renderer::event_loop::UnifiedRenderState;
use std::sync::Arc;
use winit::dpi::LogicalSize;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

pub(super) struct InitializedApp {
    pub(super) window: Arc<Window>,
    pub(super) ctx: WgpuContext,
    pub(super) pipelines: WgpuPipelines,
    pub(super) state: UnifiedRenderState,
}

/// Bootstraps the WGPU window, GPU context, and shared renderer state.
pub(super) struct AppLifecycle;

impl AppLifecycle {
    pub(super) fn initialize(
        event_loop: &ActiveEventLoop,
        application: MainContent,
        proxy: EventLoopProxy<UserEvent>,
    ) -> InitializedApp {
        let window_attrs = Window::default_attributes()
            .with_title("Molecular Dynamics Simulation (WGPU)")
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

        let ctx = WgpuContext::new(Arc::clone(&window));
        log::debug!("WGPU context initialized, window size: {:?}", ctx.size());

        let pipelines = WgpuPipelines::new(&ctx.device, &ctx.queue, ctx.format);

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
