// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Run-time options of the physics thread that are not part of the world
//! (#305, plan item M2): how the run is shown and stepped. They live in
//! `MainContent`, so a load, which replaces the world, keeps them, and they
//! are never saved.

/// Display and stepping options, toggled by keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeOptions {
    /// Status text in SI units instead of reduced units (key `D`).
    pub(crate) real_data: bool,
    /// Pause after every step (key `S`).
    pub(crate) step_by_step: bool,
    /// Log the time of every step phase (key `F4`, with the `measure-time`
    /// feature).
    pub(crate) show_timings: bool,
    /// Build the status text of the ball statistics overlay (key `B`).
    pub(crate) show_ball_stats: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            real_data: false,
            step_by_step: false,
            show_timings: false,
            show_ball_stats: true,
        }
    }
}
