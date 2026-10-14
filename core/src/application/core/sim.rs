// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{MainContent, SimulationEngine};
use crate::application::AppResult;
#[cfg(test)]
use crate::objects2d::Rect;

impl MainContent {
    pub(crate) fn increase_wait_time(&mut self) {
        self.runtime.increase_wait_time();
    }

    pub(crate) fn decrease_wait_time(&mut self) {
        self.runtime.decrease_wait_time();
    }

    /// Pause simulation when window resize is detected.
    pub(crate) fn pause_for_resize(&mut self) {
        self.world.was_paused_for_resize = true;
        self.world.sim.toggle_pause();
    }

    /// Unpause after resize, triggered when the mouse button is released.
    /// The cell grid already matches the new world boundaries (`resize`
    /// rebuilds it and nothing moves a ball while paused), so this only
    /// resumes the simulation.
    pub(crate) fn unpause_after_resize(&mut self) {
        if !self.world.was_paused_for_resize {
            return;
        }

        if self.world.sim.is_paused() {
            self.world.sim.toggle_pause();
        }
        self.world.was_paused_for_resize = false;
    }
}

impl MainContent {
    #[cfg(test)]
    pub(crate) fn update_sim(&mut self, dt: f32, bound_rect: &Rect) {
        SimulationEngine::update_sim(self.world_mut(), dt, bound_rect);
    }

    pub(crate) fn collect_neighbours_by_cell(&mut self, dt: f32) {
        let (world, step_config) = self.world_and_step_config();
        SimulationEngine::collect_neighbours_by_cell(world, step_config, dt);
    }

    /// One step with all derived data (headless runs and tests): the thermo
    /// fields are sampled as in the physics thread (see
    /// `SimulationEngine::record_thermo_fields_if_due`).
    pub fn simulation_step(&mut self) -> AppResult<()> {
        let dt = self.time_step();
        self.simulation_step_with_dt(dt)
    }

    pub(crate) fn simulation_step_with_dt(&mut self, dt: f32) -> AppResult<()> {
        let (world, step_config) = self.world_and_step_config();
        let stepped = !world.sim.is_paused();
        SimulationEngine::simulation_step_with_dt(world, step_config, dt)?;
        if stepped {
            SimulationEngine::record_thermo_fields_if_due(world);
        }
        Ok(())
    }
}
