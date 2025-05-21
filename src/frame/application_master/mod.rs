pub const MARGIN_PX_X: f32 = 50.;
pub const MARGIN_PX_Y: f32 = 50.;
pub(crate) const FPS_GOAL: u32 = 60;

use crate::frame::application_drawing::PlotContent;
use crate::frame::application_game_logic::GameLogicData;
use crate::frame::application_sim_stats::SimStats;
use crate::frame::new_ball_def::NewBallDef;
use crate::frame::window_definition::WindowDefinition;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics};
use crate::objects2d::{Vec2D, VerboseInfoMode};
use ggez::graphics::{Mesh, Shader};
use std::cell::RefCell;
use std::rc::Rc;

pub struct MainContent<'a> {
    pub(crate) physics: BallPhysics,
    win_definition: WindowDefinition,
    pub(crate) new_ball: NewBallDef,
    pub(crate) sim: SimStats,
    pub(crate) num_balls: usize, // consider removing later
    v_points: Vec<BallCell<'a>>,
    pub(crate) real_data: bool,
    pub(crate) step_by_step: bool,
    pub(crate) show_timings: bool,
    pub(crate) show_ball_stats: bool,
    show_plot: PlotContent,
    pub(crate) step_count: usize,
    pub(crate) crosshair_mesh: Option<Mesh>,
    shader: Option<Shader>,
    game_data: GameLogicData,
}

// type alias to shorten signatures (optional)
pub(crate) type BallCell<'a> = Rc<RefCell<Ball2D<'a>>>;

mod accessors;
mod balls;
mod ctor;
mod sim;
mod verbose;

#[cfg(test)]
mod tests;
