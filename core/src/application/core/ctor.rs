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
            balls: Vec::new(),
            physics: loc_physics,
            cell_grid: my_cell_grid,

            world_rect,
            game_data: <Self as GameLogic>::init_game_logic(),
            sim: SimStats::default(),
            num_balls: 0,
            real_data: false,
            step_by_step: false,
            step_count: 0,
            show_timings: false,
            show_ball_stats: true,
            resize_resume_at: None,
            was_paused_for_resize: false,
            update_hood_at_frame_start: 0,
            speed_limit_override: None,
            snapshot_slot: Arc::new(ArcSwapOption::from(None)),
            wait_time: 0,
            cached_force_results: Vec::new(),
            physics_fps: 0.0,
            thermo_history: ThermoFieldHistory::new(),
            line_segments: Vec::new(),
            line_segment_next_id: 0,
            valve_segments: Vec::new(),
            valve_segment_next_id: 0,
        })
    }
}
