// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::AppResult;
use crate::application::sim_config::{MAX_TIME_STEP, MIN_TIME_STEP, check_time_step};
use crate::objects2d::ball_impl::BallFactory;

/// Per-step settings of the physics thread: time step, ball factory and
/// neighbour update interval.
pub struct StepConfig {
    dt_goal: f32,
    ball_factory: BallFactory,
    neighbour_update_interval: usize,
}

impl StepConfig {
    /// Starts at `time_step`; balls are seeded from `seed` if given, else
    /// from entropy.
    pub(crate) fn new(time_step: f32, seed: Option<u64>) -> Self {
        StepConfig {
            dt_goal: time_step,
            ball_factory: seed.map_or_else(BallFactory::new, BallFactory::with_seed),
            neighbour_update_interval: 1,
        }
    }

    pub(crate) fn get_ball_factory(&self) -> &BallFactory {
        &self.ball_factory
    }

    pub(crate) fn neighbour_update_interval(&self) -> usize {
        self.neighbour_update_interval
    }

    pub(crate) fn double_time_step(&mut self) {
        self.dt_goal *= 2.0;
        // Clamp to the accepted range to prevent instability
        if self.dt_goal > MAX_TIME_STEP {
            self.dt_goal = MAX_TIME_STEP;
        }
    }

    pub(crate) fn halve_time_step(&mut self) {
        self.dt_goal *= 0.5;
        // Clamp to the accepted range
        if self.dt_goal < MIN_TIME_STEP {
            self.dt_goal = MIN_TIME_STEP;
        }
    }

    pub(crate) fn get_time_step(&self) -> f32 {
        self.dt_goal
    }

    /// Set the time step exactly. Rejects values outside
    /// `MIN_TIME_STEP..=MAX_TIME_STEP` (NaN included) and keeps the current
    /// time step.
    pub(crate) fn set_time_step(&mut self, dt: f32) -> AppResult<()> {
        check_time_step(dt)?;
        self.dt_goal = dt;
        Ok(())
    }
}
