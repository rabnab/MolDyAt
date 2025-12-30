use super::*;
use crate::objects2d::Rect;
use crate::objects2d::ball_cell::cell_grid;
use crate::objects2d::ball_impl::BallFactory;

impl MainContent {
    pub(crate) fn get_game_logic_data(&mut self) -> &mut GameLogicData {
        &mut self.game_data
    }

    pub(crate) fn get_ball_factory(&self) -> &BallFactory {
        self.game_data.get_ball_factory()
    }

    pub fn world_region(&self) -> Rect {
        self.world_rect
    }

    pub fn world_region_inset(&self, inner_margin: f32) -> Rect {
        let world_region = self.world_region();
        let tl = world_region.get_tl();
        Rect::new(
            tl.0 + inner_margin,
            tl.1 - inner_margin,
            world_region.w - 2.0 * inner_margin,
            world_region.h + 2.0 * inner_margin,
        )
    }

    pub(crate) fn toggle_pause(&mut self) {
        self.sim.toggle_pause();
    }

    pub(crate) fn toggle_real_data(&mut self) {
        self.real_data = !self.real_data;
    }

    pub(crate) fn toggle_show_ball_stats(&mut self) {
        self.show_ball_stats = !self.show_ball_stats;
    }

    pub(crate) fn toggle_stepwise(&mut self) {
        self.step_by_step = !self.step_by_step;
    }

    pub(crate) fn toggle_timing_display(&mut self) {
        self.show_timings = !self.show_timings;
    }

    /// tells the renderer to resize the window to the given px size
    /// pauses the sim if it's running, resumes after some time
    pub(crate) fn resize(&mut self, world_rect: crate::objects2d::Rect) {
        if !self.sim.is_paused() {
            self.pause_for_resize()
        }
        // Update world_rect with correctly computed world coordinates
        self.world_rect = world_rect;

        // Rebuild cell grid with new dimensions
        let cutoff = self.physics.get_cutoff_distance();
        let max_radius = BallFactory::get_max_radius();
        self.cell_grid = cell_grid::CellGrid::new(self.world_rect, cutoff, max_radius)
            .expect("Failed to create cell grid on resize");

        // Re-add all balls to the new cell grid
        for i in 0..self.balls.len() {
            self.cell_grid.add_ball(i, &mut self.balls);
        }
    }

    pub(crate) fn double_time_step(&mut self) {
        self.game_data.double_time_step();
    }

    pub(crate) fn halve_time_step(&mut self) {
        self.game_data.halve_time_step();
    }

    pub(crate) fn get_time_step(&self) -> f32 {
        self.game_data.get_time_step()
    }
    pub(crate) fn start_frame(&mut self) {
        self.sim.reset_frame_since_draw();
        self.update_hood_at_frame_start = self.sim.get_update_hood_cnt();
    }

    pub(crate) fn end_frame(&mut self) {
        self.sim.update_hood_cnt(self.update_hood_at_frame_start);
    }

    pub(crate) fn get_wait_time(&self) -> u64 {
        self.wait_time
    }
}
