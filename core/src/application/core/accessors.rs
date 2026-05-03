use super::*;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::operator::{is_degenerate, remap_point};

impl MainContent {
    #[cfg(test)]
    pub(crate) fn world(&self) -> &SimulationWorld {
        &self.world
    }

    #[cfg(test)]
    pub(crate) fn world_mut(&mut self) -> &mut SimulationWorld {
        &mut self.world
    }

    pub(crate) fn get_game_logic_data(&mut self) -> &mut GameLogicData {
        &mut self.game_data
    }

    pub(crate) fn get_ball_factory(&self) -> &BallFactory {
        self.game_data.get_ball_factory()
    }

    /// Get world boundary rectangle for simulation area.
    pub fn world_region(&self) -> Rect {
        self.world_rect
    }

    /// Get world boundary rectangle with inset margin.
    /// Useful for preventing balls from spawning too close to walls.
    ///
    /// # Arguments
    /// * `inner_margin` - Distance to inset from each boundary
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

    /// Resize simulation world and reconstruct cell grid.
    /// Pauses simulation if running, resumes after delay.
    /// Rebuilds cell grid with new dimensions and re-adds all balls.
    ///
    /// # Arguments
    /// * `world_rect` - New world boundary rectangle
    pub(crate) fn resize(&mut self, world_rect: crate::objects2d::Rect) {
        if !self.sim.is_paused() {
            self.pause_for_resize()
        }

        let world = &mut self.world;

        // Rescale all operator positions so they keep their relative
        // position within the world (e.g. 20%/50% stays 20%/50%).
        let old = world.world_rect;
        world.rescale_all_operators(&old, &world_rect);

        // Keep ball centers and cached position state at the same fractional
        // location in the new world rect, matching operator resize behavior.
        if !is_degenerate(&old) {
            for ball in &mut world.balls {
                let new_pos = remap_point(*ball.get_position(), &old, &world_rect);
                ball.set_position_and_update_tail(new_pos);
                ball.pre_update_pos = remap_point(ball.pre_update_pos, &old, &world_rect);
                ball.last_update_hood_pos =
                    remap_point(ball.last_update_hood_pos, &old, &world_rect);
            }
        }

        // Rebuild cell grid with new dimensions
        let cutoff_and_skin =
            world.physics.get_cutoff_distance() + world.physics.get_skin_thickness();
        let max_radius = BallFactory::get_max_radius();
        world.resize_cell_grid(world_rect, cutoff_and_skin, max_radius);

        // Discard stale thermo history — accumulated grids used the old
        // world rect and would produce incorrect cell-to-world mapping.
        world.thermo_history.clear();
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
        let update_hood_at_frame_start = self.update_hood_at_frame_start;
        self.sim.update_hood_cnt(update_hood_at_frame_start);
    }

    pub(crate) fn get_wait_time(&self) -> u64 {
        self.wait_time
    }

    /// Get current physics FPS value
    pub fn get_physics_fps(&self) -> f32 {
        self.physics_fps
    }

    /// Set current physics FPS value
    pub fn set_physics_fps(&mut self, fps: f32) {
        self.physics_fps = fps;
    }
}
