use crate::application::core::SimulationEngine;
use crate::application::{AppResult, GameLogic, MainContent};
use crate::objects2d::ball_impl::{Ball2D, BallFactory};
use std::time::Instant;

const DT_GOAL: f32 = 0.01; // 1. / (FPS_GOAL as f32);

pub struct GameLogicData {
    dt_goal: f32,
    ball_factory: BallFactory,
    neighbour_update_interval: usize,
}

struct Timer(Instant);
impl Timer {
    fn start() -> Self {
        Self(Instant::now())
    }

    fn ms(&self) -> u128 {
        self.0.elapsed().as_millis()
    }
}

impl Default for GameLogicData {
    fn default() -> Self {
        GameLogicData {
            dt_goal: DT_GOAL,
            ball_factory: BallFactory::new(),
            neighbour_update_interval: 1,
        }
    }
}

impl GameLogicData {
    pub(crate) fn get_ball_factory(&self) -> &BallFactory {
        &self.ball_factory
    }

    pub(crate) fn neighbour_update_interval(&self) -> usize {
        self.neighbour_update_interval
    }

    pub(crate) fn double_time_step(&mut self) {
        self.dt_goal *= 2.0;
        // Clamp to reasonable range to prevent instability
        if self.dt_goal > 0.16 {
            self.dt_goal = 0.16;
        }
    }

    pub(crate) fn halve_time_step(&mut self) {
        self.dt_goal *= 0.5;
        // Clamp to reasonable range
        if self.dt_goal < 0.000625 {
            self.dt_goal = 0.000625;
        }
    }

    pub(crate) fn get_time_step(&self) -> f32 {
        self.dt_goal
    }
}

impl GameLogic for MainContent {
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        SimulationEngine::is_ball_in_region(self.world(), pt)
    }

    fn is_ball_colliding(&self, pt: &Ball2D, margin: f32) -> bool {
        SimulationEngine::is_ball_colliding(self.world(), pt, margin)
    }

    fn collect_neighbours_by_cell(&mut self, dt: f32) {
        SimulationEngine::collect_neighbours_by_cell(self, dt);
    }

    fn simulation_step(&mut self) -> AppResult<()> {
        SimulationEngine::simulation_step(self)
    }

    // Helper to encapsulate the simulation update + cell grid updates + cleanup.
    // This reduces duplication and keeps measurement-only code simple.

    fn init_game_logic() -> GameLogicData {
        GameLogicData {
            neighbour_update_interval: 1,
            ..Default::default()
        }
    }
}
