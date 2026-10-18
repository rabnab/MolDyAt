// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::headless_cli::{DetailLevel, LineSegmentSpec, OutputFormat, ValveSpec};
use crate::headless_output::HeadlessSnapshotSink;
use moldyat_core::application::operator_command::OperatorCommand;
use moldyat_core::application::operator_command::source::{
    AdjustSourceSphereEmitPeriod, AdjustSourceSphereSpeed, PlaceSourceSphere, ToggleSourceSphere,
};
use moldyat_core::application::operator_command::thermostat::{PlaceThermostat, ToggleThermostat};
use moldyat_core::application::renderer_snapshot::RendererSnapshot;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SEEDED_HEADLESS_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/headless_runner/fixtures/seeded_headless_regression.jsonl"
);
const SEEDED_OPERATORS_FIXTURE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/headless_runner/fixtures/seeded_headless_operators.jsonl"
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
    let world_rect = moldyat_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        moldyat_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
    );
    let application =
        build_application(world_rect, &SimConfig::default(), None).expect("application");
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

/// Runs `config` on `application` with the JSONL written to a temporary
/// file; returns its content and the number of snapshots emitted.
fn headless_output(mut config: HeadlessConfig, application: MainContent) -> (String, u32) {
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
    config.output = Some(output_path.clone());
    config.stdout = false;

    let report = HeadlessRunner::from_config(config, application)
        .expect("runner")
        .run()
        .expect("run");
    let content = fs::read_to_string(&output_path).expect("read regression output");
    (content, report.snapshots_emitted)
}

fn seeded_headless_output() -> String {
    let world_rect = moldyat_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        moldyat_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
    );

    let config = HeadlessConfig {
        steps: 6,
        snapshot_every: 2,
        output: None,
        format: OutputFormat::Jsonl,
        detail: DetailLevel::Summary,
        rng_seed: Some(12345),
        stdout: false,
        seed_balls: Some(24),
        dt: None,
        world_scale: None,
        seed_speed: None,
        line_segments: Vec::new(),
        valves: Vec::new(),
        timings: None,
        timing_warmup: 0,
        save_setup: false,
    };

    let mut application = build_application(world_rect, &SimConfig::default(), config.rng_seed)
        .expect("build deterministic app");
    application.seed_random_balls(config.seed_balls.expect("seed ball count") as usize);

    let (content, snapshots_emitted) = headless_output(config, application);
    assert_eq!(snapshots_emitted, 4);
    content
}

/// Balls seeded in the operator scenario, before the source sphere adds more.
const OPERATORS_SEED_BALLS: u32 = 32;

/// Config of the seeded operator scenario: two walls and an open valve in a
/// quarter-size world, so that balls reach the operators within the run.
fn operators_config(steps: u32, snapshot_every: u32) -> HeadlessConfig {
    HeadlessConfig {
        steps,
        snapshot_every,
        detail: DetailLevel::Balls,
        rng_seed: Some(222),
        seed_balls: Some(OPERATORS_SEED_BALLS),
        seed_speed: Some(3.0),
        world_scale: Some(0.25),
        line_segments: vec![
            LineSegmentSpec {
                start: (-2.0, -7.0),
                end: (-2.0, 7.0),
                thickness: 2.0,
            },
            LineSegmentSpec {
                start: (2.0, -3.0),
                end: (12.0, -3.0),
                thickness: 1.0,
            },
        ],
        valves: vec![ValveSpec {
            wall: 0,
            t_start: 0.35,
            t_end: 0.65,
            is_closed: false,
        }],
        ..HeadlessConfig::default()
    }
}

/// The operator scenario's world: `config` plus an active thermostat left of
/// the valve wall and an emitting source sphere right of it. Thermostats and
/// source spheres have no headless flags, so they go through the same
/// operator commands as the UI tools.
fn operators_application(config: &HeadlessConfig) -> MainContent {
    let world_rect = scale_world_rect(test_world_rect(), config.world_scale.expect("scale"));
    let mut application = prepare_application(
        config,
        &SimConfig::default(),
        world_rect,
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");

    Box::new(PlaceThermostat {
        position: Vec2D::new(-8.0, 0.0),
    })
    .execute(&mut application);
    Box::new(PlaceSourceSphere {
        center: Vec2D::new(7.0, 3.0),
    })
    .execute(&mut application);
    let operators = application.create_snapshot().operators;
    let thermostat_id = operators.thermostats[0].id;
    let sphere_id = operators.source_spheres[0].id;
    Box::new(ToggleThermostat { thermostat_id }).execute(&mut application);
    Box::new(ToggleSourceSphere { sphere_id }).execute(&mut application);
    // Default period 3000 steps -> 20, default speed 0.05 -> 3.0. At 3.0 an
    // emitted ball moves 0.6 in one period, out of the way of the next one
    // (emission needs a free spot at the sphere's perimeter).
    Box::new(AdjustSourceSphereEmitPeriod {
        sphere_id,
        delta_steps: -2980,
    })
    .execute(&mut application);
    Box::new(AdjustSourceSphereSpeed {
        sphere_id,
        delta: 2.95,
    })
    .execute(&mut application);
    application
}

/// Bit-exact state of an operator-scenario run, for comparing runs.
struct OperatorsRunState {
    /// Balls before the first step.
    seeded_balls: usize,
    /// Kinetic and potential energy after every step.
    energies: Vec<[u32; 2]>,
    /// Momentum given to the walls (x, y) up to every step.
    wall_momentum: Vec<[u32; 2]>,
    /// Id, position and velocity of every ball after the last step, by id.
    balls: Vec<(usize, [u32; 4])>,
}

/// Runs the operator scenario, seeded with up to `seed_balls` balls, for
/// `steps` steps on a dedicated Rayon pool of `threads` workers.
fn operators_run_state(threads: usize, seed_balls: u32, steps: u32) -> OperatorsRunState {
    let run = || {
        let config = HeadlessConfig {
            seed_balls: Some(seed_balls),
            ..operators_config(steps, 0)
        };
        let mut application = operators_application(&config);
        let seeded_balls = application.create_snapshot().balls.len();
        let mut energies = Vec::with_capacity(steps as usize);
        let mut wall_momentum = Vec::with_capacity(steps as usize);
        for _ in 0..steps {
            application.simulation_step().expect("step");
            let snapshot = application.create_snapshot();
            energies.push([
                snapshot.kinetic_energy.to_bits(),
                snapshot.potential_energy.to_bits(),
            ]);
            let (px, py) = application.wall_momentum();
            wall_momentum.push([px.to_bits(), py.to_bits()]);
        }
        let mut balls: Vec<_> = application
            .create_snapshot()
            .balls
            .iter()
            .map(|ball| {
                (
                    ball.id,
                    [
                        ball.position.x.to_bits(),
                        ball.position.y.to_bits(),
                        ball.velocity.x.to_bits(),
                        ball.velocity.y.to_bits(),
                    ],
                )
            })
            .collect();
        balls.sort_unstable_by_key(|(id, _)| *id);
        OperatorsRunState {
            seeded_balls,
            energies,
            wall_momentum,
            balls,
        }
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("thread pool")
        .install(run)
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

fn maybe_update_fixture(fixture_path: &str, actual: &str) {
    if std::env::var("UPDATE_HEADLESS_FIXTURE").ok().as_deref() == Some("1") {
        fs::write(fixture_path, actual).expect("update seeded fixture");
    }
}

/// Fails unless `actual_text` equals the checked-in fixture line by line;
/// `UPDATE_HEADLESS_FIXTURE=1` rewrites the fixture first.
fn assert_matches_fixture(fixture_path: &str, actual_text: &str) {
    maybe_update_fixture(fixture_path, actual_text);
    let expected_text = fs::read_to_string(fixture_path).expect("read seeded fixture");

    let actual_records = parse_jsonl_records(actual_text);
    let expected_records = parse_jsonl_records(&expected_text);
    let actual_lines = non_empty_lines(actual_text);
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
    assert_matches_fixture(SEEDED_HEADLESS_FIXTURE_PATH, &seeded_headless_output());
}

/// Golden output of the operator scenario. The fixture's `run_start.config`
/// only records the walls and the valve; the thermostat and the source
/// sphere (positions, both switched on, emit period 20 steps, emission speed
/// 3.0) show up as `operator_counts` only. See `operators_config` and
/// `operators_application` for the whole scenario.
#[test]
#[ignore = "suite:headless-regression"]
fn test_seeded_headless_operators_fixture_matches_checked_in_jsonl() {
    let config = operators_config(200, 100);
    let application = operators_application(&config);
    let (actual_text, snapshots_emitted) = headless_output(config, application);
    assert_eq!(snapshots_emitted, 3);
    assert_matches_fixture(SEEDED_OPERATORS_FIXTURE_PATH, &actual_text);
}

#[test]
fn test_seeded_operators_run_does_not_depend_on_thread_count() {
    // Denser than the fixture run: more pairs interact, so a reduction whose
    // order depends on the thread count shows up in the f32 bits. Not much
    // denser: from about 80 seeded balls of the small default species (`N2`)
    // on, the gas keeps the source sphere's perimeter occupied and nothing
    // is emitted.
    const SEED_BALLS: u32 = 60;
    const STEPS: u32 = 300;
    let one = operators_run_state(1, SEED_BALLS, STEPS);
    let four = operators_run_state(4, SEED_BALLS, STEPS);

    // Scenario check: the source sphere emitted, so structural changes
    // (new balls, list rebuilds) are part of what is compared.
    assert!(
        one.balls.len() > one.seeded_balls,
        "scenario invalid: no ball was emitted ({} balls)",
        one.balls.len()
    );
    if let Some(step) = (0..one.energies.len()).find(|&i| one.energies[i] != four.energies[i]) {
        panic!(
            "1 thread vs 4 threads: kinetic/potential energy bits differ after step {}: {:08x?} vs {:08x?}",
            step + 1,
            one.energies[step],
            four.energies[step]
        );
    }
    assert_ne!(
        one.wall_momentum.last(),
        Some(&[0.0_f32.to_bits(); 2]),
        "scenario invalid: no ball hit a wall"
    );
    if let Some(step) =
        (0..one.wall_momentum.len()).find(|&i| one.wall_momentum[i] != four.wall_momentum[i])
    {
        panic!(
            "1 thread vs 4 threads: wall momentum bits differ after step {}: {:08x?} vs {:08x?}",
            step + 1,
            one.wall_momentum[step],
            four.wall_momentum[step]
        );
    }
    assert_eq!(
        one.balls.len(),
        four.balls.len(),
        "1 thread vs 4 threads: ball count differs"
    );
    if let Some((a, b)) = one.balls.iter().zip(&four.balls).find(|(a, b)| a != b) {
        panic!(
            "1 thread vs 4 threads diverged at ball {}: {:08x?} vs ball {}: {:08x?}",
            a.0, a.1, b.0, b.1
        );
    }
}

fn test_world_rect() -> moldyat_core::objects2d::Rect {
    moldyat_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        moldyat_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
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

    let mut application = prepare_application(
        &config,
        &SimConfig::default(),
        test_world_rect(),
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
    assert_eq!(application.time_step(), 0.005);

    let snapshot = application.create_snapshot();
    let walls = &snapshot.operators.line_segments;
    assert_eq!(walls.len(), 2);
    assert_eq!(walls[0].thickness, 4.0);
    assert_eq!(walls[1].thickness, 1.0);

    // Walls are placed before seeding, so no ball starts inside one.
    for ball in &snapshot.balls {
        for wall in walls {
            let clearance = wall.distance_to_ball(&ball.position, ball.contact_radius);
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
    let application = prepare_application(
        &config,
        &SimConfig::default(),
        test_world_rect(),
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
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
        let app = prepare_application(
            &config(seed_speed),
            &SimConfig::default(),
            test_world_rect(),
            &SaveStore::new("saves"),
            None,
        )
        .unwrap();
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

#[test]
fn test_prepare_application_places_valves_on_their_walls() {
    let config = HeadlessConfig {
        line_segments: vec![
            LineSegmentSpec {
                start: (-20.0, 0.0),
                end: (20.0, 0.0),
                thickness: 2.0,
            },
            LineSegmentSpec {
                start: (0.0, -20.0),
                end: (0.0, 20.0),
                thickness: 2.0,
            },
        ],
        valves: vec![
            ValveSpec {
                wall: 1,
                t_start: 0.25,
                t_end: 0.75,
                is_closed: true,
            },
            ValveSpec {
                wall: 0,
                t_start: 0.4,
                t_end: 0.6,
                is_closed: false,
            },
        ],
        ..HeadlessConfig::default()
    };
    let application = prepare_application(
        &config,
        &SimConfig::default(),
        test_world_rect(),
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
    let operators = application.create_snapshot().operators;
    assert_eq!(operators.walls.len(), 2);
    assert_eq!(operators.valve_spans.len(), 2);
    for span in &operators.valve_spans {
        let wall = operators.wall_of_valve(span.valve_id).expect("valve wall");
        if span.is_closed {
            // Wall 1 is vertical: the valve spans y in -10..10.
            assert_eq!(wall.start, Vec2D::new(0.0, -20.0));
            assert!((span.p0.y + 10.0).abs() < 1e-4 && (span.p1.y - 10.0).abs() < 1e-4);
        } else {
            assert_eq!(wall.start, Vec2D::new(-20.0, 0.0));
            assert!((span.p0.x + 4.0).abs() < 1e-4 && (span.p1.x - 4.0).abs() < 1e-4);
        }
    }
}

#[test]
fn test_run_writes_timings_for_steps_after_warmup() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let timings_path = std::env::temp_dir().join(format!(
        "headless-timings-{}-{nanos}.json",
        std::process::id()
    ));
    let config = HeadlessConfig {
        steps: 12,
        snapshot_every: 4,
        rng_seed: Some(3),
        seed_balls: Some(40),
        timings: Some(timings_path.clone()),
        timing_warmup: 2,
        ..HeadlessConfig::default()
    };
    let application = prepare_application(
        &config,
        &SimConfig::default(),
        test_world_rect(),
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
    HeadlessRunner::builder()
        .application(application)
        .sink(Box::new(RecordingSink(Rc::new(RefCell::new(Vec::new())))))
        .steps(config.steps)
        .snapshot_every(config.snapshot_every)
        .timings(config.timings.clone(), config.timing_warmup)
        .build()
        .run()
        .expect("run");
    let content = fs::read_to_string(&timings_path).expect("read timings");
    let _ = fs::remove_file(&timings_path);

    let value: serde_json::Value = serde_json::from_str(&content).expect("valid json");
    assert_eq!(value["schema_version"], "headless-timings.v1");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["warmup_steps"], 2);
    assert_eq!(value["steps"], 10);
    // Snapshots after steps 4, 8 and 12; all after the warmup.
    assert_eq!(value["snapshots"], 3);
    assert_eq!(value["balls"], 40);
    let per_step = &value["ms_per_step"];
    let total = per_step["total"].as_f64().expect("total");
    let mut sum = 0.0;
    for phase in [
        "neighbours",
        "positions",
        "forces",
        "apply",
        "cells",
        "thermo",
        "snapshot",
        "output",
        "other",
    ] {
        let ms = per_step[phase].as_f64().expect(phase);
        assert!(ms >= 0.0, "{phase} {ms}");
        sum += ms;
    }
    assert!(total > 0.0);
    assert!(
        (sum - total).abs() <= 1e-6 * total.max(1.0) + 1e-9,
        "{sum} vs {total}"
    );
}

#[test]
fn test_write_timings_rejects_runs_with_paused_steps() {
    use moldyat_core::application::phase_timings::PhaseTimings;

    let application =
        build_application(test_world_rect(), &SimConfig::default(), Some(1)).expect("build");
    let snapshot = application.create_snapshot();
    let mut clock = RunClock::start(0);
    clock.steps = 5;
    let phases = PhaseTimings {
        steps: 1,
        ..PhaseTimings::default()
    };
    let path = std::env::temp_dir().join(format!(
        "headless-timings-paused-{}.json",
        std::process::id()
    ));
    let result = write_timings(&path, &clock, Duration::from_millis(5), &phases, &snapshot);
    assert!(
        result.is_err(),
        "a run with paused steps must not be reported"
    );
    assert!(!path.exists());
}

#[test]
fn test_run_with_timings_fails_at_the_first_paused_step() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("headless-paused-{}-{nanos}", std::process::id()));
    let store = SaveStore::new(&dir);
    let mut seeded =
        build_application(test_world_rect(), &SimConfig::default(), Some(5)).expect("build");
    seeded.seed_random_balls(20);
    let saved = seeded.save_world(&store).expect("save");
    let mut paused =
        build_application(test_world_rect(), &SimConfig::default(), Some(5)).expect("build");
    paused
        .load_world(&store, &LoadSource::Path(saved), true)
        .expect("load paused");

    let timings_path = dir.join("timings.json");
    let result = HeadlessRunner::builder()
        .application(paused)
        .sink(Box::new(RecordingSink(Rc::new(RefCell::new(Vec::new())))))
        .steps(50)
        .timings(Some(timings_path.clone()), 0)
        .build()
        .run();
    let written = timings_path.exists();
    let _ = fs::remove_dir_all(&dir);

    let error = result.err().expect("a paused run must fail");
    assert!(error.to_string().contains("step 0"), "{error}");
    assert!(!written);
}

#[test]
fn test_prepare_application_runs_with_the_sim_config() {
    let mut sim_config = SimConfig::default();
    sim_config.integration.time_step = 0.02;
    sim_config.potential.cutoff_factor = 3.0;
    let config = HeadlessConfig::default();
    let application = prepare_application(
        &config,
        &sim_config,
        test_world_rect(),
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
    assert_eq!(application.config(), sim_config);
}

#[test]
fn test_dt_flag_overrides_the_sim_config() {
    let mut sim_config = SimConfig::default();
    sim_config.integration.time_step = 0.02;
    let config = HeadlessConfig {
        dt: Some(0.005),
        ..HeadlessConfig::default()
    };
    let application = prepare_application(
        &config,
        &sim_config,
        test_world_rect(),
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
    assert_eq!(application.time_step(), 0.005);
    assert_eq!(application.config().integration.time_step, 0.005);
}
