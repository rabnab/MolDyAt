mod application_drawing;
mod application_events;
mod application_game_logic;
pub(crate) mod application_log;
mod application_master;
mod application_sim_stats;
mod new_ball_def;
mod window_definition;

use crate::frame::application_game_logic::GameLogicData;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use crate::timing_monad::TimingMonad;
pub use application_master::MainContent;
pub use application_master::{MARGIN_PX_X, MARGIN_PX_Y};
use ggez::graphics::{Canvas, Mesh, Rect, Shader};
use ggez::{Context, GameError, GameResult};
use std::cell::{Ref, RefCell};
use std::rc::Rc;

pub trait LogStuff {
    fn any_log(&self) -> bool;
    fn print_help_message();

    fn status_text(&self, fps: u32, num_balls: usize) -> String;
    fn format_g(value: f32, precision: usize) -> String;
    fn log_closeness_update(ball: &Ball2D);
    fn log_timings(&self, timer: TimingMonad);
    fn log_separator_line();
}

pub trait DrawingStuff<'a> {
    fn draw_cursor(&self, canvas: &mut Canvas) -> GameResult;
    fn draw_plot(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
    fn draw_text_on_ball(&self, canvas: &mut Canvas, ball: &Ball2D) -> GameResult;
    fn draw_simple_balls(
        &self,
        canvas: &mut Canvas,
        balls: &Vec<Rc<RefCell<Ball2D>>>,
    ) -> GameResult;
    fn draw_ball_by_mesh(&self, canvas: &mut Canvas, ball_cell: &Rc<RefCell<Ball2D>>)
    -> GameResult;
    fn draw_neighbour_lines(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError>;
    fn draw_single_ball(
        &self,
        circle: &Mesh,
        // scale_px: f32,
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError>;
    fn draw_opengl_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        shader_used: &Shader,
        balls: &Vec<Rc<RefCell<Ball2D>>>,
    ) -> GameResult;
    fn draw_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        balls: &Vec<Rc<RefCell<Ball2D<'a>>>>,
    ) -> GameResult;
    fn get_verbose_balls(balls: &Vec<Rc<RefCell<Ball2D<'a>>>>) -> Vec<Rc<RefCell<Ball2D<'a>>>>;
    fn draw_verbose_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        verbose_balls: &[Rc<RefCell<Ball2D>>],
    ) -> GameResult;
    fn draw_cross_hair(&self, canvas: &mut Canvas) -> GameResult<()>;
    fn draw_frame(
        &mut self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        num_balls: usize,
    ) -> GameResult;
    fn resize(&mut self, width: f32, height: f32) -> Result<(), GameError>;

    fn calculate_plot_boundary<F>(&self, initial: f32, operation: F) -> f32
    where
        F: Fn(f32, f32) -> f32;

    fn draw_all_ball_tails(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        balls: &Vec<Rc<RefCell<Ball2D>>>,
    ) -> Result<(), GameError>;
}

pub trait GameLogic {
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool;
    fn is_ball_colliding(&self, pt: &Ball2D) -> bool;
    fn add_candidate_px(&mut self, x_px: f32, y_px: f32) -> GameResult;
    fn collect_neighbours_by_cell(&mut self, dt: f32);
    fn simulation_step(&mut self) -> GameResult;
    fn init_game_logic() -> GameLogicData;
}
// pub trait SimulationToggler {
//     fn make_all_verbose(&mut self, mode: VerboseInfoMode);
//     fn toggle_ball_verbose(&mut self, number: usize, mode: VerboseInfoMode);
//     fn clear_verbose_balls(&mut self);
// }

pub trait VerletIntegration {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect);
}
