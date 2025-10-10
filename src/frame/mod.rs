mod application_drawing;
mod application_game_logic;
pub(crate) mod application_log;
mod application_master;
mod application_sim_stats;
pub(crate) mod ggez;
mod new_ball_def;
mod renderer;
pub(crate) mod window;

use crate::frame::application_game_logic::GameLogicData;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::Ball2D;
use crate::timing_monad::TimingMonad;
pub use application_master::MainContent;
// pub use application_master::{MARGIN_PX_X, MARGIN_PX_Y};

// In a common module, e.g., src/error.rs
#[derive(Debug)]
pub enum AppError {
    CustomError(String),
    // Add more variants as needed
}

pub type AppResult<T = ()> = Result<T, AppError>;

// impl From<AppError> for GameError {
//     fn from(err: AppError) -> Self {
//         match err {
//             AppError::CustomError(msg) => GameError::CustomError(msg),
//             _ => GameError::CustomError("Unknown AppError".to_string()),
//         }
//     }
// }

pub trait LogStuff {
    fn any_log(&self) -> bool;
    fn print_help_message();

    fn status_text(&self, fps: u32) -> Option<String>;
    fn format_g(value: f32, precision: usize) -> String;
    fn log_closeness_update(ball: &Ball2D);
    fn log_timings(&self, timer: TimingMonad);
    fn log_separator_line();
}

// pub trait DrawingStuff<'a> {
//     fn draw_cursor(&self, canvas: &mut Canvas) -> GameResult;
//     fn draw_plot(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
//     fn draw_text_on_ball(&self, canvas: &mut Canvas, ball: &Ball2D) -> GameResult;
//     fn draw_simple_balls(
//         &self,
//         canvas: &mut Canvas,
//         balls: &Vec<Rc<RefCell<Ball2D>>>,
//     ) -> GameResult;
//     fn draw_ball_by_mesh(&self, canvas: &mut Canvas, ball_cell: &Rc<RefCell<Ball2D>>)
//     -> GameResult;
//     fn draw_neighbour_lines(
//         &self,
//         ctx: &mut Context,
//         canvas: &mut Canvas,
//         ball: &Ref<Ball2D>,
//         ball_coord_px: &Vec2D,
//     ) -> Result<(), GameError>;
//     fn draw_single_ball(
//         &self,
//         circle: &Mesh,
//         // scale_px: f32,
//         canvas: &mut Canvas,
//         ball: &Ref<Ball2D>,
//         ball_coord_px: &Vec2D,
//     ) -> Result<(), GameError>;
//     fn draw_opengl_balls(
//         &self,
//         ctx: &mut Context,
//         canvas: &mut Canvas,
//         shader_used: &Shader,
//         balls: &Vec<Rc<RefCell<Ball2D>>>,
//     ) -> GameResult;
//     fn draw_balls(
//         &self,
//         ctx: &mut Context,
//         canvas: &mut Canvas,
//         balls: &Vec<Rc<RefCell<Ball2D<'a>>>>,
//     ) -> GameResult;
//     fn get_verbose_balls(balls: &Vec<Rc<RefCell<Ball2D<'a>>>>) -> Vec<Rc<RefCell<Ball2D<'a>>>>;
//     fn draw_verbose_balls(
//         &self,
//         ctx: &mut Context,
//         canvas: &mut Canvas,
//         verbose_balls: &[Rc<RefCell<Ball2D>>],
//     ) -> GameResult;
//     fn draw_cross_hair(&self, canvas: &mut Canvas) -> GameResult<()>;
//     fn draw_frame(
//         &mut self,
//         ctx: &mut Context,
//         canvas: &mut Canvas,
//         num_balls: usize,
//     ) -> GameResult;
//     fn resize(&mut self, width: f32, height: f32) -> Result<(), GameError>;
//
//     fn calculate_plot_boundary<F>(&self, initial: f32, operation: F) -> f32
//     where
//         F: Fn(f32, f32) -> f32;
//
//     fn draw_all_ball_tails(
//         &self,
//         ctx: &mut Context,
//         canvas: &mut Canvas,
//         balls: &Vec<Rc<RefCell<Ball2D>>>,
//     ) -> Result<(), GameError>;
//     /// Draws the cell grid as faint colored rectangles in the background.
//     fn draw_cell_grid_background(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
// }

pub trait GameLogic {
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool;
    fn is_ball_colliding(&self, pt: &Ball2D) -> bool;
    // fn add_candidate_px(&mut self, x_px: f32, y_px: f32) -> AppResult<()>;
    fn collect_neighbours_by_cell(&mut self, dt: f32);
    fn simulation_step(&mut self) -> AppResult<()>;
    fn init_game_logic() -> GameLogicData;

    // /// Add a new ball candidate at the given position with zero initial velocity using the current size of ball_factory.
    // fn add_candidate(&mut self, ball_pos: Vec2D) -> AppResult<()>;
    // fn add_candidate_ball(&mut self, candidate: Ball2D) -> AppResult;
}
// pub trait SimulationToggler {
//     fn make_all_verbose(&mut self, mode: VerboseInfoMode);
//     fn toggle_ball_verbose(&mut self, number: usize, mode: VerboseInfoMode);
//     fn clear_verbose_balls(&mut self);
// }

pub trait VerletIntegration {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect);
}
