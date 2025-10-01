use std::time::{Duration, Instant};

pub struct TimingMonad {
    timings: Vec<(String, Duration)>,
}

impl TimingMonad {
    pub fn new() -> Self {
        Self {
            timings: Vec::new(),
        }
    }

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

    pub fn log(&self) {
        for (label, duration) in &self.timings {
            println!("{}: {:?} ms", label, duration.as_millis());
        }
    }
}
