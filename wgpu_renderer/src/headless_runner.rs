use crate::headless_cli::HeadlessConfig;
use crate::headless_output::{HeadlessSnapshotSink, HeadlessWriter};
use rust_thermo_core::application::{GameLogic, MainContent, VerletIntegration};
use rust_thermo_core::objects2d::Rect;
use std::io;
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

    pub fn from_config(config: HeadlessConfig, application: MainContent) -> io::Result<Self> {
        let sink = HeadlessWriter::builder()
            .stdout(config.stdout)
            .output(config.output);
        Ok(Self::builder()
            .application(application)
            .sink(Box::new(sink.build()?))
            .steps(config.steps)
            .snapshot_every(config.snapshot_every)
            .build())
    }

    pub fn run(mut self) -> io::Result<HeadlessRunReport> {
        let bound = self.application.world().world_region();
        let mut snapshots_emitted = 0;
        self.sink
            .write_snapshot(&self.application.create_snapshot())?;
        snapshots_emitted += 1;
        for step in 0..self.steps {
            self.application
                .simulation_step()
                .map_err(|error| io::Error::other(format!("{error:?}")))?;
            self.application.update_sim(self.dt, &bound);
            if step + 1 == self.steps
                || (self.snapshot_every != 0 && (step + 1) % self.snapshot_every == 0)
            {
                self.sink
                    .write_snapshot(&self.application.create_snapshot())?;
                snapshots_emitted += 1;
            }
        }
        Ok(HeadlessRunReport { snapshots_emitted })
    }
}

pub fn build_application(world_rect: Rect) -> io::Result<MainContent> {
    MainContent::new(world_rect).map_err(|error| io::Error::other(format!("{error:?}")))
}
