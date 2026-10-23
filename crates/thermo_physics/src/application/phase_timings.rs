// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wall-clock time spent in each phase of the physics step (#221).
//!
//! Always on: a step takes milliseconds and the timers cost one
//! `Instant::now()` per [`PhaseClock::lap`] plus one per
//! [`PhaseClock::start`], so there is no feature gate. Headless
//! perf runs (`--timings`, `scripts/headless_perf.sh`) read and reset them;
//! nothing in the simulation depends on them.
//!
//! [`PhaseClock`] is the only timer of the step: each phase boundary is one
//! [`PhaseClock::lap`], which books the time to [`PhaseTimings`] and, with
//! the `measure-time` feature and the timing display (`F4`) on, also logs it
//! on [`crate::logging::TIMING_TARGET`].

use std::time::{Duration, Instant};

/// A phase of the physics step, as reported in [`PhaseTimings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Neighbours,
    Positions,
    Forces,
    Apply,
    Cells,
    Thermo,
}

impl Phase {
    /// Every phase, in step order.
    #[cfg(test)]
    pub(crate) const ALL: [Phase; 6] = [
        Phase::Neighbours,
        Phase::Positions,
        Phase::Forces,
        Phase::Apply,
        Phase::Cells,
        Phase::Thermo,
    ];

    /// Name in the timing log.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Phase::Neighbours => "neighbours",
            Phase::Positions => "positions",
            Phase::Forces => "forces",
            Phase::Apply => "apply",
            Phase::Cells => "cells",
            Phase::Thermo => "thermo",
        }
    }
}

/// Time summed over all steps since the last [`PhaseTimings::reset`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PhaseTimings {
    /// Steps that ran (paused steps do not count).
    pub steps: u64,
    /// Verlet list rebuilds and neighbour statistics.
    pub neighbours: Duration,
    /// Position update with wall contacts, and the kernel point copy.
    pub positions: Duration,
    /// Pair forces (SIMD kernel).
    pub forces: Duration,
    /// Velocity update, stats (including the wall-contact counts), wall
    /// overlaps, thermostats.
    pub apply: Duration,
    /// Source spheres, cell grid moves and removal of invalid balls.
    pub cells: Duration,
    /// Global temperature and the per-cell T/P fields.
    pub thermo: Duration,
}

impl PhaseTimings {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Time booked to `phase`.
    #[cfg(test)]
    pub(crate) fn get(&self, phase: Phase) -> Duration {
        match phase {
            Phase::Neighbours => self.neighbours,
            Phase::Positions => self.positions,
            Phase::Forces => self.forces,
            Phase::Apply => self.apply,
            Phase::Cells => self.cells,
            Phase::Thermo => self.thermo,
        }
    }

    /// Book `elapsed` to `phase`.
    pub(crate) fn add(&mut self, phase: Phase, elapsed: Duration) {
        let slot = match phase {
            Phase::Neighbours => &mut self.neighbours,
            Phase::Positions => &mut self.positions,
            Phase::Forces => &mut self.forces,
            Phase::Apply => &mut self.apply,
            Phase::Cells => &mut self.cells,
            Phase::Thermo => &mut self.thermo,
        };
        *slot += elapsed;
    }

    /// Sum of all phases.
    pub fn total(&self) -> Duration {
        self.neighbours + self.positions + self.forces + self.apply + self.cells + self.thermo
    }
}

/// Times consecutive phases of the step: each [`Self::lap`] books the time
/// since the previous lap (or since [`Self::start`]) to one phase.
pub(crate) struct PhaseClock {
    last: Instant,
    /// Also log each lap (timing display, `F4`). Only takes effect with the
    /// `measure-time` feature; without it the log call is compiled out.
    log: bool,
}

impl PhaseClock {
    pub(crate) fn start(log: bool) -> Self {
        Self {
            last: Instant::now(),
            log,
        }
    }

    /// Book the time since the last lap to `phase`.
    pub(crate) fn lap(&mut self, timings: &mut PhaseTimings, phase: Phase) {
        self.lap_as(timings, phase, phase.label());
    }

    /// [`Self::lap`] with its own name in the timing log, for a part of a
    /// phase that is timed separately (the time still goes to `phase`).
    pub(crate) fn lap_as(&mut self, timings: &mut PhaseTimings, phase: Phase, label: &str) {
        let now = Instant::now();
        let elapsed = now - self.last;
        self.last = now;
        timings.add(phase, elapsed);
        if cfg!(feature = "measure-time") && self.log {
            log::info!(
                target: crate::logging::TIMING_TARGET,
                "{}: {} µs",
                label,
                elapsed.as_micros()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Phase, PhaseClock, PhaseTimings};
    use std::time::{Duration, Instant};

    #[test]
    fn all_lists_every_phase_once_in_order() {
        for (i, phase) in Phase::ALL.into_iter().enumerate() {
            // Exhaustive on purpose: a new variant fails to compile here
            // until it is added to `Phase::ALL`.
            let position = match phase {
                Phase::Neighbours => 0,
                Phase::Positions => 1,
                Phase::Forces => 2,
                Phase::Apply => 3,
                Phase::Cells => 4,
                Phase::Thermo => 5,
            };
            assert_eq!(position, i, "{phase:?}");
        }
    }

    #[test]
    fn add_books_each_phase_to_its_own_field() {
        let mut timings = PhaseTimings::default();
        for (i, phase) in Phase::ALL.into_iter().enumerate() {
            timings.add(phase, Duration::from_micros(1 << i));
        }
        for (i, phase) in Phase::ALL.into_iter().enumerate() {
            assert_eq!(
                timings.get(phase),
                Duration::from_micros(1 << i),
                "{phase:?}"
            );
        }
        assert_eq!(timings.total(), Duration::from_micros(0b11_1111));
    }

    #[test]
    fn laps_book_consecutive_intervals_without_gaps_or_overlap() {
        let mut timings = PhaseTimings::default();
        let before_start = Instant::now();
        let mut clock = PhaseClock::start(false);
        let after_start = Instant::now();
        std::thread::sleep(Duration::from_millis(2));
        clock.lap(&mut timings, Phase::Positions);
        std::thread::sleep(Duration::from_millis(2));
        let before_last_lap = Instant::now();
        clock.lap_as(&mut timings, Phase::Forces, "forces, separately");
        let after_last_lap = Instant::now();

        assert!(timings.positions >= Duration::from_millis(2));
        assert!(timings.forces >= Duration::from_millis(2));
        // No gap: the laps cover at least the span that surely lies between
        // the clock's start and its last lap...
        assert!(timings.total() >= before_last_lap - after_start);
        // ...and no overlap: at most the span around it.
        assert!(timings.total() <= after_last_lap - before_start);
        assert_eq!(timings.steps, 0, "laps do not count steps");
    }
}
