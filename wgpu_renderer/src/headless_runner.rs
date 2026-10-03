use crate::headless_cli::HeadlessConfig;
use crate::headless_output::{HeadlessSnapshotSink, HeadlessWriter};
use rust_thermo_core::application::persistence::{LoadSource, SaveStore};
use rust_thermo_core::application::{GameLogic, MainContent};
use rust_thermo_core::objects2d::{Rect, Vec2D};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

pub struct HeadlessRunner {
    application: MainContent,
    sink: Box<dyn HeadlessSnapshotSink>,
    steps: u32,
    snapshot_every: u32,
    dt: f32,
}

pub struct HeadlessRunnerBuilder {
    application: Option<MainContent>,
    sink: Option<Box<dyn HeadlessSnapshotSink>>,
    steps: u32,
    snapshot_every: u32,
    dt: f32,
}

pub struct HeadlessRunReport {
    pub snapshots_emitted: u32,
}

impl HeadlessRunnerBuilder {
    pub fn new() -> Self {
        Self {
            application: None,
            sink: None,
            steps: 1,
            snapshot_every: 0,
            dt: Duration::from_millis(1).as_secs_f32(),
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

    pub fn dt(mut self, dt: f32) -> Self {
        self.dt = dt;
        self
    }

    pub fn build(self) -> HeadlessRunner {
        HeadlessRunner {
            application: self.application.expect("application is required"),
            sink: self.sink.expect("sink is required"),
            steps: self.steps,
            snapshot_every: self.snapshot_every,
            dt: self.dt,
        }
    }
}

impl HeadlessRunner {
    pub fn builder() -> HeadlessRunnerBuilder {
        HeadlessRunnerBuilder::new()
    }

    pub fn from_config(mut config: HeadlessConfig, application: MainContent) -> io::Result<Self> {
        // Record the effective time step in run_start, also when it is the default.
        config.dt = Some(application.time_step());
        let sink = HeadlessWriter::builder()
            .stdout(config.stdout)
            .output(config.output.clone())
            .config(config.clone());
        Ok(Self::builder()
            .application(application)
            .sink(Box::new(sink.build()?))
            .steps(config.steps)
            .snapshot_every(config.snapshot_every)
            .build())
    }

    pub fn run(mut self) -> io::Result<HeadlessRunReport> {
        let mut snapshots_emitted = 0;
        let mut latest_snapshot = self.application.create_snapshot();
        self.sink.write_run_start(&latest_snapshot)?;
        self.sink.write_frame(&latest_snapshot)?;
        snapshots_emitted += 1;
        for step in 0..self.steps {
            self.application
                .simulation_step()
                .map_err(|error| io::Error::other(format!("{error:?}")))?;
            if step + 1 == self.steps
                || (self.snapshot_every != 0 && (step + 1) % self.snapshot_every == 0)
            {
                latest_snapshot = self.application.create_snapshot();
                self.sink.write_frame(&latest_snapshot)?;
                snapshots_emitted += 1;
            }
        }
        self.sink
            .write_run_end(&latest_snapshot, snapshots_emitted)?;
        Ok(HeadlessRunReport { snapshots_emitted })
    }
}

/// The world `rect` with width and height multiplied by `scale`, around its centre.
pub fn scale_world_rect(rect: Rect, scale: f32) -> Rect {
    let (w, h) = (rect.w * scale, rect.h * scale);
    Rect::new(
        rect.x + (rect.w - w) / 2.0,
        rect.y + (rect.h - h) / 2.0,
        w,
        h,
    )
}

pub fn build_application(world_rect: Rect, rng_seed: Option<u64>) -> io::Result<MainContent> {
    let application = if let Some(seed) = rng_seed {
        MainContent::new_with_seed(world_rect, seed)
    } else {
        MainContent::new(world_rect)
    };
    application.map_err(|error| io::Error::other(format!("{error:?}")))
}

/// Build the application for a headless run: time step, then walls, then
/// seeded balls (so seeding never places a ball inside a wall).
pub fn prepare_application(
    config: &HeadlessConfig,
    world_rect: Rect,
    store: &SaveStore,
    load: Option<PathBuf>,
) -> io::Result<MainContent> {
    let mut application = build_application(world_rect, config.rng_seed)?;
    if let Some(path) = load {
        // Same entry point as Ctrl+O, but running: a headless run has no
        // one to look at a paused world.
        application
            .load_world(store, &LoadSource::Path(path), false)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
    }
    if let Some(dt) = config.dt {
        application
            .set_time_step(dt)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
    }
    for segment in &config.line_segments {
        application.add_line_segment_with_thickness(
            Vec2D::new(segment.start.0, segment.start.1),
            Vec2D::new(segment.end.0, segment.end.1),
            segment.thickness,
        );
    }
    if let Some(seed_balls) = config.seed_balls {
        application
            .seed_random_balls_with_speed(seed_balls as usize, config.seed_speed.unwrap_or(0.0));
    }
    Ok(application)
}

#[cfg(test)]
mod tests;
