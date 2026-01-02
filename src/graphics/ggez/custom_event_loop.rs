use crate::application::physics_thread::{PhysicsCommand, PhysicsThread};
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::application::{LogStuff, MainContent};
use crate::graphics::energy_plot::{EnergyData, EnergyPlotWindow, SharedEnergyData};
use crate::graphics::ggez::ggez_renderer::GgezRenderer;
use crate::graphics::ggez::ggez_renderer_backend::GgezBackend;
use crate::graphics::renderer::Renderer;
use crate::objects2d::Vec2D;
use arc_swap::ArcSwapOption;
use ggez::Context;
use ggez::event::winit_event::{ElementState, Event, KeyboardInput, MouseScrollDelta, WindowEvent};
use ggez::event::{MouseButton, process_event};
use ggez::input::keyboard::{KeyCode, KeyInput, KeyMods};
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use winit::event_loop::{ControlFlow, EventLoop};

pub struct CustomEventLoop {
    physics_thread: Option<PhysicsThread>,
    command_sender: Option<Sender<PhysicsCommand>>,
    snapshot_slot: Option<Arc<ArcSwapOption<RendererSnapshot>>>,
    last_processed_step: u64,
    last_step_time: std::time::Instant,
    simulation_fps: f32,
    last_displayed_step: u64,
    steps_per_frame: f32,
    pressed_keys: HashSet<KeyCode>,
    pressed_alt_keys: HashSet<KeyCode>,
    renderer: GgezRenderer,
    cursor_pos: Vec2D,
    energy_window: Option<EnergyPlotWindow>,

    // SHARED between both:
    energy_data: SharedEnergyData,

    // Pan state
    is_panning: bool,
    pan_start_pos_px: [f32; 2], // Cursor position in pixels when pan started
    pan_start_offset: Vec2D,

    // Controls whether frame rendering is currently enabled (toggled via keyboard, e.g. F2)
    render_drawing: bool,
}

impl CustomEventLoop {
    /// Create a new custom event loop with physics thread and renderer.
    ///
    /// # Arguments
    /// * `application` - MainContent with simulation state (will be moved to physics thread)
    /// * `renderer` - Renderer for visualization
    pub fn new(application: MainContent, renderer: GgezRenderer) -> Self {
        let energy_data = Arc::new(Mutex::new(EnergyData::new()));

        // Get snapshot slot before moving MainContent to physics thread
        let snapshot_slot = application.get_snapshot_slot();

        // Spawn physics thread (this will print its thread ID)
        let physics_thread = PhysicsThread::spawn(application);

        // Get command sender for forwarding user input
        let command_sender = physics_thread.get_command_sender();

        Self {
            physics_thread: Some(physics_thread),
            command_sender: Some(command_sender),
            snapshot_slot: Some(snapshot_slot),
            last_processed_step: 0,
            last_step_time: std::time::Instant::now(),
            simulation_fps: 0.0,
            last_displayed_step: 0,
            steps_per_frame: 0.0,
            pressed_keys: HashSet::new(),
            pressed_alt_keys: HashSet::new(),
            renderer,
            cursor_pos: Vec2D::zero(),
            energy_window: None,
            energy_data,
            is_panning: false,
            pan_start_pos_px: [0.0, 0.0],
            pan_start_offset: Vec2D::zero(),
            render_drawing: true,
        }
    }

    pub fn register_key_pressed(&mut self, key: KeyCode) {
        self.pressed_keys.insert(key);
    }

    pub fn register_key_released(&mut self, key: KeyCode) {
        self.pressed_keys.remove(&key);
    }

    pub fn register_alt_key_pressed(&mut self, key: KeyCode) {
        self.pressed_alt_keys.insert(key);
    }
    pub fn register_alt_key_released(&mut self, key: KeyCode) {
        self.pressed_alt_keys.remove(&key);
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    pub fn is_alt_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_alt_keys.contains(&key)
    }

    /// Send a command to the physics thread if in threaded mode
    /// Returns true if command was sent, false if in legacy mode
    fn send_physics_command(&self, command: crate::application::physics_thread::PhysicsCommand) {
        if let Some(sender) = &self.command_sender {
            let _ = sender.send(command);
        }
    }

    fn toggle_energy_window(
        &mut self,
        window_target: &winit::event_loop::EventLoopWindowTarget<()>,
    ) {
        if self.energy_window.is_none() {
            // Create and open energy window with default mode
            let mut window = EnergyPlotWindow::new(
                self.energy_data.clone(),
                crate::graphics::energy_plot::DisplayMode::default(),
            );
            if let Err(e) = window.create(window_target) {
                eprintln!("Failed to create energy window: {}", e);
                return;
            }
            self.energy_window = Some(window);
        } else if let Some(window) = &mut self.energy_window {
            if window.is_open() {
                // Cycle display mode instead of closing
                window.cycle_display_mode();
                window.request_redraw();
            } else {
                // Reopen if closed with last mode
                let last_mode = window.get_display_mode();
                let mut new_window = EnergyPlotWindow::new(self.energy_data.clone(), last_mode);
                if let Err(e) = new_window.create(window_target) {
                    eprintln!("Failed to create energy window: {}", e);
                    return;
                }
                self.energy_window = Some(new_window);
            }
        }
    }

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
                    if let Some(energy_window) = &mut self.energy_window
                        && energy_window.window_id() == Some(window_id)
                    {
                        self.handle_energy_window_event(event);
                        return;
                    }
                    // Otherwise, handle as main window event
                    self.handle_window_event(&mut ctx, event, control_flow, window_target);
                }
                Event::RedrawRequested(window_id) => {
                    // Check if redraw is for energy window
                    if let Some(energy_window) = &mut self.energy_window
                        && energy_window.window_id() == Some(window_id)
                    {
                        let _ = energy_window.render();
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

    fn handle_energy_window_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                // Close the energy window
                self.energy_window = None;
            }
            WindowEvent::Resized(size) => {
                if let Some(window) = &mut self.energy_window {
                    window.handle_resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode: Some(keycode),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                match keycode {
                    KeyCode::Escape => {
                        // Close energy window on Escape
                        self.energy_window = None;
                    }
                    KeyCode::Tab => {
                        // Cycle display mode on Tab
                        if let Some(window) = &mut self.energy_window {
                            window.cycle_display_mode();
                            window.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
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
                        state,
                        ..
                    },
                ..
            } => {
                let key_input = KeyInput {
                    keycode: Some(keycode),
                    scancode: 0,
                    mods: KeyMods::empty(),
                };

                match state {
                    ElementState::Pressed => {
                        let _ = self.handle_key_down(ctx, key_input, false);
                    }
                    ElementState::Released => {
                        let _ = self.handle_key_up(ctx, key_input, window_target);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pos = ctx.mouse.position();
                match state {
                    ElementState::Pressed => {
                        let _ = self.handle_mouse_button_down(ctx, button, pos.x, pos.y);
                    }
                    ElementState::Released => {
                        let _ = self.handle_mouse_button_up(ctx, button, pos.x, pos.y);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                self.handle_mouse_motion(ctx, x, y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(ctx, delta);
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
        let snapshot_slot = self.snapshot_slot.as_ref()?;

        // PHASE 4: Load snapshot BEFORE expensive rendering work
        let snapshot_guard_before = snapshot_slot.load();
        let snapshot_arc_before = snapshot_guard_before.as_ref()?.clone();
        let snapshot = snapshot_arc_before.as_ref();

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
            .draw_balls_from_snapshot(&mut backend, snapshot);

        // Draw cursor (on top of balls)
        let _ = self
            .renderer
            .draw_cursor(&mut backend, snapshot.cursor_ball_size_idx);

        let _ = self.renderer.finish_frame(&mut backend);

        let cache = backend.into_cache();
        self.renderer.put_cache(cache);

        // PHASE 4: Verify snapshot hasn't changed before presenting
        let snapshot_guard_after = snapshot_slot.load();
        let is_stale = if let Some(ref snapshot_arc_after) = *snapshot_guard_after {
            !Arc::ptr_eq(&snapshot_arc_before, snapshot_arc_after)
        } else {
            // If the snapshot disappears during rendering (very rare), we do NOT treat it as stale.
            // This preserves legacy behavior: a None snapshot does not trigger early return,
            // and the application will still be presented. See CodeQL nitpick for rationale.
            false
        };

        // CRITICAL: Always call end_frame() to drop SurfaceOutput before returning
        // Otherwise we get "SurfaceOutput must be dropped before new Surface" panic
        let _ = ctx.gfx.end_frame();

        if is_stale {
            // Newer snapshot available - application was rendered but not presented (skipped)
            return None;
        }

        // Track steps per application for performance metrics
        let steps_this_frame = snapshot
            .simulation_step
            .saturating_sub(self.last_displayed_step);
        if steps_this_frame > 0 {
            // Smooth with exponential moving average (weight=0.1)
            if self.steps_per_frame == 0.0 {
                self.steps_per_frame = steps_this_frame as f32;
            } else {
                self.steps_per_frame = self.steps_per_frame * 0.9 + steps_this_frame as f32 * 0.1;
            }
            self.last_displayed_step = snapshot.simulation_step;
        }

        Some(())
    }

    fn update_and_draw(&mut self, ctx: &mut Context) {
        // Tell the timer stuff a application has happened
        ctx.time.tick();

        if self.render_drawing {
            // Skip most frames to reduce load
            // Physics runs in separate thread, we just render from snapshot
            self.render_from_snapshot(ctx);
        }

        // Update energy data from snapshot - only once per simulation step
        if let Some(snapshot_slot) = &self.snapshot_slot
            && let Some(snapshot) = snapshot_slot.load().as_ref()
        {
            // Only update if this is a new simulation step
            if snapshot.simulation_step > self.last_processed_step {
                // Use actual physics FPS from snapshot (measured in physics thread)
                // This is the true simulation performance, not the snapshot observation rate
                self.simulation_fps = snapshot.physics_fps;
                self.last_processed_step = snapshot.simulation_step;

                let mut energy_data = self.energy_data.lock().unwrap();
                energy_data.v_kin.push_back(snapshot.kinetic_energy);
                energy_data.v_pot.push_back(snapshot.potential_energy);
                energy_data.time = snapshot.time;

                // Update status text from snapshot (with renderer FPS, simulation FPS, and steps/frame)
                let renderer_fps = ctx.time.fps();
                if let Some(ref status) = snapshot.status_text {
                    // Replace "fps 0" placeholder with combined renderer, simulation FPS, and steps/frame
                    let updated_status = status.replace(
                        "fps 0",
                        &format!(
                            "render: {} fps | sim: {:.0} fps | steps/frame: {:.1}",
                            renderer_fps as u32, self.simulation_fps, self.steps_per_frame
                        ),
                    );
                    energy_data.status_text = Some(updated_status);
                } else {
                    energy_data.status_text = None;
                }

                // Limit buffer size to prevent unbounded growth
                const MAX_ENERGY_SAMPLES: usize = 1000;
                if energy_data.v_kin.len() > MAX_ENERGY_SAMPLES {
                    energy_data.v_kin.pop_front();
                }
                if energy_data.v_pot.len() > MAX_ENERGY_SAMPLES {
                    energy_data.v_pot.pop_front();
                }

                // Release the lock before requesting redraw
                drop(energy_data);

                // Request redraw for energy window if it exists and new data was added
                if let Some(window) = &self.energy_window {
                    window.request_redraw();
                }
            }
        }

        // Handle +/- keys for time step control in threaded mode
        if self.is_alt_key_pressed(KeyCode::Minus) {
            self.send_physics_command(PhysicsCommand::IncreaseWaitTime);
        } else if self.is_alt_key_pressed(KeyCode::Equals) {
            self.send_physics_command(PhysicsCommand::DecreaseWaitTime);
        } else if self.is_key_pressed(KeyCode::Minus) {
            self.send_physics_command(PhysicsCommand::HalveTimeStep);
        } else if self.is_key_pressed(KeyCode::Equals) {
            self.send_physics_command(PhysicsCommand::DoubleTimeStep);
        }

        // Reset states for next application (common to both paths)
        ctx.mouse.reset_delta();
        ctx.keyboard.save_keyboard_state();
        ctx.mouse.save_mouse_state();

        ggez::timer::yield_now();
    }

    fn handle_key_down(
        &mut self,
        ctx: &mut Context,
        input: KeyInput,
        repeated: bool,
    ) -> ggez::GameResult {
        if let Some(key) = input.keycode {
            match key {
                KeyCode::Space => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::TogglePause,
                ),
                KeyCode::LControl => self.renderer.enable_drawing_cursor(repeated),
                KeyCode::Escape => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::ClearBalls,
                ),
                KeyCode::LShift => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::EnableRandomBallSpeed,
                ),
                KeyCode::Minus | KeyCode::Equals => {
                    if ctx.keyboard.is_key_pressed(KeyCode::LAlt)
                        || ctx.keyboard.is_key_pressed(KeyCode::RAlt)
                    {
                        self.register_alt_key_pressed(key);
                    } else {
                        self.register_key_pressed(key);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_key_up(
        &mut self,
        _ctx: &mut Context,
        input: KeyInput,
        window_target: &winit::event_loop::EventLoopWindowTarget<()>,
    ) -> ggez::GameResult {
        if let Some(key) = input.keycode {
            match key {
                KeyCode::Tab => self.toggle_energy_window(window_target),
                KeyCode::F2 => {
                    self.render_drawing = !self.render_drawing;
                    println!(
                        "============\nRender drawing toggled to {}",
                        self.render_drawing
                    );
                }
                KeyCode::H => MainContent::print_help_message(),
                KeyCode::D => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::ToggleRealData,
                ),
                KeyCode::S => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::ToggleStepwise,
                ),
                KeyCode::T => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::ToggleTimingDisplay,
                ),
                KeyCode::P => self.renderer.cycle_plot_content(),
                KeyCode::Equals | KeyCode::Minus => {
                    self.register_key_released(key);
                    self.register_alt_key_released(key);
                }
                KeyCode::R => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::RandomizeSpeeds,
                ),
                KeyCode::B => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::ToggleBallStats,
                ),
                KeyCode::V => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::StartVerbosityInput,
                ),
                KeyCode::LControl => {
                    let cursor_pos = self.cursor_pos.clone();
                    self.send_physics_command(
                        crate::application::physics_thread::PhysicsCommand::AddBall(
                            cursor_pos.clone(),
                        ),
                    );
                    self.renderer.disable_drawing_cursor();
                }
                KeyCode::F5 => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::AddRandomBalls(100),
                ),
                KeyCode::F1 => self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::StopAllBalls,
                ),
                KeyCode::Key0 | KeyCode::Numpad0 => {
                    // Reset viewport to default view (1.0x zoom, centered)
                    self.renderer.viewport_mut().reset();
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_mouse_button_down(
        &mut self,
        _ctx: &mut Context,
        button: MouseButton,
        x: f32,
        y: f32,
    ) -> ggez::GameResult {
        match button {
            MouseButton::Right => {
                self.send_physics_command(
                    crate::application::physics_thread::PhysicsCommand::NextBallSize,
                );
            }
            MouseButton::Middle => {
                // Start panning - store initial cursor position (in pixels) and pan offset
                self.is_panning = true;
                self.pan_start_pos_px = [x, y];
                self.pan_start_offset = self.renderer.viewport().pan_offset().clone();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_mouse_button_up(
        &mut self,
        _ctx: &mut Context,
        button: MouseButton,
        _x: f32,
        _y: f32,
    ) -> ggez::GameResult {
        if button == MouseButton::Middle {
            // Stop panning
            self.is_panning = false;
        }
        Ok(())
    }

    fn handle_mouse_motion(&mut self, _ctx: &mut Context, x: f32, y: f32) {
        let new_cursor_pos = self.renderer.update_cursor_and_text(x, y);

        // Handle panning if middle mouse button is down
        if self.is_panning {
            // Calculate cursor delta in pixel space (avoids jitter from viewport transformation)
            let cursor_delta_px =
                Vec2D::new(x - self.pan_start_pos_px[0], y - self.pan_start_pos_px[1]);

            // Convert pixel delta to world space delta
            // Viewport transformation: world_delta = pixel_delta / (LEN_TO_PX * zoom)
            let zoom = self.renderer.viewport().zoom();
            let world_delta = Vec2D::new(
                crate::graphics::window::window_definition::WindowDefinition::px_to_len(
                    cursor_delta_px.x,
                ) / zoom,
                crate::graphics::window::window_definition::WindowDefinition::px_to_len(
                    cursor_delta_px.y,
                ) / zoom,
            );

            // Invert delta because panning moves the viewport in opposite direction
            // Y is inverted only in X direction; Y already accounts for screen coordinate inversion
            let pan_delta = Vec2D::new(-world_delta.x, world_delta.y);
            // Apply delta to the initial pan offset
            let new_pan = &self.pan_start_offset + &pan_delta;
            self.renderer.viewport_mut().set_pan(new_pan);
        }

        self.cursor_pos = new_cursor_pos;
    }

    fn handle_mouse_wheel(&mut self, _ctx: &mut Context, delta: MouseScrollDelta) {
        // Calculate zoom factor from scroll delta
        let zoom_factor = match delta {
            MouseScrollDelta::LineDelta(_x, y) => {
                // Each line is approximately one "click" of the scroll wheel
                // Zoom in/out by 10% per click
                if y.abs() > 0.001 {
                    1.1_f32.powf(y)
                } else {
                    1.0
                }
            }
            MouseScrollDelta::PixelDelta(pos) => {
                // For pixel-based scrolling (touchpads), scale appropriately
                let y = pos.y as f32;
                if y.abs() > 0.1 {
                    1.0 + y * 0.002 // Smaller factor for smoother touchpad scrolling
                } else {
                    1.0
                }
            }
        };

        // Zoom at cursor position
        if (zoom_factor - 1.0).abs() > 0.001 {
            self.renderer
                .viewport_mut()
                .zoom_at_point(&self.cursor_pos, zoom_factor);
        }
    }

    fn handle_resize(&mut self, _ctx: &mut Context, width: f32, height: f32) -> ggez::GameResult {
        // Always resize the renderer (it's on the render thread)
        self.renderer.resize(width, height);

        // Get the updated world_rect from renderer's WindowDefinition
        let world_rect = self.renderer.get_world_region();

        // Send resize command to physics thread or resize application directly
        self.send_physics_command(crate::application::physics_thread::PhysicsCommand::Resize {
            world_rect,
        });
        Ok(())
    }
}
