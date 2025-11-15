pub(crate) const FPS_GOAL: u32 = 60;

use crate::frame::application_game_logic::GameLogicData;
use crate::frame::application_sim_stats::SimStats;
use crate::frame::renderer_snapshot::RendererSnapshot;
use crate::frame::window::window_definition::WindowDefinition;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics};
use crate::objects2d::{Rect, Vec2D, VerboseInfoMode};
use arc_swap::ArcSwapOption;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub struct MainContent {
    /// World bounds for physics simulation
    world_rect: Rect,
    pub(crate) physics: BallPhysics,

    pub(crate) sim: SimStats,
    pub(crate) num_balls: usize, // consider removing later
    pub(crate) balls: Vec<Ball2D>,
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
    /// Override for speed limit (None = use default calculation, Some(f32::INFINITY) = disable)
    pub(crate) speed_limit_override: Option<f32>,

    /// Snapshot slot for lock-free communication with renderer
    snapshot_slot: Arc<ArcSwapOption<RendererSnapshot>>,
}

pub struct StatsAccumulator {
    pub(crate) pot: f32,
    pub(crate) kin: f32,
    pub(crate) v_com: Vec2D,
    sum_mass: f32,
    pub(crate) max_v: f32,
    pub(crate) max_id: usize,
}

// type alias to shorten signatures (optional)
pub(crate) type WindowDefinitionRef = Rc<RefCell<WindowDefinition>>;

mod accessors;
mod balls;
mod ctor;
mod sim;
mod snapshot;
mod verbose;

#[cfg(test)]
mod tests;
