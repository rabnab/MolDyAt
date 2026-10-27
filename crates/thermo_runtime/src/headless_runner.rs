// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::CreateSnapshot;
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::export::{
    ExportFormat, ExportKind, SampleStream, StreamSettings, series_to_string, snapshot_to_string,
    write_export,
};
use crate::headless_cli::HeadlessConfig;
use crate::headless_output::{HeadlessSnapshotSink, HeadlessWriter};
use crate::headless_script::CommandRecord;
use crate::headless_script::Script;
use crate::headless_timings::{RunClock, write_timings};
use crate::metrics::{MetricsRegistry, SharedMetricsRegistry, read_metrics, write_metrics};
use script_run::ScriptRun;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
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
    /// Shared metrics registry for time-series data; without one (tests,
    /// custom sinks) nothing is sampled.
    metrics: Option<SharedMetricsRegistry>,
    export: Option<ExportRun>,
    /// The `--script` commands still to run (#340).
    script: Option<ScriptRun>,
}

pub struct HeadlessRunnerBuilder {
    application: Option<MainContent>,
    sink: Option<Box<dyn HeadlessSnapshotSink>>,
    steps: u32,
    snapshot_every: u32,
    timings: Option<PathBuf>,
    timing_warmup: u32,
    commit: &'static str,
    metrics: Option<SharedMetricsRegistry>,
    export: Option<ExportSettings>,
    script: Option<Script>,
}

/// Where and how a run writes its export files (#337).
#[derive(Debug, Clone)]
pub struct ExportSettings {
    pub dir: PathBuf,
    pub format: ExportFormat,
    /// Series to export; `None` exports all.
    pub series: Option<Vec<String>>,
    /// Also stream every frame's sample to `stream.<ext>` (#366).
    pub stream: bool,
}

/// The series of a run being exported (`--export-dir`).
///
/// The export has its own registry, sampled at every k-th frame and at the
/// last one, with k chosen from the known run length so that the whole run
/// fits into the ring buffers; the frames' own `metrics` keep following
/// every frame. The frame interval (`--snapshot-every`) is unchanged.
struct ExportRun {
    settings: ExportSettings,
    registry: SharedMetricsRegistry,
    /// Sample every `stride`-th frame (and the last one).
    stride: u64,
    /// Frames sampled or skipped so far.
    frames: u64,
    /// `--export-stream`, fed every frame.
    stream: Option<StreamRun>,
}

/// The `--export-stream` file and the registry it is fed from: every frame,
/// not thinned like the series export.
struct StreamRun {
    /// Holds just the newest samples; the stream writes each one as it comes.
    registry: MetricsRegistry,
    /// `None` once the stream has failed or ended.
    stream: Option<SampleStream>,
    files: Vec<PathBuf>,
    /// The first write error; the run goes on and returns it at the end.
    error: Option<io::Error>,
}

impl StreamRun {
    fn new(settings: &ExportSettings) -> Self {
        let registry = MetricsRegistry::new(2);
        let stream = SampleStream::new(
            StreamSettings {
                dir: settings.dir.clone(),
                stem: "stream".into(),
                format: settings.format,
                replace_existing: true,
            },
            &registry,
        );
        Self {
            registry,
            stream: Some(stream),
            files: Vec::new(),
            error: None,
        }
    }

    fn sample(&mut self, snapshot: &RendererSnapshot) {
        self.registry.sample(snapshot);
        if let Some(stream) = &mut self.stream {
            match stream.record(&self.registry) {
                Ok(started) => self.files.extend(started),
                Err(error) => self.fail(error),
            }
        }
    }

    /// Stop the stream; the simulation goes on.
    fn fail(&mut self, error: io::Error) {
        log::error!("--export-stream stopped: {error}; the run goes on");
        self.stream = None;
        self.error = Some(io::Error::new(
            error.kind(),
            format!("--export-stream: {error}"),
        ));
    }

    /// Flush and close the file; the files written, or the first error.
    fn finish(&mut self) -> io::Result<Vec<PathBuf>> {
        if let Some(stream) = self.stream.take()
            && let Err(error) = stream.finish()
        {
            self.fail(error);
        }
        match self.error.take() {
            Some(error) => Err(error),
            None => Ok(std::mem::take(&mut self.files)),
        }
    }
}

impl ExportRun {
    /// `steps` steps with a frame every `snapshot_every` steps (0: the first
    /// and the last only) give `1 + steps / every (+ 1)` frames.
    fn new(settings: ExportSettings, steps: u32, snapshot_every: u32) -> Self {
        let registry = MetricsRegistry::shared();
        let capacity = read_metrics(&registry).capacity() as u64;
        let frames = match (steps, snapshot_every) {
            (0, _) => 1,
            (_, 0) => 2,
            (steps, every) => 1 + u64::from(steps / every) + u64::from(steps % every != 0),
        };
        // One slot stays free for the last frame, which is always sampled.
        let stride = if frames <= capacity {
            1
        } else {
            frames.div_ceil(capacity - 1)
        };
        Self {
            stream: settings.stream.then(|| StreamRun::new(&settings)),
            settings,
            registry,
            stride,
            frames: 0,
        }
    }

    /// Steps between two samples when the run sampled only every k-th frame.
    fn sample_interval_steps(&self, snapshot_every: u32) -> Option<u64> {
        (self.stride > 1).then(|| self.stride * u64::from(snapshot_every))
    }

    fn sample(&mut self, snapshot: &RendererSnapshot, last: bool) {
        if last || self.frames.is_multiple_of(self.stride) {
            write_metrics(&self.registry).sample(snapshot);
        }
        if let Some(stream) = &mut self.stream {
            stream.sample(snapshot);
        }
        self.frames += 1;
    }

    /// Fail before the first step on a series name that does not exist: the
    /// built-in series and the species counts all exist after the first
    /// sample.
    fn check_series(&self) -> io::Result<()> {
        let registry = read_metrics(&self.registry);
        match self
            .settings
            .series
            .iter()
            .flatten()
            .find(|name| registry.get_series(name).is_none())
        {
            Some(unknown) => Err(io::Error::other(format!(
                "--export-series: unknown series {unknown}"
            ))),
            None => Ok(()),
        }
    }
}

pub struct HeadlessRunReport {
    /// Snapshots written (the binary writes the count to `run_end` itself).
    pub snapshots_emitted: u32,
    /// Files written by `--export-dir`, series first.
    pub exported: Vec<PathBuf>,
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
            metrics: None,
            export: None,
            script: None,
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

    /// Shared metrics registry for time-series data.
    pub fn metrics(mut self, metrics: SharedMetricsRegistry) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Write the series and the final snapshot when the run ends. Needs a
    /// registry (`.metrics()`).
    pub fn export(mut self, export: ExportSettings) -> Self {
        self.export = Some(export);
        self
    }

    /// Run the commands of `script` at their steps (#340). The run length
    /// stays what `.steps()` sets; `from_config` sets it from the script.
    pub fn script(mut self, script: Option<Script>) -> Self {
        self.script = script;
        self
    }

    fn export_if(mut self, export: Option<ExportSettings>) -> Self {
        self.export = export;
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
            metrics: self.metrics,
            export: self
                .export
                .map(|settings| ExportRun::new(settings, self.steps, self.snapshot_every)),
            script: self.script.as_ref().map(ScriptRun::new),
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

        let metrics = MetricsRegistry::shared();

        let sink = HeadlessWriter::builder()
            .stdout(config.stdout)
            .output(config.output.clone())
            .config(config.clone())
            .metrics(Arc::clone(&metrics));
        Ok(Self::builder()
            .application(application)
            .sink(Box::new(sink.build()?))
            .steps(config.steps)
            .snapshot_every(config.snapshot_every)
            .timings(config.timings.clone(), config.timing_warmup)
            .commit(commit)
            .metrics(metrics)
            .script(config.script.clone())
            .export_if(config.export_dir.clone().map(|dir| ExportSettings {
                dir,
                format: config.export_format,
                series: config.export_series.clone(),
                stream: config.export_stream,
            }))
            .build())
    }

    /// The export files of `--export-dir`: the series of the whole run and
    /// the per-ball state of the final snapshot, under fixed names so scripts
    /// find them, then the closed `--export-stream` file. A failed stream is
    /// returned as the error once the other files are written.
    fn write_exports(&mut self, snapshot: &RendererSnapshot) -> io::Result<Vec<PathBuf>> {
        let Some(export) = &mut self.export else {
            return Ok(Vec::new());
        };
        let settings = &export.settings;
        let series = series_to_string(
            &read_metrics(&export.registry),
            settings.series.as_deref(),
            settings.format,
            export.sample_interval_steps(self.snapshot_every),
        );
        let snapshot = snapshot_to_string(snapshot, settings.format);
        let mut written = [
            (ExportKind::Series, series),
            (ExportKind::Snapshot, snapshot),
        ]
        .into_iter()
        .map(|(kind, text)| write_export(&settings.dir, "", kind, settings.format, &text, false))
        .collect::<io::Result<Vec<_>>>()?;
        if let Some(stream) = &mut export.stream {
            written.extend(stream.finish()?);
        }
        Ok(written)
    }

    fn sample_metrics(&self, snapshot: &RendererSnapshot) {
        if let Some(metrics) = &self.metrics {
            write_metrics(metrics).sample(snapshot);
        }
    }

    /// Run the script commands due after `steps_done` steps; their records
    /// go to the sink unless `hold` takes them (the commands before the
    /// first step run before `run_start` is written).
    fn run_script(
        &mut self,
        steps_done: u32,
        hold: Option<&mut Vec<CommandRecord>>,
    ) -> io::Result<()> {
        let Some(script) = &mut self.script else {
            return Ok(());
        };
        let records = script.run_due(steps_done, &mut self.application, self.metrics.as_ref())?;
        match hold {
            Some(held) => held.extend(records),
            None => {
                for record in &records {
                    self.sink.write_command(record)?;
                }
            }
        }
        Ok(())
    }

    pub fn run(mut self) -> io::Result<HeadlessRunReport> {
        let mut snapshots_emitted = 0;
        // Setup commands run before the first snapshot, so `run_start` and
        // the frame at step 0 show what they placed.
        let mut setup = Vec::new();
        self.run_script(0, Some(&mut setup))?;
        let mut latest_snapshot = self.application.create_snapshot();

        self.sample_metrics(&latest_snapshot);
        if let Some(export) = &mut self.export {
            export.sample(&latest_snapshot, self.steps == 0);
            export.check_series()?;
        }

        self.sink.write_run_start(&latest_snapshot)?;
        for record in &setup {
            self.sink.write_command(record)?;
        }
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
                // Sampling is not part of building a snapshot: it is timed as
                // output, not as snapshot, like writing the frame.
                self.sample_metrics(&latest_snapshot);
                if let Some(export) = &mut self.export {
                    export.sample(&latest_snapshot, step + 1 == self.steps);
                }
                self.sink.write_frame(&latest_snapshot)?;
                clock.snapshot += snapshot_done - snapshot_start;
                clock.output += snapshot_done.elapsed();
                clock.snapshots += 1;
                snapshots_emitted += 1;
            }
            // After the step's frame, so a frame shows the world before the
            // commands of its step.
            self.run_script(step + 1, None)?;
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
        // Last, so an export error cannot cost the run its `run_end` record
        // or its timings report.
        let exported = self.write_exports(&latest_snapshot)?;
        Ok(HeadlessRunReport {
            snapshots_emitted,
            exported,
        })
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

mod script_run;

#[cfg(test)]
mod tests;
