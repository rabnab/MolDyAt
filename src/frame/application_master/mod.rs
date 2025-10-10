pub(crate) const FPS_GOAL: u32 = 60;

use crate::frame::application_game_logic::GameLogicData;
use crate::frame::application_sim_stats::SimStats;
use crate::frame::renderer::Renderer;
use crate::frame::window::window_definition::WindowDefinition;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics};
use crate::objects2d::{Vec2D, VerboseInfoMode};
use std::cell::RefCell;
use std::rc::Rc;

pub struct MainContent {
    renderer: RendererRef,
    pub(crate) physics: BallPhysics,

    pub(crate) sim: SimStats,
    pub(crate) num_balls: usize, // consider removing later
    v_points: Vec<BallCell>,
    pub(crate) cell_grid: CellGrid,
    pub(crate) real_data: bool,
    pub(crate) step_by_step: bool,
    pub(crate) show_timings: bool,
    pub(crate) show_ball_stats: bool,
    pub(crate) step_count: usize,

    game_data: GameLogicData,
    resize_resume_at: Option<std::time::Instant>,
    was_paused_for_resize: bool,
    update_hood_at_frame_start: usize,
}
// type alias to shorten signatures (optional)
pub(crate) type BallCell = Rc<RefCell<Ball2D>>;
pub(crate) type WindowDefinitionRef = Rc<RefCell<WindowDefinition>>;
pub(crate) type RendererRef = Rc<RefCell<dyn Renderer>>;
mod accessors;
mod balls;
mod ctor;
mod sim;
mod verbose;

#[cfg(test)]
mod tests;
