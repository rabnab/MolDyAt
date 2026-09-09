use super::*;
use crate::headless_cli::{DetailLevel, OutputFormat};
use crate::headless_output::HeadlessSnapshotSink;
use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

struct RecordingSink(Rc<RefCell<Vec<u64>>>);

impl HeadlessSnapshotSink for RecordingSink {
    fn write_run_start(&mut self, _snapshot: &RendererSnapshot) -> io::Result<()> {
        Ok(())
    }

    fn write_frame(&mut self, snapshot: &RendererSnapshot) -> io::Result<()> {
        self.0.borrow_mut().push(snapshot.simulation_step);
        Ok(())
    }

    fn write_run_end(
        &mut self,
        _final_snapshot: &RendererSnapshot,
        _snapshots_emitted: u32,
    ) -> io::Result<()> {
        Ok(())
    }
}

fn runner_with_snapshot_steps(
    steps: u32,
    snapshot_every: u32,
) -> (HeadlessRunner, Rc<RefCell<Vec<u64>>>) {
    let world_rect = rust_thermo_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        rust_thermo_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
    );
    let application = build_application(world_rect, None).expect("application");
    let recorded = Rc::new(RefCell::new(Vec::new()));
    let sink = RecordingSink(recorded.clone());
    let runner = HeadlessRunner::builder()
        .application(application)
        .sink(Box::new(sink))
        .steps(steps)
        .snapshot_every(snapshot_every)
        .build();
    (runner, recorded)
}

fn seeded_headless_output() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let unique = format!(
        "{}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed"),
        nanos
    );
    let output_path = std::env::temp_dir().join(format!("headless-regression-{unique}.jsonl"));

    let world_rect = rust_thermo_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        rust_thermo_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
    );

    let config = HeadlessConfig {
        steps: 6,
        snapshot_every: 2,
        output: Some(output_path.clone()),
        format: OutputFormat::Jsonl,
        detail: DetailLevel::Summary,
        rng_seed: Some(12345),
        stdout: false,
        seed_balls: Some(24),
    };

    let mut application =
        build_application(world_rect, config.rng_seed).expect("build deterministic app");
    application.seed_random_balls(config.seed_balls.expect("seed ball count") as usize);

    let report = HeadlessRunner::from_config(config, application)
        .expect("runner")
        .run()
        .expect("run");
    assert_eq!(report.snapshots_emitted, 4);

    let content = fs::read_to_string(&output_path).expect("read regression output");
    let _ = fs::remove_file(&output_path);
    content
}

fn parse_jsonl_records(input: &str) -> Vec<serde_json::Value> {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid jsonl record"))
        .collect()
}

#[test]
fn test_headless_runner_emits_step_zero_and_final() {
    let (runner, recorded) = runner_with_snapshot_steps(1, 0);
    let report = runner.run().expect("run");
    assert_eq!(report.snapshots_emitted, 2);
    assert_eq!(*recorded.borrow(), vec![0, 1]);
}

#[test]
fn test_headless_runner_emits_interval_and_final() {
    let (runner, recorded) = runner_with_snapshot_steps(3, 2);
    let report = runner.run().expect("run");
    assert_eq!(report.snapshots_emitted, 3);
    assert_eq!(*recorded.borrow(), vec![0, 2, 3]);
}

#[test]
fn test_headless_runner_zero_interval_skips_periodic_snapshots() {
    let (runner, recorded) = runner_with_snapshot_steps(3, 0);
    let report = runner.run().expect("run");
    assert_eq!(report.snapshots_emitted, 2);
    assert_eq!(*recorded.borrow(), vec![0, 3]);
}

#[test]
fn test_seeded_headless_regression_fixture_matches_checked_in_jsonl() {
    let actual = parse_jsonl_records(&seeded_headless_output());
    let expected = parse_jsonl_records(include_str!("fixtures/seeded_headless_regression.jsonl"));

    assert_eq!(
        actual, expected,
        "seeded headless regression output drifted from the checked-in fixture"
    );
}
