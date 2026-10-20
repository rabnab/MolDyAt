// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::objects2d::ball_impl::{Ball2D, Named, SpeciesSet, get_update_hood_call_cnt};

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
        let mut text = if world.real_data {
            world.sim.get_status_text_real(fps, num_balls, dt)
        } else {
            world
                .sim
                .get_status_text(fps, num_balls, get_update_hood_call_cnt(), dt)
        };
        text.push('\n');
        self.write_species_status(&mut text);
        Some(text)
    }

    /// Status lines for the species (#238): the species of new balls, then
    /// one line per species present with its ball count, in table order,
    /// e.g. `New: Ar`, `N2: 120`, `Ar: 80`. One line each, so a narrow
    /// status pane (the energy window cuts lines at its width) never hides a
    /// count.
    #[cfg(test)]
    pub(crate) fn species_status(&self) -> String {
        let mut text = String::new();
        self.write_species_status(&mut text);
        text
    }

    /// [`Self::species_status`], appended to `text`. Runs for every snapshot
    /// while ball stats are shown: one pass over the balls into a fixed-size
    /// array, no allocation besides growing `text`.
    fn write_species_status(&self, text: &mut String) {
        use std::fmt::Write;
        let table = self.world().species();
        let mut counts = [0usize; SpeciesSet::CAPACITY];
        for ball in &self.world().balls {
            if let Some(count) = counts.get_mut(ball.species().index()) {
                *count += 1;
            }
        }
        let new_name = table
            .get(self.new_ball_species())
            .map_or("?", |species| species.name.as_str());
        let _ = write!(text, "New: {new_name}");
        for (species, &count) in table.iter().zip(&counts) {
            if count > 0 {
                let _ = write!(text, "\n{}: {count}", species.name);
            }
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
