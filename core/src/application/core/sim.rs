// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{MainContent, SimulationEngine};
use crate::application::VerletIntegration;
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

impl VerletIntegration for MainContent {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect) {
        SimulationEngine::update_sim(self.world_mut(), dt, bound_rect);
    }
}
