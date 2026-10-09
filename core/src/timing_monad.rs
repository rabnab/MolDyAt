// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::time::Duration;
#[cfg(feature = "measure-time")]
use std::time::Instant;

pub struct TimingMonad {
    timings: Vec<(String, Duration)>,
}

impl Default for TimingMonad {
    fn default() -> Self {
        Self::new()
    }
}

impl TimingMonad {
    /// Create new timing monad for performance measurement.
    pub fn new() -> Self {
        Self {
            timings: Vec::new(),
        }
    }

    /// Execute function and measure execution time.
    /// Only measures when "measure-time" feature is enabled, otherwise zero overhead.
    ///
    /// # Arguments
    /// * `label` - Description for this timing measurement
    /// * `f` - Function to execute and time
    #[cfg(feature = "measure-time")]
    #[inline(never)]
    pub fn time<F, R>(&mut self, label: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        self.timings.push((label.to_string(), duration));
        result
    }

    #[cfg(not(feature = "measure-time"))]
    #[inline(always)]
    pub fn time<F, R>(&mut self, _label: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        f() // Zero overhead
    }
    #[cfg(not(feature = "measure-time"))]
    #[inline(never)]
    pub fn log(&self) {
        // Zero overhead
    }

    #[cfg(feature = "measure-time")]
    #[inline(never)]
    pub fn time_result<F, R, E>(&mut self, label: &str, f: F) -> Result<R, E>
    where
        F: FnOnce() -> Result<R, E>,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        self.timings.push((label.to_string(), duration));
        result
    }

    /// Print all accumulated timing measurements in microseconds.
    /// Only prints when "measure-time" feature is enabled.
    #[cfg(feature = "measure-time")]
    #[inline(never)]
    pub fn log(&self) {
        for (label, duration) in &self.timings {
            println!("{}: {:?} µs", label, duration.as_nanos() / 1000);
        }
    }
}
