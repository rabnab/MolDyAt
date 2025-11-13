use crate::frame::application_master::MainContent;
use crate::frame::energy_plot::{EnergyPlotWindow, SharedEnergyData, EnergyData};
use crate::frame::ggez::ggez_renderer::GgezRenderer;
use crate::frame::ggez::ggez_renderer_backend::GgezBackend;
use crate::frame::renderer::Renderer;
use crate::frame::{GameLogic, LogStuff};
use crate::objects2d::Vec2D;
use ggez::event::winit_event::{ElementState, Event, KeyboardInput, WindowEvent};
use ggez::event::{MouseButton, process_event};
use ggez::input::keyboard::{KeyCode, KeyInput, KeyMods};
use ggez::Context;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use winit::event_loop::{ControlFlow, EventLoop};

pub struct CustomEventLoop {
    pub(crate) application: MainContent,
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

    pub fn run(mut self, mut ctx: Context, event_loop: EventLoop<()>) {
        event_loop.run(move |mut event, _window_target, control_flow| {
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
                Event::WindowEvent { event, .. } => {
                    self.handle_window_event(&mut ctx, event, control_flow);
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
                        let _ = self.handle_key_up(ctx, key_input);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let ElementState::Pressed = state {
                    let pos = ctx.mouse.position();
                    let _ = self.handle_mouse_button_down(ctx, button, pos.x, pos.y);
                }
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

    fn update_and_draw(&mut self, ctx: &mut Context) {
        // Tell the timer stuff a frame has happened
        ctx.time.tick();

        // Update
        self.application.unpause_after_resize();
        self.application.start_frame();

        while ctx.time.check_update_time(
            crate::frame::application_master::FPS_GOAL * self.application.get_speedup(),
        ) {
            let _ = self.application.simulation_step();
        }

        self.application.end_frame();

        if self.is_key_pressed(KeyCode::Minus) {
            self.application.decrease_speedup();
        } else if self.is_key_pressed(KeyCode::Equals) {
            self.application.increase_speedup();
        }

        // Draw
        let _ = ctx.gfx.begin_frame();

        let fps = ctx.time.fps();
        let cell_grid = &self.application.cell_grid;

        let mut cur_render = self.renderer.borrow_mut();
        let cache = cur_render.take_cache();
        let mut backend = GgezBackend::new(ctx, cache);

        let status_text = self.application.status_text(fps as u32);
        cur_render.set_physics_engine(self.application.physics.clone());
        let _ = cur_render.draw_balls(&mut backend, self.application.get_balls());
        let _ = cur_render.draw_frame(&mut backend, cell_grid, status_text);
        let _ = cur_render.draw_cursor(&mut backend, self.application.get_ball_factory().get_new_ball_size_idx());
        let _ = cur_render.finish_frame(&mut backend);

        let cache = backend.into_cache();
        cur_render.put_cache(cache);

        let _ = ctx.gfx.end_frame();

        // Reset states for next frame
        ctx.mouse.reset_delta();
        ctx.keyboard.save_keyboard_state();
        ctx.mouse.save_mouse_state();

        ggez::timer::yield_now();
    }

    fn handle_key_down(&mut self, _ctx: &mut Context, input: KeyInput, repeated: bool) -> ggez::GameResult {
        if let Some(key) = input.keycode {
            match key {
                KeyCode::Space => self.application.toggle_pause(),
                KeyCode::LControl => self.renderer.borrow_mut().enable_drawing_cursor(repeated),
                KeyCode::Escape => self.application.clear_points(),
                KeyCode::LShift => self.application.enable_random_ball_speed(),
                KeyCode::Minus | KeyCode::Equals => {
                    self.register_key_pressed(key);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_key_up(&mut self, _ctx: &mut Context, input: KeyInput) -> ggez::GameResult {
        if let Some(key) = input.keycode {
            match key {
                KeyCode::H => MainContent::print_help_message(),
                KeyCode::D => self.application.toggle_real_data(),
                KeyCode::S => self.application.toggle_stepwise(),
                KeyCode::T => self.application.toggle_timing_display(),
                KeyCode::P => self.renderer.borrow_mut().cycle_plot_content(),
                KeyCode::Equals | KeyCode::Minus => {
                    self.register_key_released(key);
                }
                KeyCode::R => self.application.randomize_speeds(),
                KeyCode::B => self.application.toggle_show_ball_stats(),
                KeyCode::V => self.application.start_handling_verbosity_input(),
                KeyCode::LControl => {
                    let cursor_pos = self.cursor_pos.clone();
                    let _ = self.application.add_candidate(cursor_pos);
                    self.renderer.borrow_mut().disable_drawing_cursor();
                }
                KeyCode::F5 => self.application.add_random_candidates(100),
                KeyCode::F1 => self.application.stop_all_balls(),
                KeyCode::F2 => {
                    let mut small = self.application.get_game_logic_data().get_ball_factory().create_basic(Vec2D::zero(), 2);
                    let mut large = self.application.get_game_logic_data().get_ball_factory().create_basic(Vec2D::zero(), 4);
                    small.v = Vec2D::new(0.1, 0.);
                    large.v = Vec2D::new(0., 0.1);
                    self.application.add_candidate_to_vec(small);
                    self.application.add_candidate_to_vec(large);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_mouse_button_down(&mut self, _ctx: &mut Context, button: MouseButton, _x: f32, _y: f32) -> ggez::GameResult {
        if let MouseButton::Right = button {
            self.application.next_ball_size();
        }
        Ok(())
    }

    fn handle_mouse_motion(&mut self, _ctx: &mut Context, x: f32, y: f32) {
        self.cursor_pos = self.renderer.borrow_mut().update_cursor_and_text(x, y);
    }

    fn handle_resize(&mut self, _ctx: &mut Context, width: f32, height: f32) -> ggez::GameResult {
        self.application.resize(width, height);
        Ok(())
    }
}
