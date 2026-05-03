use super::{BallPhysics, CellGrid, MainContent, SimStats};
use crate::application::thermo_field::ThermoFieldHistory;
use crate::application::{AppResult, GameLogic};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::BallFactory;
use arc_swap::ArcSwapOption;
use std::sync::Arc;

impl MainContent {
    pub fn new(world_rect: Rect) -> AppResult<Self> {
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
                cached_force_results: Vec::new(),
                thermo_history: ThermoFieldHistory::new(),
                line_segments: Default::default(),
                valve_segments: Default::default(),
                source_spheres: Default::default(),
                thermostats: Default::default(),
            },
            game_data: <Self as GameLogic>::init_game_logic(),
            update_hood_at_frame_start: 0,
            snapshot_slot: Arc::new(ArcSwapOption::from(None)),
            wait_time: 0,
            physics_fps: 0.0,
        })
    }
}
