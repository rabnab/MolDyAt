use crate::application::physics_thread::PhysicsCommand;
use crate::application::{LogStuff, MainContent};
use crate::graphics::energy_plot::EnergyData;
use crate::graphics::ggez::ggez_renderer::GgezRenderer;
use crate::graphics::ggez::ggez_renderer_backend::GgezBackend;
use crate::graphics::renderer::event_loop::{EnergyWindowManager, EventLoopState, InputAction};
use crate::graphics::renderer::viewport::ViewportTransform;
use ggez::Context;
use ggez::event::process_event;
use ggez::event::winit_event::{ElementState, Event, KeyboardInput, WindowEvent};
use ggez::input::keyboard::KeyCode;
use std::sync::{Arc, Mutex};
use winit::event_loop::{ControlFlow, EventLoop};

pub struct CustomEventLoop {
    // Shared state with InputHandler, PhysicsCoordinator, and ViewportState
    state: EventLoopState,

    // ggez-specific fields
    renderer: GgezRenderer,

    // Shared energy window management
    energy_manager: EnergyWindowManager,

    // Additional tracking for render FPS
    last_processed_step: u64,
    simulation_fps: f32,
}

impl CustomEventLoop {
    /// Create a new custom event loop with physics thread and renderer.
    ///
    /// # Arguments
    /// * `application` - MainContent with simulation state (will be moved to physics thread)
    /// * `renderer` - Renderer for visualization
    pub fn new(application: MainContent, renderer: GgezRenderer) -> Self {
        let energy_data = Arc::new(Mutex::new(EnergyData::new()));

        // Create shared event loop state (this spawns physics thread)
        let state = EventLoopState::new(application);

        // Use shared energy window manager
        let energy_manager = EnergyWindowManager::new(energy_data);

        Self {
            state,
            renderer,
            energy_manager,
            last_processed_step: 0,
            simulation_fps: 0.0,
        }
    }

    // No longer need toggle_energy_window - use energy_manager.toggle() instead

    /// Start the main event loop.
    /// Takes ownership and runs until window is closed.
    ///
    /// # Arguments
    /// * `ctx` - ggez graphics context
    /// * `event_loop` - Winit event loop
    pub fn run(mut self, mut ctx: Context, event_loop: EventLoop<()>) {
        event_loop.run(move |mut event, window_target, control_flow| {
            if ctx.quit_requested {
                ctx.continuing = false;
            }
            if !ctx.continuing {
                *control_flow = ControlFlow::Exit;
                return;
            }

            *control_flow = ControlFlow::Poll;

            // Let ggez process the event for internal state updates
            process_event(&mut ctx, &mut event);

            match event {
                Event::WindowEvent { event, window_id } => {
                    // Check if this event is for the energy window
                    if self.energy_manager.is_energy_window_event(window_id) {
                        self.energy_manager.handle_event(event);
                        return;
                    }
                    // Otherwise, handle as main window event
                    self.handle_window_event(&mut ctx, event, control_flow, window_target);
                }
                Event::RedrawRequested(window_id) => {
                    // Check if redraw is for energy window
                    if self.energy_manager.is_energy_window_event(window_id) {
                        self.energy_manager.handle_redraw();
                    }
                    // Otherwise it's for the main window, ggez handles it
                }
                Event::MainEventsCleared => {
                    self.update_and_draw(&mut ctx);
                }
                _ => {}
            }
        });
    }

    fn handle_window_event(
        &mut self,
        ctx: &mut Context,
        event: WindowEvent,
        _control_flow: &mut ControlFlow,
        window_target: &winit::event_loop::EventLoopWindowTarget<()>,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                ctx.request_quit();
            }
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode: Some(keycode),
                        state: key_state,
                        ..
                    },
                ..
            } => {
                // Use InputHandler to process keyboard input
                let action = match key_state {
                    ElementState::Pressed => self.state.input.handle_key_down(keycode, false),
                    ElementState::Released => self.state.input.handle_key_up(keycode),
                };

                if let Some(action) = action {
                    // Handle ggez-specific actions here
                    match &action {
                        InputAction::ToggleEnergyWindow => {
                            self.energy_manager.toggle(window_target);
                        }
                        InputAction::CyclePlotContent => {
                            self.renderer.cycle_plot_content();
                        }
                        InputAction::EnableCursor { repeated } => {
                            self.renderer.enable_drawing_cursor(*repeated);
                        }
                        InputAction::DisableCursor => {
                            self.renderer.disable_drawing_cursor();
                        }
                        InputAction::PrintHelp => {
                            MainContent::print_help_message();
                        }
                        InputAction::ToggleRendering => {
                            let new_state = !self.state.render_drawing;
                            self.state.render_drawing = new_state;
                            println!("============\nRender drawing toggled to {}", new_state);
                        }
                        InputAction::ResetView => {
                            self.renderer.viewport_mut().reset();
                            self.state.view_dirty = true;
                        }
                        _ => {}
                    }
                    // Process generic actions through EventLoopState
                    self.state.process_action(&action);
                }
            }
            WindowEvent::MouseInput {
                state: mouse_state,
                button,
                ..
            } => {
                let pos = ctx.mouse.position();
                let world_pos = self.renderer.update_cursor_and_text(pos.x, pos.y);
                let pan_offset = self.renderer.viewport().pan_offset();

                if let Some(action) = self.state.input.handle_mouse_button(
                    button,
                    mouse_state,
                    [pos.x, pos.y],
                    world_pos,
                    pan_offset,
                ) {
                    let affects_view = self.state.process_action(&action);
                    if affects_view {
                        self.state.view_dirty = true;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                let world_pos = self.renderer.update_cursor_and_text(x, y);

                // Update cursor position in InputHandler
                self.state.input.set_cursor_pos(world_pos, [x, y]);

                if let Some(action) = self.state.input.handle_mouse_motion(x, y) {
                    if let InputAction::PanUpdate { delta } = &action {
                        // Handle pan update: convert pixel delta to world delta and apply
                        let start_offset = self.state.input.pan_start_offset();
                        let zoom = self.renderer.viewport().zoom();
                        let new_pan =
                            ViewportTransform::apply_pan_delta(*delta, start_offset, zoom);
                        self.renderer.viewport_mut().pan(new_pan);
                        self.state.view_dirty = true;
                    }
                    self.state.process_action(&action);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let cursor_pos = self.state.input.cursor_pos();

                if let Some(action) = self.state.input.handle_mouse_wheel(delta, cursor_pos) {
                    match &action {
                        InputAction::ZoomIn { center } => {
                            self.renderer.viewport_mut().zoom_at_point(center, 1.1);
                            self.state.view_dirty = true;
                        }
                        InputAction::ZoomOut { center } => {
                            self.renderer
                                .viewport_mut()
                                .zoom_at_point(center, 1.0 / 1.1);
                            self.state.view_dirty = true;
                        }
                        InputAction::ResetView => {
                            self.renderer.viewport_mut().reset();
                            self.state.view_dirty = true;
                        }
                        _ => {}
                    }
                    self.state.process_action(&action);
                }
            }
            WindowEvent::Resized(size) => {
                let _ = self.handle_resize(ctx, size.width as f32, size.height as f32);
            }
            _ => {}
        }
    }

    /// Render from snapshot (new architecture)
    /// This method reads the latest snapshot and renders it without accessing MainContent
    /// Implements freshness checking to prevent displaying stale frames
    fn render_from_snapshot(&mut self, ctx: &mut Context) -> Option<()> {
        // Use EventLoopState's should_render check
        if !self.state.should_render() {
            return Some(());
        }

        let snapshot = self.state.physics.get_latest_snapshot()?;

        let _ = ctx.gfx.begin_frame();

        let cache = self.renderer.take_cache();
        let mut backend = GgezBackend::new(ctx, cache);

        // Draw cell grid and boundary frame first (background layer)
        // Cell grid dimensions now match the actual simulation
        let grid_spec = &snapshot.grid_spec;
        let _ = self.renderer.draw_frame(&mut backend, grid_spec);

        // Draw balls from snapshot (on top of cell grid)
        let _ = self
            .renderer
            .draw_balls_from_snapshot(&mut backend, &snapshot);

        // Draw cursor (on top of balls)
        let _ = self
            .renderer
            .draw_cursor(&mut backend, snapshot.cursor_ball_size_idx);

        let _ = self.renderer.finish_frame(&mut backend);

        let cache = backend.into_cache();
        self.renderer.put_cache(cache);

        // CRITICAL: Always call end_frame() to drop SurfaceOutput before returning
        // Otherwise we get "SurfaceOutput must be dropped before new Surface" panic
        let _ = ctx.gfx.end_frame();

        // Mark as rendered in EventLoopState
        self.state.mark_rendered();

        Some(())
    }

    fn update_and_draw(&mut self, ctx: &mut Context) {
        // Tell the timer stuff a frame has happened
        ctx.time.tick();

        if self.state.render_drawing {
            self.state.physics.increment_render_count();

            // Skip most frames to reduce load
            // Physics runs in separate thread, we just render from snapshot
            self.render_from_snapshot(ctx);
        }

        // Update render FPS
        if let Some(fps) = self.state.physics.update_render_fps() {
            // FPS was updated, continue
            let _ = fps;
        }

        // Update energy data from snapshot - only once per simulation step
        if let Some(snapshot) = self.state.physics.get_latest_snapshot() {
            // Only update if this is a new simulation step
            if snapshot.simulation_step > self.last_processed_step {
                // Use actual physics FPS from snapshot (measured in physics thread)
                // This is the true simulation performance, not the snapshot observation rate
                self.simulation_fps = snapshot.physics_fps;
                self.last_processed_step = snapshot.simulation_step;

                // Use shared energy manager for data updates
                self.energy_manager.update_from_snapshot(
                    &snapshot,
                    self.state.physics.render_fps(),
                    self.state.physics.steps_per_frame(),
                );
            }
        }

        // Handle +/- keys for time step control in threaded mode
        // Note: Use time-based cooldown to prevent rapid-fire commands
        if self.state.input.can_process_plus_minus() {
            let is_alt_pressed = self.state.input.is_alt_modifier_pressed();

            if is_alt_pressed && self.state.input.is_key_pressed(KeyCode::Minus) {
                self.state
                    .physics
                    .send_command(PhysicsCommand::IncreaseWaitTime);
                self.state.input.mark_plus_minus_processed();
            } else if is_alt_pressed && self.state.input.is_key_pressed(KeyCode::Equals) {
                self.state
                    .physics
                    .send_command(PhysicsCommand::DecreaseWaitTime);
                self.state.input.mark_plus_minus_processed();
            } else if !is_alt_pressed && self.state.input.is_key_pressed(KeyCode::Minus) {
                self.state
                    .physics
                    .send_command(PhysicsCommand::HalveTimeStep);
                self.state.input.mark_plus_minus_processed();
            } else if !is_alt_pressed && self.state.input.is_key_pressed(KeyCode::Equals) {
                self.state
                    .physics
                    .send_command(PhysicsCommand::DoubleTimeStep);
                self.state.input.mark_plus_minus_processed();
            }
        }

        // Reset states for next frame (common to both paths)
        ctx.mouse.reset_delta();
        ctx.keyboard.save_keyboard_state();
        ctx.mouse.save_mouse_state();

        ggez::timer::yield_now();
    }

    fn handle_resize(&mut self, _ctx: &mut Context, width: f32, height: f32) -> ggez::GameResult {
        self.state.view_dirty = true;
        // Always resize the renderer (it's on the render thread)
        self.renderer.resize(width, height);

        // Get the updated world_rect from renderer's WindowDefinition
        let world_rect = self.renderer.get_world_region();

        // Send resize command to physics thread
        self.state
            .physics
            .send_command(PhysicsCommand::Resize { world_rect });
        Ok(())
    }
}
