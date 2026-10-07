//! Wall-clock time spent in each phase of the physics step (#221).
//!
//! Always on: a step takes milliseconds and the timers cost a few
//! `Instant::now()` calls per phase, so there is no feature gate. Headless
//! perf runs (`--timings`, `scripts/headless_perf.sh`) read and reset them;
//! nothing in the simulation depends on them.

use std::time::Duration;

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
    /// Velocity update, stats, wall overlaps, thermostats.
    pub apply: Duration,
    /// Source spheres, cell grid moves and removal of invalid balls.
    pub cells: Duration,
    /// Global temperature and the per-cell T/P fields.
    pub thermo: Duration,
}

impl PhaseTimings {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Sum of all phases.
    pub fn total(&self) -> Duration {
        self.neighbours + self.positions + self.forces + self.apply + self.cells + self.thermo
    }
}
