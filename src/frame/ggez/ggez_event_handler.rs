use crate::frame::application_master::MainContent;
use crate::frame::ggez::ggez_renderer::GgezRenderer;
use crate::frame::ggez::ggez_renderer_backend::GgezBackend;
use crate::frame::renderer::Renderer;
use crate::frame::{GameLogic, LogStuff};
use crate::objects2d::Vec2D;
use ggez::event::{EventHandler, MouseButton};
use ggez::input::keyboard::{KeyCode, KeyInput};
use ggez::{Context, GameError, GameResult};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

pub(super) struct GgezEventHandler {
    application: MainContent,
    pressed_key: HashSet<KeyCode>,
    renderer: Rc<RefCell<GgezRenderer>>,
    cursor_pos: Vec2D,
}

impl GgezEventHandler {
    pub fn new(application: MainContent, renderer_ref: Rc<RefCell<GgezRenderer>>) -> Self {
        GgezEventHandler {
            application,
            pressed_key: HashSet::new(),
            renderer: renderer_ref,
            cursor_pos: Vec2D::zero(),
        }
    }

    pub(crate) fn register_key_pressed(&mut self, key: KeyCode) {
        self.pressed_key.insert(key);
    }

    pub(crate) fn register_key_released(&mut self, key: KeyCode) {
        self.pressed_key.remove(&key);
    }

    pub(crate) fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_key.contains(&key)
    }
}

impl EventHandler for GgezEventHandler {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        self.application.unpause_after_resize();

        self.application.start_frame();
        while ctx.time.check_update_time(
            crate::frame::application_master::FPS_GOAL * self.application.get_speedup(),
        ) {
            self.application.simulation_step()?;
        }
        self.application.end_frame();
        //sim.set_update_hood_cnt(get_update_hood_call_cnt() - tmp_cnt);
        if self.is_key_pressed(KeyCode::Minus) {
            self.application.decrease_speedup();
        } else if self.is_key_pressed(KeyCode::Equals) {
            self.application.increase_speedup();
        }
        // if cnt > 0 {
        //     println!("simulate {} avg {:?} simtime {}", cnt, sum/cnt, self.sim.time);
        // }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        let fps = ctx.time.fps();
        let cell_grid = &self.application.cell_grid;

        let mut cur_render = self.renderer.borrow_mut();

        let cache = cur_render.take_cache();

        let mut backend = GgezBackend::new(ctx, cache);

        let status_text = self.application.status_text(fps as u32);
        cur_render.set_physics_engine(self.application.physics.clone());
        cur_render.draw_balls(&mut backend, self.application.get_balls())?;
        cur_render.draw_frame(&mut backend, cell_grid, status_text)?;

        cur_render.draw_cursor(
            &mut backend,
            self.application.get_ball_factory().get_new_ball_size_idx(),
        )?;
        cur_render.finish_frame(&mut backend)?;

        let cache = backend.into_cache();
        cur_render.put_cache(cache);

        Ok(())
    }

    fn mouse_button_down_event(
        &mut self,
        _ctx: &mut Context,
        button: MouseButton,
        _x: f32,
        _y: f32,
    ) -> GameResult {
        if let MouseButton::Right = button {
            self.application.next_ball_size();
        }
        Ok(())
    }

    fn mouse_motion_event(
        &mut self,
        _ctx: &mut Context,
        x: f32,
        y: f32,
        _dx: f32,
        _dy: f32,
    ) -> GameResult {
        // Calculate clamped positions

        // Update cursor position
        self.cursor_pos = self.renderer.borrow_mut().update_cursor_and_text(x, y);
        Ok(())
    }

    fn key_down_event(
        &mut self,
        _context: &mut Context,
        input: KeyInput,
        repeated: bool,
    ) -> Result<(), GameError> {
        if let Some(key) = input.keycode {
            match key {
                KeyCode::Space => self.application.toggle_pause(),
                KeyCode::LControl => self.renderer.borrow_mut().enable_drawing_cursor(repeated),
                KeyCode::Escape => self.application.clear_points(),
                KeyCode::LShift => self.application.enable_random_ball_speed(),
                KeyCode::Minus | KeyCode::Equals => {
                    self.register_key_pressed(key);
                }
                _ => {} // explicit ignoring of other keys
            }
        }

        Ok(())
    }

    fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> Result<(), GameError> {
        match input.keycode {
            Some(key) => match key {
                KeyCode::H => MainContent::print_help_message(),
                KeyCode::D => self.application.toggle_real_data(),
                KeyCode::S => self.application.toggle_stepwise(),
                KeyCode::T => self.application.toggle_timing_display(),
                KeyCode::P => self.renderer.borrow_mut().cycle_plot_content(),
                KeyCode::Equals | KeyCode::Minus => {
                    self.register_key_released(key);
                }
                KeyCode::R => self.application.randomize_speeds(),
                KeyCode::B => {
                    self.application.toggle_show_ball_stats();
                }
                KeyCode::V => self.application.start_handling_verbosity_input(),
                KeyCode::LControl => {
                    let cursor_pos = self.cursor_pos.clone();
                    self.application.add_candidate(cursor_pos)?;
                    self.renderer.borrow_mut().disable_drawing_cursor();
                }
                KeyCode::F5 => self.application.add_random_candidates(100),
                KeyCode::F1 => self.application.stop_all_balls(),
                KeyCode::F2 => {
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
                // todo() KeyCode::LShift => self.application.new_ball.rand_speed = false,
                _ => {} // No action required for unmatched keys
            },
            None => {} // No keycode provided
        }
        Ok(())
    }

    fn resize_event(
        &mut self,
        _ctx: &mut Context,
        width: f32,
        height: f32,
    ) -> Result<(), GameError> {
        self.application.resize(width, height);

        Ok(())
    }
}
