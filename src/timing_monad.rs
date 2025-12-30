use std::time::Duration;
#[cfg(feature = "measure-time")]
use std::time::Instant;

pub struct TimingMonad {
    timings: Vec<(String, Duration)>,
}

impl TimingMonad {
    pub fn new() -> Self {
        Self {
            timings: Vec::new(),
        }
    }

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

    #[cfg(feature = "measure-time")]
    #[inline(never)]
    pub fn log(&self) {
        for (label, duration) in &self.timings {
            println!("{}: {:?} µs", label, duration.as_nanos() / 1000);
        }
    }
}
