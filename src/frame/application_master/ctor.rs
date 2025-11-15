use super::{BallPhysics, CellGrid, MainContent, RendererRef, SimStats};
use crate::frame::{AppResult, GameLogic};
use crate::objects2d::Rect;

impl MainContent {
    pub fn new(world_rect: Rect, renderer: RendererRef) -> AppResult<Self> {
        let loc_physics = BallPhysics::default();
        let cutoff = loc_physics.get_cutoff_distance();

        let my_cell_grid = CellGrid::new(world_rect, cutoff)?;

        Ok(Self {
            balls: Vec::new(),
            physics: loc_physics,
            cell_grid: my_cell_grid,

            renderer,
            game_data: <Self as GameLogic>::init_game_logic(),
            sim: SimStats::default(),
            num_balls: 0,
            real_data: false,
            step_by_step: false,
            step_count: 0,
            show_timings: false,
            show_ball_stats: false,
            resize_resume_at: None,
            was_paused_for_resize: false,
            update_hood_at_frame_start: 0,
            speed_limit_override: None,
        })
    }
}
