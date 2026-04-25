//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Renders a background grid and balls using instanced quads.
//! Integrates with the physics thread via PhysicsCoordinator to receive simulation snapshots.

use super::wgpu_context::WgpuContext;
use super::wgpu_input::execute_input_action;
use super::wgpu_pipelines::{WgpuPipelines, compute_view_proj};
use crate::wgpu::MARGINS;
use rust_thermo_core::application::MainContent;
use rust_thermo_core::graphics::renderer::ViewportTransform;
use rust_thermo_core::graphics::renderer::event_loop::UnifiedRenderState;
use rust_thermo_core::objects2d::Rect;
use rust_thermo_core::objects2d::ball_impl::BallFactory;
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    window::Window,
};

/// WGPU-based window for rendering the molecular dynamics simulation.
///
/// This struct manages the winit window and WGPU context lifecycle.
pub struct WgpuWindow;

impl WgpuWindow {
    /// Create a new WGPU window configuration.
    pub fn new() -> Self {
        Self
    }

    /// Create the window and run the event loop.
    ///
    /// This method does not return until the window is closed.
    /// It initializes WGPU, creates the rendering surface, spawns the physics thread,
    /// and enters the main event loop receiving data from snapshots.
    ///
    /// # Arguments
    /// * `application` - MainContent to be moved to physics thread
    pub fn create_window(&self, application: MainContent) {
        log::info!("Creating WGPU window");

        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .build()
            .expect("Failed to create event loop");
        let event_loop_proxy = event_loop.create_proxy();
        let mut app = WgpuApp::new(application, event_loop_proxy);
        event_loop
            .run_app(&mut app)
            .expect("Winit event loop terminated with an error");
    }
}

#[derive(Debug)]
enum UserEvent {
    /// Triggered when new snapshot data is available from the physics thread.
    NewSnapshot,
}

struct WgpuApp {
    application: Option<MainContent>,
    window: Option<Arc<Window>>,
    proxy: EventLoopProxy<UserEvent>,
    ctx: Option<WgpuContext>,
    pipelines: Option<WgpuPipelines>,
    state: Option<UnifiedRenderState>,
}

impl WgpuApp {
    fn new(application: MainContent, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            application: Some(application),
            proxy,
            window: None,
            ctx: None,
            pipelines: None,
            state: None,
        }
    }

    fn init_if_needed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title("Molecular Dynamics Simulation (WGPU)")
            .with_inner_size(PhysicalSize::new(1280, 720))
            .with_min_inner_size(PhysicalSize::new(400, 300));
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .expect("Failed to create window"),
        );

        // Hide system cursor - we render our own crosshair
        window.set_cursor_visible(false);

        // Initialize WGPU context
        let ctx = WgpuContext::new(Arc::clone(&window));
        log::debug!("WGPU context initialized, window size: {:?}", ctx.size());

        // Initialize all rendering pipelines
        let pipelines = WgpuPipelines::new(&ctx.device, &ctx.queue, ctx.format);

        // Build UnifiedRenderState (spawns physics thread internally)
        let initial_world_bounds = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let window_size = (ctx.size().0 as f32, ctx.size().1 as f32);
        let application = self
            .application
            .take()
            .expect("Application state already consumed");

        let proxy = Arc::new(self.proxy.clone());
        let wakeup_fn = move || {
            proxy
                .send_event(UserEvent::NewSnapshot)
                .expect("Failed to send NewSnapshot event");
        };

        let state = UnifiedRenderState::new(
            application,
            initial_world_bounds,
            window_size,
            MARGINS,
            Box::new(wakeup_fn),
        );
        log::info!("Physics thread spawned, main thread: rendering (WGPU)");

        self.window = Some(window);
        self.ctx = Some(ctx);
        self.pipelines = Some(pipelines);
        self.state = Some(state);
    }
}

impl ApplicationHandler<UserEvent> for WgpuApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Polling causes about_to_wait to be called, which is where we pull the latest snapshot and request redraws.
        event_loop.set_control_flow(ControlFlow::Wait);
        self.init_if_needed(event_loop);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::NewSnapshot => {
                // Pull latest snapshot and request redraws for both windows
                if let (Some(state), Some(window)) = (self.state.as_mut(), self.window.as_ref()) {
                    state.begin_frame();

                    // Request redraw for main window
                    window.request_redraw();

                    // actually redraw for energy window if it's open
                    state.energy_manager.handle_redraw();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        if event == WindowEvent::RedrawRequested {
            log::trace!(
                "Handling redraw for  window with id {:?} and should_render: {}",
                window_id,
                state.should_render()
            );
            // Check if redraw is for energy window
            // if state.should_render() && state.energy_manager.is_energy_window_event(window_id) {
            //     state.energy_manager.handle_redraw();
            //     return;
            // }

            // Render main window
            let Some(window) = self.window.as_ref() else {
                return;
            };
            if window_id != window.id() {
                return;
            }

            if state.should_render() {
                // Start metrics timer just before issuing draw calls.
                state.begin_render();

                // Render from snapshot if available
                if let Some(snapshot) = &state.last_snapshot {
                    let Some(ctx) = self.ctx.as_mut() else {
                        return;
                    };
                    let Some(pipelines) = self.pipelines.as_mut() else {
                        return;
                    };

                    // Compute view-projection matrix
                    let pan = state.viewport.pan_offset();
                    let zoom = state.viewport.zoom();
                    let view_proj = compute_view_proj(&snapshot.grid_spec, pan.x, pan.y, zoom);

                    // Update all pipelines from snapshot
                    let show_ball_outline = state.active_tool
                        == rust_thermo_core::graphics::renderer::event_loop::OperatorTool::Ball;
                    let cursor_wp = (state.cursor_world_pos.x, state.cursor_world_pos.y);
                    let ball_count = snapshot.balls.len() as u32;
                    let operator_previews = state.operator_previews();
                    if let Err(e) = pipelines.prepare_from_snapshot(
                        ctx,
                        snapshot,
                        view_proj,
                        cursor_wp,
                        BallFactory::radius_for_size_idx(state.current_ball_size_idx),
                        show_ball_outline,
                        state.config.thermo_display,
                        operator_previews,
                    ) {
                        eprintln!("Pipeline prepare error: {:?}", e);
                    }

                    state.metrics.record_balls_rendered(ball_count);
                    for _ in 0..6 {
                        state.metrics.record_draw_call(0);
                    }

                    // Render frame
                    if let Err(e) = render_frame(ctx, pipelines) {
                        log::error!("Render error: {:?}", e);
                        if e == wgpu::SurfaceError::OutOfMemory {
                            event_loop.exit();
                        }
                    }
                }

                // Finalise metrics and mark frame as displayed so
                // should_render() won't re-trigger for the same data.
                state.end_frame();
            }

            return;
        }

        // Check if this event is for the energy window
        if state.energy_manager.is_energy_window_event(window_id) {
            state.energy_manager.handle_event(event);
            return;
        }

        let Some(ctx) = self.ctx.as_mut() else {
            return;
        };
        let Some(pipelines) = self.pipelines.as_mut() else {
            return;
        };

        // Main window events
        handle_window_event(event, ctx, state, pipelines, event_loop);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        // Window-border resize may not emit a client-area MouseInput release.
        // DeviceEvent::Button release is a reliable signal to finish resize pause.
        if let DeviceEvent::Button {
            state: ElementState::Released,
            ..
        } = event
        {
            state.resume_after_resize_if_pending();
        }
    }
}

/// Handle window events (resize, input, etc.)
fn handle_window_event(
    event: WindowEvent,
    ctx: &mut WgpuContext,
    state: &mut UnifiedRenderState,
    pipelines: &mut WgpuPipelines,
    window_target: &ActiveEventLoop,
) {
    match event {
        WindowEvent::CloseRequested => {
            log::info!("Window close requested");
            window_target.exit();
        }
        WindowEvent::Resized(size) => {
            log::debug!("Window resized to {}x{}", size.width, size.height);
            ctx.resize(size.width, size.height);
            let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
                size.width,
                size.height,
                MARGINS,
            );
            state.handle_resize_with_world_rect(size.width as f32, size.height as f32, world_rect);
        }
        WindowEvent::ScaleFactorChanged { .. } => {
            // A follow-up `Resized` event updates the surface dimensions.
        }
        WindowEvent::MouseInput { .. }
        | WindowEvent::CursorMoved { .. }
        | WindowEvent::MouseWheel { .. } => {
            state.view_dirty |= state.handle_mouse_event(&event);
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let PhysicalKey::Code(key) = event.physical_key else {
                return;
            };

            // Use InputHandler to process keyboard input
            let action = match event.state {
                ElementState::Pressed => state.input.handle_key_down(key, event.repeat),
                ElementState::Released => state.input.handle_key_up(key),
            };

            if let Some(action) = action {
                state.view_dirty |=
                    execute_input_action(action, state, pipelines, &ctx.queue, window_target);
            }
        }
        _ => {}
    }
}

/// Render a single frame.
fn render_frame(ctx: &WgpuContext, pipelines: &WgpuPipelines) -> Result<(), wgpu::SurfaceError> {
    let output = ctx.surface.get_current_texture()?;
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Main Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        pipelines.render_all(&mut render_pass, ctx);
    }

    ctx.queue.submit(std::iter::once(encoder.finish()));
    output.present();

    Ok(())
}
