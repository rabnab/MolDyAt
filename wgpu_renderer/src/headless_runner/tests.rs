use super::*;
use crate::headless_cli::{DetailLevel, OutputFormat};
use crate::headless_output::HeadlessSnapshotSink;
use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

const SEEDED_HEADLESS_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/headless_runner/fixtures/seeded_headless_regression.jsonl"
);

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
    struct TempFileGuard {
        path: std::path::PathBuf,
    }

    impl Drop for TempFileGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let unique = format!("{}-{}", std::process::id(), nanos);
    let output_path = std::env::temp_dir().join(format!("headless-regression-{unique}.jsonl"));
    let _output_guard = TempFileGuard {
        path: output_path.clone(),
    };
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

fn non_empty_lines(input: &str) -> Vec<&str> {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn maybe_update_seeded_fixture(actual: &str) {
    if std::env::var_os("UPDATE_HEADLESS_FIXTURE").is_some() {
        fs::write(SEEDED_HEADLESS_FIXTURE_PATH, actual).expect("update seeded fixture");
    }
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
    let actual_text = seeded_headless_output();
    maybe_update_seeded_fixture(&actual_text);
    let expected_text =
        fs::read_to_string(SEEDED_HEADLESS_FIXTURE_PATH).expect("read seeded fixture");

    let actual_records = parse_jsonl_records(&actual_text);
    let expected_records = parse_jsonl_records(&expected_text);
    let actual_lines = non_empty_lines(&actual_text);
    let expected_lines = non_empty_lines(&expected_text);

    if actual_lines != expected_lines {
        let first_diff = actual_lines
            .iter()
            .zip(expected_lines.iter())
            .position(|(actual, expected)| actual != expected);

        match first_diff {
            Some(index) => panic!(
                "seeded headless regression output drifted at line {}\nexpected: {}\nactual:   {}",
                index + 1,
                expected_lines[index],
                actual_lines[index]
            ),
            None => panic!(
                "seeded headless regression output line count drifted: expected {}, actual {}",
                expected_lines.len(),
                actual_lines.len()
            ),
        }
    }

    assert_eq!(
        actual_records, expected_records,
        "seeded headless regression output drifted from the checked-in fixture"
    );
}
