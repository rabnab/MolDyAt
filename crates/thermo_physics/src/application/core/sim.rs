// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{MainContent, SimulationEngine};
use crate::application::AppResult;
#[cfg(test)]
use crate::objects2d::Rect;

impl MainContent {
    pub fn increase_wait_time(&mut self) {
        self.runtime.increase_wait_time();
    }

    pub fn decrease_wait_time(&mut self) {
        self.runtime.decrease_wait_time();
    }

    /// Pause simulation when window resize is detected.
    pub(crate) fn pause_for_resize(&mut self) {
        self.runtime.paused_for_resize = true;
        self.world.sim.toggle_pause();
    }

    /// Unpause after resize, triggered when the mouse button is released.
    /// The cell grid already matches the new world boundaries (`resize`
    /// rebuilds it and nothing moves a ball while paused), so this only
    /// resumes the simulation.
    pub fn unpause_after_resize(&mut self) {
        if !self.runtime.paused_for_resize {
            return;
        }

        if self.world.sim.is_paused() {
            self.world.sim.toggle_pause();
        }
        self.runtime.paused_for_resize = false;
    }
}

impl MainContent {
    #[cfg(test)]
    pub(crate) fn update_sim(&mut self, dt: f32, bound_rect: &Rect) {
        let log_timings = self.options.show_timings;
        SimulationEngine::update_sim(self.world_mut(), dt, bound_rect, log_timings);
    }

    pub(crate) fn collect_neighbours_by_cell(&mut self, dt: f32) {
        let (world, step_config, options) = self.step_parts();
        SimulationEngine::collect_neighbours_by_cell(world, step_config, dt, options.show_timings);
    }

    /// One step with all derived data (headless runs and tests): the thermo
    /// fields are sampled as in the physics thread (see
    /// `SimulationEngine::record_thermo_fields_if_due`).
    pub fn simulation_step(&mut self) -> AppResult<()> {
        self.step_recording_thermo_fields(true)
    }

    /// One step, which also samples the thermo fields when
    /// `record_thermo_fields` (the physics thread does so only while the
    /// heatmap is shown) and the world is not paused.
    pub fn step_recording_thermo_fields(&mut self, record_thermo_fields: bool) -> AppResult<()> {
        let dt = self.time_step();
        self.step_with_dt(dt, record_thermo_fields)
    }

    #[cfg(test)]
    pub(crate) fn simulation_step_with_dt(&mut self, dt: f32) -> AppResult<()> {
        self.step_with_dt(dt, true)
    }

    fn step_with_dt(&mut self, dt: f32, record_thermo_fields: bool) -> AppResult<()> {
        let (world, step_config, options) = self.step_parts();
        let stepped = !world.sim.is_paused();
        SimulationEngine::simulation_step_with_dt(world, step_config, options, dt)?;
        if stepped && record_thermo_fields {
            SimulationEngine::record_thermo_fields_if_due(world, options.show_timings);
        }
        Ok(())
    }
}
