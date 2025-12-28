mod core;
mod game_logic;
pub(crate) mod log_util;
pub(crate) mod physics_thread;
pub(crate) mod renderer_snapshot;
mod sim_stats;

use crate::application::game_logic::GameLogicData;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::Ball2D;
use crate::timing_monad::TimingMonad;
pub use core::MainContent;

// In a common module, e.g., src/error.rs
#[derive(Debug)]
pub enum AppError {
    CustomError(String),
    // Add more variants as needed
}

pub type AppResult<T = ()> = Result<T, AppError>;

pub trait LogStuff {
    fn any_log(&self) -> bool;
    fn print_help_message();

    fn status_text(&self, fps: u32, dt: f32) -> Option<String>;
    fn format_g(value: f32, precision: usize) -> String;
    fn log_closeness_update(ball: &Ball2D);
    fn log_timings(&self, timer: TimingMonad);
    fn log_separator_line();
}

pub trait GameLogic {
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool;
    fn is_ball_colliding(&self, pt: &Ball2D, margin: f32) -> bool;
    // fn add_candidate_px(&mut self, x_px: f32, y_px: f32) -> AppResult<()>;
    fn collect_neighbours_by_cell(&mut self, dt: f32);
    fn simulation_step(&mut self) -> AppResult<()>;
    fn init_game_logic() -> GameLogicData;
}

pub trait VerletIntegration {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect);
}
