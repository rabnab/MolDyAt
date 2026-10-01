use super::*;
use crate::headless_cli::{DetailLevel, LineSegmentSpec, OutputFormat};
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
        dt: None,
        world_scale: None,
        seed_speed: None,
        line_segments: Vec::new(),
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
    if std::env::var("UPDATE_HEADLESS_FIXTURE").ok().as_deref() == Some("1") {
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
#[ignore = "suite:headless-regression"]
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

fn test_world_rect() -> rust_thermo_core::objects2d::Rect {
    rust_thermo_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        rust_thermo_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
    )
}

#[test]
fn test_prepare_application_applies_dt_and_line_segments() {
    let config = HeadlessConfig {
        steps: 4,
        rng_seed: Some(12345),
        seed_balls: Some(24),
        dt: Some(0.005),
        line_segments: vec![
            LineSegmentSpec {
                start: (-20.0, 0.0),
                end: (20.0, 0.0),
                thickness: 4.0,
            },
            LineSegmentSpec {
                start: (0.0, -20.0),
                end: (0.0, 20.0),
                thickness: 1.0,
            },
        ],
        ..HeadlessConfig::default()
    };

    let mut application = prepare_application(&config, test_world_rect()).expect("prepare");
    assert_eq!(application.time_step(), 0.005);

    let snapshot = application.create_snapshot();
    let walls = &snapshot.operators.line_segments;
    assert_eq!(walls.len(), 2);
    assert_eq!(walls[0].thickness, 4.0);
    assert_eq!(walls[1].thickness, 1.0);

    // Walls are placed before seeding, so no ball starts inside one.
    for ball in &snapshot.balls {
        for wall in walls {
            let clearance = wall.distance_to_ball(&ball.position, ball.radius);
            assert!(
                clearance >= 0.5 * wall.thickness,
                "ball {} starts inside wall {}",
                ball.id,
                wall.id
            );
        }
    }

    for _ in 0..config.steps {
        application.simulation_step().expect("step");
    }
    let time = application.create_snapshot().time;
    assert!((time - 4.0 * 0.005).abs() < 1e-6, "time {time}");
}

#[test]
fn test_run_start_records_effective_dt_segments_and_world_rect() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let output_path = std::env::temp_dir().join(format!(
        "headless-run-start-{}-{nanos}.jsonl",
        std::process::id()
    ));
    let config = HeadlessConfig {
        output: Some(output_path.clone()),
        stdout: false,
        rng_seed: Some(1),
        seed_speed: Some(2.5),
        world_scale: Some(1.5),
        line_segments: vec![LineSegmentSpec {
            start: (-10.0, 5.0),
            end: (10.0, 5.0),
            thickness: 2.0,
        }],
        ..HeadlessConfig::default()
    };
    let application = prepare_application(&config, test_world_rect()).expect("prepare");
    let default_dt = application.time_step();
    HeadlessRunner::from_config(config, application)
        .expect("runner")
        .run()
        .expect("run");
    let content = fs::read_to_string(&output_path).expect("read output");
    let _ = fs::remove_file(&output_path);

    let run_start = &parse_jsonl_records(&content)[0];
    assert_eq!(run_start["record_kind"], "run_start");
    assert_eq!(run_start["config"]["dt"].as_f64(), Some(default_dt as f64));
    assert_eq!(run_start["config"]["seed_speed"].as_f64(), Some(2.5));
    assert_eq!(run_start["config"]["world_scale"].as_f64(), Some(1.5));
    let segment = &run_start["config"]["line_segments"][0];
    assert_eq!(segment["start"], serde_json::json!([-10.0, 5.0]));
    assert_eq!(segment["end"], serde_json::json!([10.0, 5.0]));
    assert_eq!(segment["thickness"], 2.0);
    assert_eq!(run_start["initial"]["operator_counts"]["line_segments"], 1);
    let world = &run_start["initial"]["world_rect"];
    assert!(world["left"].as_f64() < world["right"].as_f64());
    assert!(world["bottom"].as_f64() < world["top"].as_f64());
}

#[test]
fn test_scale_world_rect_keeps_centre() {
    let rect = Rect::new(-56.0, -31.0, 112.0, 62.0);
    let scaled = scale_world_rect(rect, 5.0);
    assert_eq!((scaled.w, scaled.h), (560.0, 310.0));
    assert_eq!(
        (scaled.x + scaled.w / 2.0, scaled.y + scaled.h / 2.0),
        (0.0, 0.0)
    );
    assert_eq!(scale_world_rect(rect, 1.0), rect);
}

#[test]
fn test_prepare_application_seeds_moving_balls() {
    let config = |seed_speed| HeadlessConfig {
        rng_seed: Some(7),
        seed_balls: Some(20),
        seed_speed,
        ..HeadlessConfig::default()
    };
    let speeds = |seed_speed| {
        let app = prepare_application(&config(seed_speed), test_world_rect()).unwrap();
        let snapshot = app.create_snapshot();
        snapshot
            .balls
            .iter()
            .map(|b| b.velocity.norm())
            .collect::<Vec<f32>>()
    };
    let at_rest = speeds(None);
    assert!(!at_rest.is_empty());
    assert!(at_rest.iter().all(|&v| v == 0.0));
    let moving = speeds(Some(4.0));
    assert!(moving.iter().any(|&v| v > 0.0));
    assert!(moving.iter().all(|&v| v <= 4.0 + 1e-4));
}
