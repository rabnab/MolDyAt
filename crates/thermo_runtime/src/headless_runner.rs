// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::CreateSnapshot;
use crate::headless_cli::HeadlessConfig;
use crate::headless_output::{HeadlessSnapshotSink, HeadlessWriter};
use crate::headless_timings::{RunClock, write_timings};
use std::io;
use std::path::PathBuf;
use std::time::Instant;
use thermo_physics::application::persistence::{LoadSource, SaveStore};
use thermo_physics::application::{MainContent, SimConfig};
use thermo_physics::objects2d::{Rect, Vec2D};

pub struct HeadlessRunner {
    application: MainContent,
    sink: Box<dyn HeadlessSnapshotSink>,
    steps: u32,
    snapshot_every: u32,
    timings: Option<PathBuf>,
    timing_warmup: u32,
    /// Git commit of the binary, for the timings report (`--timings`).
    commit: &'static str,
}

pub struct HeadlessRunnerBuilder {
    application: Option<MainContent>,
    sink: Option<Box<dyn HeadlessSnapshotSink>>,
    steps: u32,
    snapshot_every: u32,
    timings: Option<PathBuf>,
    timing_warmup: u32,
    commit: &'static str,
}

pub struct HeadlessRunReport {
    /// Snapshots written (the binary writes the count to `run_end` itself).
    pub snapshots_emitted: u32,
}

impl Default for HeadlessRunnerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadlessRunnerBuilder {
    pub fn new() -> Self {
        Self {
            application: None,
            sink: None,
            steps: 1,
            snapshot_every: 0,
            timings: None,
            timing_warmup: 0,
            commit: UNKNOWN_COMMIT,
        }
    }

    pub fn application(mut self, application: MainContent) -> Self {
        self.application = Some(application);
        self
    }

    pub fn sink(mut self, sink: Box<dyn HeadlessSnapshotSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    pub fn steps(mut self, steps: u32) -> Self {
        self.steps = steps;
        self
    }

    pub fn snapshot_every(mut self, snapshot_every: u32) -> Self {
        self.snapshot_every = snapshot_every;
        self
    }

    /// Write per-phase step timings to `path`, from step `warmup` on.
    pub fn timings(mut self, path: Option<PathBuf>, warmup: u32) -> Self {
        self.timings = path;
        self.timing_warmup = warmup;
        self
    }

    /// The git commit of the binary that runs, named in the timings report
    /// (`--timings`, read by `scripts/headless_perf.sh`). The binary bakes it
    /// in at build time; without this call the report says `unknown`.
    pub fn commit(mut self, commit: &'static str) -> Self {
        self.commit = commit;
        self
    }

    pub fn build(self) -> HeadlessRunner {
        HeadlessRunner {
            application: self.application.expect("application is required"),
            sink: self.sink.expect("sink is required"),
            steps: self.steps,
            snapshot_every: self.snapshot_every,
            timings: self.timings,
            timing_warmup: self.timing_warmup,
            commit: self.commit,
        }
    }
}

impl HeadlessRunner {
    pub fn builder() -> HeadlessRunnerBuilder {
        HeadlessRunnerBuilder::new()
    }

    /// The run `config` asks for, of `application`, by the binary built from
    /// git commit `commit` (named in the timings report).
    pub fn from_config(
        mut config: HeadlessConfig,
        application: MainContent,
        commit: &'static str,
    ) -> io::Result<Self> {
        // Record the effective time step and run settings in run_start, also
        // when they are the defaults.
        config.dt = Some(application.time_step());
        config.sim = Some(application.config());
        let sink = HeadlessWriter::builder()
            .stdout(config.stdout)
            .output(config.output.clone())
            .config(config.clone());
        Ok(Self::builder()
            .application(application)
            .sink(Box::new(sink.build()?))
            .steps(config.steps)
            .snapshot_every(config.snapshot_every)
            .timings(config.timings.clone(), config.timing_warmup)
            .commit(commit)
            .build())
    }

    pub fn run(mut self) -> io::Result<HeadlessRunReport> {
        let mut snapshots_emitted = 0;
        let mut latest_snapshot = self.application.create_snapshot();
        self.sink.write_run_start(&latest_snapshot)?;
        self.sink.write_frame(&latest_snapshot)?;
        snapshots_emitted += 1;
        let mut clock = RunClock::start(0);
        for step in 0..self.steps {
            if step == self.timing_warmup {
                self.application.reset_phase_timings();
                clock = RunClock::start(step);
            }
            self.application
                .simulation_step()
                .map_err(|error| io::Error::other(format!("{error:?}")))?;
            clock.steps += 1;
            // Fail at the first paused step, not after the whole run (see
            // `write_timings`). Headless runs never pause today; defensive.
            if self.timings.is_some()
                && step >= self.timing_warmup
                && self.application.phase_timings().steps != clock.steps
            {
                return Err(io::Error::other(format!(
                    "--timings: step {step} did not run (simulation paused)"
                )));
            }
            if step + 1 == self.steps
                || (self.snapshot_every != 0 && (step + 1) % self.snapshot_every == 0)
            {
                let snapshot_start = Instant::now();
                latest_snapshot = self.application.create_snapshot();
                let snapshot_done = Instant::now();
                self.sink.write_frame(&latest_snapshot)?;
                clock.snapshot += snapshot_done - snapshot_start;
                clock.output += snapshot_done.elapsed();
                clock.snapshots += 1;
                snapshots_emitted += 1;
            }
        }
        let wall = clock.started.elapsed();
        self.sink
            .write_run_end(&latest_snapshot, snapshots_emitted)?;
        if let Some(path) = &self.timings {
            write_timings(
                path,
                self.commit,
                &clock,
                wall,
                self.application.phase_timings(),
                &latest_snapshot,
            )?;
        }
        Ok(HeadlessRunReport { snapshots_emitted })
    }
}

/// Commit named in the timings report when the binary sets none.
pub const UNKNOWN_COMMIT: &str = "unknown";

pub fn build_application(
    world_rect: Rect,
    sim_config: &SimConfig,
    rng_seed: Option<u64>,
) -> io::Result<MainContent> {
    MainContent::with_config(world_rect, sim_config, rng_seed)
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

/// Build the application for a headless run from `sim_config` (its
/// `world.rect()` is the world box), or from the loaded save and its config,
/// with `--dt` overriding either: time step, then walls, then seeded balls
/// (so seeding never places a ball inside a wall).
pub fn prepare_application(
    config: &HeadlessConfig,
    sim_config: &SimConfig,
    store: &SaveStore,
    load: Option<PathBuf>,
) -> io::Result<MainContent> {
    let mut sim_config = sim_config.clone();
    if let Some(dt) = config.dt {
        sim_config.integration.time_step = dt;
    }
    let mut application = build_application(sim_config.world.rect(), &sim_config, config.rng_seed)?;
    if let Some(path) = load {
        // Same entry point as Ctrl+O, but running: a headless run has no
        // one to look at a paused world.
        application
            .load_world(store, &LoadSource::Path(path), false)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
    }
    // `--dt` also beats the time step of a loaded save.
    if let Some(dt) = config.dt {
        application
            .set_time_step(dt)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
    }
    let line_ids: Vec<usize> = config
        .line_segments
        .iter()
        .map(|segment| {
            application.add_line_segment_with_thickness(
                Vec2D::new(segment.start.0, segment.start.1),
                Vec2D::new(segment.end.0, segment.end.1),
                segment.thickness,
            )
        })
        .collect();
    for valve in &config.valves {
        line_ids
            .get(valve.wall)
            .and_then(|&line_id| {
                application.add_valve_segment(line_id, valve.t_start, valve.t_end, valve.is_closed)
            })
            .ok_or_else(|| {
                io::Error::other(format!("cannot place --valve on wall {}", valve.wall))
            })?;
    }
    if let Some(seed_balls) = config.seed_balls {
        let speed = config.seed_speed.unwrap_or(0.0);
        if config.seed_species.is_empty() {
            application.seed_random_balls_with_speed(seed_balls as usize, speed);
        } else {
            application
                .seed_random_mixture(seed_balls as usize, speed, &config.seed_species)
                .map_err(|error| io::Error::other(format!("--seed-species: {error:?}")))?;
        }
    }
    Ok(application)
}

#[cfg(test)]
mod tests;
