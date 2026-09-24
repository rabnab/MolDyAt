use super::frame_renderer::FrameRenderer;
use super::wgpu_context::WgpuContext;
use super::wgpu_input::execute_input_action;
use super::wgpu_pipelines::WgpuPipelines;
use crate::wgpu::margins_for_scale_factor;
use rust_thermo_core::graphics::renderer::event_loop::UnifiedRenderState;
use winit::{
    event::{DeviceEvent, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowEventRoute {
    Redraw,
    EnergyWindow,
    MainWindow,
    Ignore,
}

/// Routes window and device events to the WGPU backend's focused handlers.
pub(super) struct EventRouter;

impl EventRouter {
    pub(super) fn route_window_event(
        window_id: WindowId,
        event: WindowEvent,
        ctx: Option<&mut WgpuContext>,
        state: &mut UnifiedRenderState,
        pipelines: Option<&mut WgpuPipelines>,
        event_loop: &ActiveEventLoop,
        main_window: Option<&Window>,
    ) {
        let targets_main_window = main_window.is_some_and(|window| window.id() == window_id);
        let route = Self::classify_window_event(
            &event,
            state.energy_manager().is_energy_window_event(window_id),
            ctx.is_some() && pipelines.is_some() && targets_main_window,
        );

        match route {
            WindowEventRoute::Redraw => {
                let (Some(window), Some(ctx), Some(pipelines)) = (main_window, ctx, pipelines)
                else {
                    return;
                };

                FrameRenderer::handle_redraw_requested(
                    window_id, event_loop, window, ctx, pipelines, state,
                );
            }
            WindowEventRoute::EnergyWindow => {
                if matches!(event, WindowEvent::RedrawRequested) {
                    state.energy_manager_mut().handle_redraw();
                } else {
                    state.energy_manager_mut().handle_event(event);
                }
            }
            WindowEventRoute::MainWindow => {
                let (Some(ctx), Some(pipelines), Some(main_window)) = (ctx, pipelines, main_window)
                else {
                    return;
                };
                Self::handle_main_window_event(
                    event,
                    ctx,
                    state,
                    pipelines,
                    event_loop,
                    main_window,
                );
            }
            WindowEventRoute::Ignore => {}
        }
    }

    pub(super) fn route_device_event(event: DeviceEvent, state: &mut UnifiedRenderState) {
        if Self::should_resume_after_device_event(&event) {
            state.resume_after_resize_if_pending();
        }
    }

    pub(super) fn classify_window_event(
        event: &WindowEvent,
        is_energy_window_event: bool,
        has_main_window_resources: bool,
    ) -> WindowEventRoute {
        if matches!(event, WindowEvent::RedrawRequested) {
            return if is_energy_window_event {
                WindowEventRoute::EnergyWindow
            } else if has_main_window_resources {
                WindowEventRoute::Redraw
            } else {
                WindowEventRoute::Ignore
            };
        }

        if is_energy_window_event {
            return WindowEventRoute::EnergyWindow;
        }

        if has_main_window_resources {
            WindowEventRoute::MainWindow
        } else {
            WindowEventRoute::Ignore
        }
    }

    pub(super) fn should_resume_after_device_event(event: &DeviceEvent) -> bool {
        matches!(
            event,
            DeviceEvent::Button {
                state: ElementState::Released,
                ..
            }
        )
    }

    fn handle_main_window_event(
        event: WindowEvent,
        ctx: &mut WgpuContext,
        state: &mut UnifiedRenderState,
        pipelines: &mut WgpuPipelines,
        window_target: &ActiveEventLoop,
        main_window: &Window,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Window close requested");
                window_target.exit();
            }
            WindowEvent::Resized(size) => {
                log::debug!("Window resized to {}x{}", size.width, size.height);
                ctx.resize(size.width, size.height);
                let margins = margins_for_scale_factor(main_window.scale_factor());
                state.handle_resize(size.width as f32, size.height as f32, margins);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // A follow-up `Resized` event updates the surface dimensions.
            }
            WindowEvent::MouseInput { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseWheel { .. } => {
                let dirty = state.handle_mouse_event(&event);
                state.merge_view_dirty(dirty);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(key) = event.physical_key else {
                    return;
                };

                let action = match event.state {
                    ElementState::Pressed => state.input.handle_key_down(key, event.repeat),
                    ElementState::Released => state.input.handle_key_up(key),
                };

                if let Some(action) = action {
                    let dirty =
                        execute_input_action(action, state, pipelines, window_target, main_window);
                    state.merge_view_dirty(dirty);
                }
            }
            _ => {}
        }
    }
}
