// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::objects2d::ball_impl::{Ball2D, Named, get_update_hood_call_cnt};

impl MainContent {
    /// Generate status text with simulation statistics.
    /// Returns None if ball stats display is disabled.
    /// Uses real units (SI) if real_data flag is set, otherwise simulation units.
    pub(crate) fn status_text(&self, fps: u32, dt: f32) -> Option<String> {
        if !self.world().show_ball_stats {
            return None;
        }
        let world = self.world();
        let num_balls = world.balls.len();
        let return_value = if world.real_data {
            world.sim.get_status_text_real(fps, num_balls, dt)
        } else {
            world
                .sim
                .get_status_text(fps, num_balls, get_update_hood_call_cnt(), dt)
        };
        Some(return_value)
    }
}

pub(crate) fn log_closeness_update(ball: &Ball2D) {
    if ball.verbose.console {
        log::debug!(
            target: crate::logging::VERBOSE_TARGET,
            "update_closeness: {} nearest/farthest: {}/{}",
            ball.get_name(),
            ball.hood.nearest,
            ball.hood.farthest
        );
    }
}

pub(crate) fn log_separator_line() {
    log::debug!("----------------------------------------");
}
