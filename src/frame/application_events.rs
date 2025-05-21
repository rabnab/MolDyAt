use crate::frame::application_master::MainContent;
use crate::frame::{DrawingStuff, GameLogic, LogStuff};
use crate::objects2d::ball_impl::get_update_hood_call_cnt;
use ggez::event::{EventHandler, MouseButton};
use ggez::graphics::{Canvas, Color};
use ggez::input::keyboard::{KeyCode, KeyInput};
use ggez::{Context, GameError, GameResult};

impl<'a> EventHandler for MainContent<'a> {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        // let mut cnt = 0;
        // let mut sum: Duration = Duration::new(0, 0);
        let tmp_cnt = get_update_hood_call_cnt();
        while _ctx
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
        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(0, 0, 0));
        self.draw_balls(ctx, &mut canvas, self.get_ball_vec())?;
        self.draw_cursor(ctx, &mut canvas)?;
        self.draw_frame(ctx, &mut canvas, self.get_number_of_balls())?;
        self.draw_plot(ctx, &mut canvas)?;
        canvas.finish(ctx)?;
        self.sim.reset_frame_since_draw();
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

    // fn mouse_button_up_event(
    //     &mut self,
    //     _ctx: &mut Context,
    //     button: MouseButton,
    //     x: f32,
    //     y: f32,
    // ) -> Result<(), GameError> {
    //     match button {
    //         MouseButton::Left => {
    //             self.add_candidate_px(x, y).ok(); //ignore any errors because of not being able to add
    //             self.sim.draw_cursor = false;
    //         }
    //         _ => (),
    //     }
    //     Ok(())
    // }

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
                KeyCode::F5 => self.add_random_candidates(100),
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
        self.resize(width, height)?;
        Ok(())
    }
}
