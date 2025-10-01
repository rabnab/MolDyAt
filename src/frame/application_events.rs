use crate::frame::application_master::MainContent;
use crate::frame::new_ball_def::NewBallDef;
use crate::frame::{DrawingStuff, GameLogic, LogStuff};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::get_update_hood_call_cnt;
use crate::timing_monad::TimingMonad;
use ggez::event::{EventHandler, MouseButton};
use ggez::graphics::{Canvas, Color};
use ggez::input::keyboard::{KeyCode, KeyInput};
use ggez::{Context, GameError, GameResult};

impl<'a> EventHandler for MainContent<'a> {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        self.unpause_after_resize(Some(ctx));

        // let mut cnt = 0;
        // let mut sum: Duration = Duration::new(0, 0);
        let tmp_cnt = get_update_hood_call_cnt();
        while ctx
            .time
            .check_update_time(crate::frame::application_master::FPS_GOAL * self.sim.get_speedup())
        {
            // let frame_start = Instant::now();
            // let dur = frame_start.checked_duration_since(self.sim.time_last_frame);
            // if let Some(dur) = dur {
            //     cnt+=1;
            //     sum+=dur;
            // }
            //
            // self.sim.time_last_frame = frame_start;
            self.sim.inc_frame_since_draw();

            self.simulation_step()?;
        }
        self.sim
            .set_update_hood_cnt(get_update_hood_call_cnt() - tmp_cnt);
        if self.sim.is_key_pressed(KeyCode::Minus) {
            self.sim.decrease_speedup();
        } else if self.sim.is_key_pressed(KeyCode::Equals) {
            self.sim.increase_speedup();
        }
        // if cnt > 0 {
        //     println!("simulate {} avg {:?} simtime {}", cnt, sum/cnt, self.sim.time);
        // }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        // Get the window's width and height
        // println!("{} draw", self.sim.time);
        let mut timer = TimingMonad::new();

        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(0, 0, 0));
        timer.time_result("draw_balls", || {
            self.draw_balls(ctx, &mut canvas, self.get_ball_vec())
        })?;
        timer.time_result("draw_cursor", || self.draw_cursor(&mut canvas))?;
        timer.time_result("draw_frame", || {
            self.draw_frame(ctx, &mut canvas, self.get_number_of_balls())
        })?;
        timer.time_result("draw_plot", || self.draw_plot(ctx, &mut canvas))?;
        timer.time_result("finish_canvas", || canvas.finish(ctx))?;
        timer.time("reset_frame", || self.sim.reset_frame_since_draw());
        self.log_timings(timer);
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
            self.new_ball.next_ball_size();
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
        self.mut_window_definition().update_cursor_and_text(x, y);
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
                KeyCode::Space => self.sim.toggle_pause(),
                KeyCode::LControl => self.enable_drawing_cursor(repeated),
                KeyCode::Escape => self.clear_points(),
                KeyCode::LShift => self.enable_random_ball_speed(),
                KeyCode::Minus | KeyCode::Equals => {
                    self.sim.register_key_pressed(key);
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
                KeyCode::D => self.real_data = !self.real_data,
                KeyCode::S => self.step_by_step = !self.step_by_step,
                KeyCode::T => self.show_timings = !self.show_timings,
                KeyCode::P => self.cycle_plot_content(),
                KeyCode::Equals | KeyCode::Minus => {
                    self.sim.register_key_released(key);
                }
                KeyCode::R => self.randomize_speeds(),
                KeyCode::B => {
                    self.show_ball_stats = !self.show_ball_stats;
                    if !self.show_ball_stats {
                        self.mut_window_definition().ball_status_text_pos = None;
                    }
                }
                KeyCode::V => self.start_handling_verbosity_input(),
                KeyCode::LControl => {
                    self.add_cursor_candidate();
                    self.sim.set_draw_cursor(false);
                }
                KeyCode::F5 => self.add_random_candidates(10),
                KeyCode::F1 => self.get_ball_vec().iter().for_each(|b_rc| {
                    let mut b = b_rc.borrow_mut();
                    b.v = Vec2D::zero();
                }),
                KeyCode::F2 => {
                    let small_size = self.new_ball.get_ball_size_of_idx(2);
                    let large_size = self.new_ball.get_ball_size_of_idx(4);

                    let small_mass = NewBallDef::get_ball_mass_of_size(small_size);
                    let large_mass = NewBallDef::get_ball_mass_of_size(large_size);
                    let small = self
                        .get_game_logic_data()
                        .get_ball_factory()
                        .create_massive_with_velocity(
                            Vec2D::zero(),
                            small_size,
                            small_mass,
                            Vec2D::new(0.1, 0.),
                        );

                    let large = self
                        .get_game_logic_data()
                        .get_ball_factory()
                        .create_massive_with_velocity(
                            Vec2D::zero(),
                            large_size,
                            large_mass,
                            Vec2D::new(0., 0.1),
                        );

                    self.add_candidate_to_vec(small);
                    self.add_candidate_to_vec(large);
                }
                KeyCode::LShift => self.new_ball.rand_speed = false,
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
        if !self.sim.is_paused() {
            self.pause_for_resize()
        }
        // Resume after 300ms
        self.resize(width, height)?;
        Ok(())
    }
}
