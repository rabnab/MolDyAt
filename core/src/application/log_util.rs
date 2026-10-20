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
        Some(format!("{return_value}\n{}", self.species_status()))
    }

    /// Status lines for the species (#238): the species of new balls, then
    /// the ball count of every species present, in table order, e.g.
    /// `New: Ar` and `N2: 120 | Ar: 80`.
    pub(crate) fn species_status(&self) -> String {
        let table = self.world().species();
        let mut counts = vec![0usize; table.len()];
        for ball in &self.world().balls {
            if let Some(count) = counts.get_mut(ball.species().index()) {
                *count += 1;
            }
        }
        let present: Vec<String> = table
            .iter()
            .zip(&counts)
            .filter(|&(_, &count)| count > 0)
            .map(|(species, count)| format!("{}: {count}", species.name))
            .collect();
        let new_ball_species = self.new_ball_species();
        let new_name = table
            .get(new_ball_species)
            .map_or("?", |species| species.name.as_str());
        if present.is_empty() {
            format!("New: {new_name}")
        } else {
            format!("New: {new_name}\n{}", present.join(" | "))
        }
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
