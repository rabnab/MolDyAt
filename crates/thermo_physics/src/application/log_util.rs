// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::application::sim_stats::StatusInfo;
use crate::objects2d::ball_impl::{Ball2D, Named, SpeciesSet};

impl MainContent {
    /// What the status lines need besides the observable values, or `None`
    /// while ball stats are not shown (`B`). The UI formats them (#341).
    pub fn status_info(&self) -> Option<StatusInfo> {
        self.options().show_ball_stats.then(|| StatusInfo {
            real_units: self.options().real_data,
            dt_limited_by: self
                .time_step_limit()
                .filter(|_| self.active_time_step() < self.time_step())
                .map(|(_, id)| id),
        })
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
            ball.cold.hood.nearest,
            ball.cold.hood.farthest
        );
    }
}

pub(crate) fn log_separator_line() {
    log::debug!("----------------------------------------");
}
