use crate::frame::application_master::MainContent;
use crate::frame::energy_plot::{EnergyData, EnergyPlotWindow, SharedEnergyData};
use crate::frame::ggez::ggez_renderer::GgezRenderer;
use crate::frame::ggez::ggez_renderer_backend::GgezBackend;
use crate::frame::physics_thread::{PhysicsCommand, PhysicsThread};
use crate::frame::renderer::Renderer;
use crate::frame::renderer_snapshot::RendererSnapshot;
use crate::frame::{GameLogic, LogStuff};
use crate::objects2d::Vec2D;
use arc_swap::ArcSwapOption;
use ggez::Context;
use ggez::event::winit_event::{ElementState, Event, KeyboardInput, WindowEvent};
use ggez::event::{MouseButton, process_event};
use ggez::input::keyboard::{KeyCode, KeyInput, KeyMods};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use winit::event_loop::{ControlFlow, EventLoop};

pub struct CustomEventLoop {
    // OLD ARCHITECTURE (will be removed):
    pub(crate) application: MainContent,

    // NEW ARCHITECTURE (being added):
    physics_thread: Option<PhysicsThread>,
    command_sender: Option<Sender<PhysicsCommand>>,
    snapshot_slot: Option<Arc<ArcSwapOption<RendererSnapshot>>>,
    last_processed_step: u64,
    last_step_time: std::time::Instant,
    simulation_fps: f32,

    // SHARED between both:
    pressed_keys: HashSet<KeyCode>,
    renderer: Rc<RefCell<GgezRenderer>>,
    cursor_pos: Vec2D,
    energy_window: Option<EnergyPlotWindow>,
    energy_data: SharedEnergyData,
}

impl CustomEventLoop {
    pub fn new(application: MainContent, renderer: Rc<RefCell<GgezRenderer>>) -> Self {
        let energy_data = Arc::new(Mutex::new(EnergyData::new()));
        Self {
            application,
            physics_thread: None,
            command_sender: None,
            snapshot_slot: None,
            last_processed_step: 0,
            last_step_time: std::time::Instant::now(),
            simulation_fps: 0.0,
            pressed_keys: HashSet::new(),
            renderer,
            cursor_pos: Vec2D::zero(),
            energy_window: None,
            energy_data,
        }
    }

    /// New constructor that uses physics thread architecture
    /// This will eventually replace `new()`
    #[allow(dead_code)]
    pub fn new_with_physics_thread(
        application: MainContent,
        renderer: Rc<RefCell<GgezRenderer>>,
    ) -> Self {
        let energy_data = Arc::new(Mutex::new(EnergyData::new()));

        // Get snapshot slot before moving MainContent to physics thread
        let snapshot_slot = application.get_snapshot_slot();

        // Spawn physics thread (this will print its thread ID)
        let physics_thread = PhysicsThread::spawn(application);

        // Get command sender for forwarding user input
        let command_sender = physics_thread.get_command_sender();

        // Create a dummy MainContent for compatibility during transition
        // TODO: Remove this once all references to self.application are eliminated
        let dummy_world = crate::objects2d::Rect::new(0.0, 0.0, 100.0, 100.0);
        let dummy_application =
            MainContent::new(dummy_world).expect("Failed to create dummy MainContent");

        Self {
            application: dummy_application,
            physics_thread: Some(physics_thread),
            command_sender: Some(command_sender),
            snapshot_slot: Some(snapshot_slot),
            last_processed_step: 0,
            last_step_time: std::time::Instant::now(),
            simulation_fps: 0.0,
            pressed_keys: HashSet::new(),
            renderer,
            cursor_pos: Vec2D::zero(),
            energy_window: None,
            energy_data,
        }
    }

    pub fn register_key_pressed(&mut self, key: KeyCode) {
        self.pressed_keys.insert(key);
    }

    pub fn register_key_released(&mut self, key: KeyCode) {
        self.pressed_keys.remove(&key);
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    /// Send a command to the physics thread if in threaded mode
    /// Returns true if command was sent, false if in legacy mode
    fn send_physics_command(&self, command: crate::frame::physics_thread::PhysicsCommand) -> bool {
        if let Some(sender) = &self.command_sender {
            let _ = sender.send(command);
            true
        } else {
            false
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
                crate::frame::energy_plot::DisplayMode::default(),
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
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                let pos = ctx.mouse.position();
                let _ = self.handle_mouse_button_down(ctx, button, pos.x, pos.y);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                self.handle_mouse_motion(ctx, x, y);
            }
            WindowEvent::Resized(size) => {
                let _ = self.handle_resize(ctx, size.width as f32, size.height as f32);
            }
            _ => {}
        }
    }

    /// Render from snapshot (new architecture)
    /// This method reads the latest snapshot and renders it without accessing MainContent
    #[allow(dead_code)]
    fn render_from_snapshot(&mut self, ctx: &mut Context) -> Option<()> {
        let snapshot_slot = self.snapshot_slot.as_ref()?;
        let snapshot = snapshot_slot.load();
        let snapshot = snapshot.as_ref()?;

        // Update dummy application's cell grid if bounds changed
        // This ensures the grid rendering matches the actual simulation
        let current_world_rect = self.application.world_region();
        let bounds_changed = current_world_rect != snapshot.cell_grid_bounds;

        if bounds_changed {
            // Rebuild the dummy application's cell grid with correct bounds
            self.application.resize(snapshot.cell_grid_bounds);
        }

        let _ = ctx.gfx.begin_frame();

        let mut cur_render = self.renderer.borrow_mut();
        let cache = cur_render.take_cache();
        let mut backend = GgezBackend::new(ctx, cache);

        // Draw balls from snapshot
        let _ = cur_render.draw_balls_from_snapshot(&mut backend, snapshot);

        // Draw cell grid and frame (read-only access to application is safe)
        // Cell grid dimensions now match the actual simulation
        let cell_grid = &self.application.cell_grid;
        let _ = cur_render.draw_frame(&mut backend, cell_grid, None);

        // Draw cursor
        let _ = cur_render.draw_cursor(&mut backend, snapshot.cursor_ball_size_idx);

        let _ = cur_render.finish_frame(&mut backend);

        let cache = backend.into_cache();
        cur_render.put_cache(cache);

        let _ = ctx.gfx.end_frame();

        Some(())
    }

    fn update_and_draw(&mut self, ctx: &mut Context) {
        // Tell the timer stuff a frame has happened
        ctx.time.tick();

        // Check if we're in threaded mode (snapshot_slot is Some)
        if self.snapshot_slot.is_some() {
            // NEW PATH: Threaded physics with snapshot-based rendering
            // Physics runs in separate thread, we just render from snapshot
            self.render_from_snapshot(ctx);

            // Update energy data from snapshot - only once per simulation step
            if let Some(snapshot_slot) = &self.snapshot_slot
                && let Some(snapshot) = snapshot_slot.load().as_ref()
            {
                // Only update if this is a new simulation step
                if snapshot.simulation_step > self.last_processed_step {
                    // Calculate simulation FPS
                    let now = std::time::Instant::now();
                    let elapsed = now.duration_since(self.last_step_time).as_secs_f32();
                    if elapsed > 0.0 {
                        // Smooth FPS with exponential moving average (weight=0.1)
                        let instant_fps = 1.0 / elapsed;
                        self.simulation_fps = if self.simulation_fps == 0.0 {
                            instant_fps
                        } else {
                            self.simulation_fps * 0.9 + instant_fps * 0.1
                        };
                    }
                    self.last_step_time = now;
                    self.last_processed_step = snapshot.simulation_step;

                    let mut energy_data = self.energy_data.lock().unwrap();
                    energy_data.v_kin.push_back(snapshot.kinetic_energy);
                    energy_data.v_pot.push_back(snapshot.potential_energy);
                    energy_data.time = snapshot.time;

                    // Update status text from snapshot (with renderer FPS and simulation FPS)
                    let renderer_fps = ctx.time.fps();
                    if let Some(ref status) = snapshot.status_text {
                        // Replace "fps 0" placeholder with combined renderer and simulation FPS
                        let updated_status = status.replace(
                            "fps 0",
                            &format!(
                                "render: {} fps | sim: {:.0} fps",
                                renderer_fps as u32, self.simulation_fps
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
            if self.is_key_pressed(KeyCode::Minus) {
                let _ = self.send_physics_command(
                    crate::frame::physics_thread::PhysicsCommand::HalveTimeStep,
                );
            } else if self.is_key_pressed(KeyCode::Equals) {
                let _ = self.send_physics_command(
                    crate::frame::physics_thread::PhysicsCommand::DoubleTimeStep,
                );
            }
        } else {
            // LEGACY PATH: Direct MainContent access
            // Update
            self.application.unpause_after_resize();
            self.application.start_frame();

            while ctx.time.check_update_time(
                crate::frame::application_master::FPS_GOAL * self.application.get_speedup(),
            ) {
                let _ = self.application.simulation_step();
            }

            self.application.end_frame();

            // Update energy data for the energy plot window
            {
                let mut energy_data = self.energy_data.lock().unwrap();
                energy_data
                    .v_kin
                    .push_back(self.application.sim.get_kinetic_energy());
                energy_data
                    .v_pot
                    .push_back(self.application.sim.get_potential_energy());
                energy_data.time = self.application.sim.get_time();

                // Update status text for the energy window
                let fps = ctx.time.fps();
                energy_data.status_text = self
                    .application
                    .status_text(fps as u32, self.application.get_time_step());

                // Limit buffer size to prevent unbounded growth
                const MAX_ENERGY_SAMPLES: usize = 1000;
                if energy_data.v_kin.len() > MAX_ENERGY_SAMPLES {
                    energy_data.v_kin.pop_front();
                }
                if energy_data.v_pot.len() > MAX_ENERGY_SAMPLES {
                    energy_data.v_pot.pop_front();
                }
            }

            // Request redraw for energy window if it exists
            if let Some(window) = &self.energy_window {
                window.request_redraw();
            }

            if self.is_key_pressed(KeyCode::Minus)
                && !self.send_physics_command(
                    crate::frame::physics_thread::PhysicsCommand::HalveTimeStep,
                )
            {
                self.application.halve_time_step();
            } else if self.is_key_pressed(KeyCode::Equals)
                && !self.send_physics_command(
                    crate::frame::physics_thread::PhysicsCommand::DoubleTimeStep,
                )
            {
                self.application.double_time_step();
            }

            // Draw main window (without status text - now shown in energy window)
            let _ = ctx.gfx.begin_frame();

            let cell_grid = &self.application.cell_grid;

            let mut cur_render = self.renderer.borrow_mut();
            let cache = cur_render.take_cache();
            let mut backend = GgezBackend::new(ctx, cache);

            // Don't pass status_text - it's now in the energy window for better performance
            cur_render.set_physics_engine(self.application.physics.clone());
            let _ = cur_render.draw_balls(&mut backend, self.application.get_balls());
            let _ = cur_render.draw_frame(&mut backend, cell_grid, None);
            let _ = cur_render.draw_cursor(
                &mut backend,
                self.application.get_ball_factory().get_new_ball_size_idx(),
            );
            let _ = cur_render.finish_frame(&mut backend);

            let cache = backend.into_cache();
            cur_render.put_cache(cache);

            let _ = ctx.gfx.end_frame();
        }

        // Reset states for next frame (common to both paths)
        ctx.mouse.reset_delta();
        ctx.keyboard.save_keyboard_state();
        ctx.mouse.save_mouse_state();

        ggez::timer::yield_now();
    }

    fn handle_key_down(
        &mut self,
        _ctx: &mut Context,
        input: KeyInput,
        repeated: bool,
    ) -> ggez::GameResult {
        if let Some(key) = input.keycode {
            match key {
                KeyCode::Space => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::TogglePause,
                    ) {
                        self.application.toggle_pause();
                    }
                }
                KeyCode::LControl => self.renderer.borrow_mut().enable_drawing_cursor(repeated),
                KeyCode::Escape => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::ClearBalls,
                    ) {
                        self.application.clear_points();
                    }
                }
                KeyCode::LShift => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::EnableRandomBallSpeed,
                    ) {
                        self.application.enable_random_ball_speed();
                    }
                }
                KeyCode::Minus | KeyCode::Equals => {
                    self.register_key_pressed(key);
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
                KeyCode::H => MainContent::print_help_message(),
                KeyCode::D => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::ToggleRealData,
                    ) {
                        self.application.toggle_real_data();
                    }
                }
                KeyCode::S => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::ToggleStepwise,
                    ) {
                        self.application.toggle_stepwise();
                    }
                }
                KeyCode::T => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::ToggleTimingDisplay,
                    ) {
                        self.application.toggle_timing_display();
                    }
                }
                KeyCode::P => self.renderer.borrow_mut().cycle_plot_content(),
                KeyCode::Equals | KeyCode::Minus => {
                    self.register_key_released(key);
                }
                KeyCode::R => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::RandomizeSpeeds,
                    ) {
                        self.application.randomize_speeds();
                    }
                }
                KeyCode::B => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::ToggleBallStats,
                    ) {
                        self.application.toggle_show_ball_stats();
                    }
                }
                KeyCode::V => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::StartVerbosityInput,
                    ) {
                        self.application.start_handling_verbosity_input();
                    }
                }
                KeyCode::LControl => {
                    let cursor_pos = self.cursor_pos.clone();
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::AddBall(cursor_pos.clone()),
                    ) {
                        let _ = self.application.add_candidate(cursor_pos);
                    }
                    self.renderer.borrow_mut().disable_drawing_cursor();
                }
                KeyCode::F5 => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::AddRandomBalls(100),
                    ) {
                        self.application.add_random_candidates(100);
                    }
                }
                KeyCode::F1 => {
                    if !self.send_physics_command(
                        crate::frame::physics_thread::PhysicsCommand::StopAllBalls,
                    ) {
                        self.application.stop_all_balls();
                    }
                }
                KeyCode::F2 => {
                    // F2: Add two specific balls with velocities
                    // Note: In threaded mode, we can't easily create balls here
                    // since we don't have access to BallFactory. Skip for now.
                    if self.command_sender.is_none() {
                        let mut small = self
                            .application
                            .get_game_logic_data()
                            .get_ball_factory()
                            .create_basic(Vec2D::zero(), 2);
                        let mut large = self
                            .application
                            .get_game_logic_data()
                            .get_ball_factory()
                            .create_basic(Vec2D::zero(), 4);
                        small.v = Vec2D::new(0.1, 0.);
                        large.v = Vec2D::new(0., 0.1);
                        self.application.add_candidate_to_vec(small);
                        self.application.add_candidate_to_vec(large);
                    }
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
        _x: f32,
        _y: f32,
    ) -> ggez::GameResult {
        if let MouseButton::Right = button
            && !self
                .send_physics_command(crate::frame::physics_thread::PhysicsCommand::NextBallSize)
        {
            self.application.next_ball_size();
        }
        Ok(())
    }

    fn handle_mouse_motion(&mut self, _ctx: &mut Context, x: f32, y: f32) {
        self.cursor_pos = self.renderer.borrow_mut().update_cursor_and_text(x, y);
    }

    fn handle_resize(&mut self, _ctx: &mut Context, width: f32, height: f32) -> ggez::GameResult {
        // Always resize the renderer (it's on the render thread)
        self.renderer.borrow_mut().resize(width, height);

        // Get the updated world_rect from renderer's WindowDefinition
        let world_rect = self.renderer.borrow().get_world_region();

        // Send resize command to physics thread or resize application directly
        if !self.send_physics_command(crate::frame::physics_thread::PhysicsCommand::Resize {
            world_rect,
        }) {
            self.application.resize(world_rect);
        }
        Ok(())
    }
}
