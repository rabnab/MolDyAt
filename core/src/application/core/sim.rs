use super::{MainContent, SimulationEngine};
use crate::application::VerletIntegration;
use crate::objects2d::Rect;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::BallFactory;

impl MainContent {
    pub(crate) fn increase_wait_time(&mut self) {
        self.runtime.increase_wait_time();
    }

    pub(crate) fn decrease_wait_time(&mut self) {
        self.runtime.decrease_wait_time();
    }

    /// Pause simulation when window resize is detected.
    pub(crate) fn pause_for_resize(&mut self) {
        self.was_paused_for_resize = true;
        self.sim.toggle_pause();
    }

    /// Unpause after resize, triggered when the mouse button is released.
    /// Unpauses simulation and reconstructs cell grid to match new world boundaries.
    pub(crate) fn unpause_after_resize(&mut self) {
        if !self.was_paused_for_resize {
            return;
        }

        if self.sim.is_paused() {
            self.sim.toggle_pause();
        }

        let max_radius = BallFactory::get_max_radius();
        let nu_rect = self.world_region();
        let cutoff_and_skin = self.physics.get_cutoff_and_skin();
        let nu_grid = CellGrid::resize(
            &mut self.world.cell_grid,
            nu_rect,
            cutoff_and_skin,
            max_radius,
            &mut self.world.balls,
        )
        .expect("Failed to resize CellGrid");
        self.world.cell_grid = nu_grid;
        self.world.was_paused_for_resize = false;
    }
}

impl VerletIntegration for MainContent {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect) {
        SimulationEngine::update_sim(self.world_mut(), dt, bound_rect);
    }
}
