// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::application::sim_stats::StatusNumbers;
use crate::objects2d::ball_impl::{Ball2D, Named, SpeciesSet, get_update_hood_call_cnt};

impl MainContent {
    /// The numbers of the status lines, or `None` while ball stats are not
    /// shown (`B`). The UI formats them (#341).
    pub fn status_numbers(&self, dt: f32) -> Option<StatusNumbers> {
        if !self.options().show_ball_stats {
            return None;
        }
        Some(self.world().sim.status_numbers(
            self.options().real_data,
            dt,
            get_update_hood_call_cnt(),
        ))
    }
}

/// Ball count per species index.
pub type SpeciesCounts = [usize; SpeciesSet::CAPACITY];

/// Add `ball` to `counts`.
#[inline]
pub fn count_ball(counts: &mut SpeciesCounts, ball: &Ball2D) {
    if let Some(count) = counts.get_mut(ball.species().index()) {
        *count += 1;
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
