use super::{BallPhysics, CellGrid, MainContent, SimStats};
use crate::application::game_logic::GameLogicData;
use crate::application::thermo_field::ThermoFieldHistory;
use crate::application::{AppResult, GameLogic};
use crate::graphics::renderer::{Margins, ViewportTransform};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::BallFactory;
use arc_swap::ArcSwapOption;
use std::sync::Arc;

impl MainContent {
    /// Compute the default startup world rectangle for a window and viewport margins.
    ///
    /// This centralizes the initial world bootstrap policy so renderer entrypoints
    /// do not need to duplicate the world-size math.
    pub fn bootstrap_world_rect(window_width: u32, window_height: u32, margins: Margins) -> Rect {
        ViewportTransform::compute_world_bounds_from_window_size(
            window_width,
            window_height,
            margins,
        )
    }

    pub fn new(world_rect: Rect) -> AppResult<Self> {
        Self::new_internal(world_rect, None)
    }

    pub fn new_with_seed(world_rect: Rect, seed: u64) -> AppResult<Self> {
        Self::new_internal(world_rect, Some(seed))
    }

    fn new_internal(world_rect: Rect, seed: Option<u64>) -> AppResult<Self> {
        let loc_physics = BallPhysics::default();
        let cutoff_and_skin = loc_physics.get_cutoff_and_skin();
        let max_radius = BallFactory::get_max_radius();

        let my_cell_grid = CellGrid::new(world_rect, cutoff_and_skin, max_radius)?;

        Ok(Self {
            world: super::SimulationWorld {
                balls: Vec::new(),
                physics: loc_physics,
                cell_grid: my_cell_grid,
                world_rect,
                sim: SimStats::default(),
                real_data: false,
                step_by_step: false,
                step_count: 0,
                show_timings: false,
                show_ball_stats: true,
                was_paused_for_resize: false,
                speed_limit_override: None,
                kernel_points: Vec::new(),
                cached_force_results: Vec::new(),
                thermo_history: ThermoFieldHistory::new(),
                line_segments: Default::default(),
                valve_segments: Default::default(),
                source_spheres: Default::default(),
                thermostats: Default::default(),
                ghost_wall: None,
                wall_chains: Default::default(),
                #[cfg(test)]
                test_faults: Default::default(),
            },
            game_data: seed.map_or_else(
                <Self as GameLogic>::init_game_logic,
                GameLogicData::with_seed,
            ),
            runtime: super::PhysicsRuntimeState {
                snapshot_slot: Arc::new(ArcSwapOption::from(None)),
                wait_time: 0,
                physics_fps: 0.0,
            },
        })
    }
}
