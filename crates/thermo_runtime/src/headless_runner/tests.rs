// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::headless_cli::{DetailLevel, LineSegmentSpec, ValveSpec};
use crate::headless_output::HeadlessSnapshotSink;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The default world (`DEFAULT_WORLD_RECT`, `world.scale = 1`), pinned here
/// so a change to it shows up in these tests.
fn window_world_rect() -> thermo_physics::objects2d::Rect {
    let rect = thermo_physics::objects2d::Rect::new(-56.0, 31.0, 112.0, -62.0);
    assert_eq!(rect, SimConfig::default().world.rect());
    rect
}

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
        self.0.borrow_mut().push(snapshot.frame.simulation_step);
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
    let world_rect = window_world_rect();
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

    let report = HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
        .expect("runner")
        .run()
        .expect("run");
    let content = fs::read_to_string(&output_path).expect("read regression output");
    (content, report.snapshots_emitted)
}

/// The checked-in inputs of the fixture runs (#340).
const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/headless_runner/fixtures");

/// The run that `fixtures/<name>.args` describes: the command line of
/// `moldyat` without the program name, run from the fixtures directory
/// (`--config` and `--script` name files there). Returns the parsed headless
/// config and the `--config` run settings (defaults without one).
fn fixture_run(name: &str) -> (HeadlessConfig, SimConfig) {
    let dir = std::path::Path::new(FIXTURES_DIR);
    let text = fs::read_to_string(dir.join(format!("{name}.args"))).expect("read fixture args");
    let mut args = vec!["moldyat".to_string()];
    let mut sim = SimConfig::default();
    let mut words = text.split_whitespace();
    while let Some(word) = words.next() {
        match word {
            // A flag of the binary, not of `parse_headless_args`.
            "--config" => {
                let file = words.next().expect("--config value");
                sim = SimConfig::load(&dir.join(file)).expect("fixture config");
            }
            "--script" => {
                let file = words.next().expect("--script value");
                args.push(word.to_string());
                args.push(dir.join(file).display().to_string());
            }
            _ => args.push(word.to_string()),
        }
    }
    let config = crate::headless_cli::parse_headless_args(&args)
        .expect("fixture args parse")
        .expect("fixture args are a headless run");
    (config, sim)
}

/// The application of a fixture run, as the binary prepares it.
fn fixture_application(config: &HeadlessConfig, sim: &SimConfig) -> MainContent {
    prepare_application(config, sim, &SaveStore::new("saves"), None).expect("prepare")
}

fn seeded_headless_output() -> String {
    let (config, sim) = fixture_run("seeded_headless_regression");
    assert_eq!(sim.world.rect(), window_world_rect());
    let application = fixture_application(&config, &sim);
    let (content, snapshots_emitted) = headless_output(config, application);
    assert_eq!(snapshots_emitted, 4);
    content
}

/// The operator scenario (`fixtures/seeded_headless_operators.*`): two walls
/// and an open valve from the command line, an active thermostat and an
/// emitting source sphere from the script, in a quarter-size world from the
/// config, so that balls reach the operators within the run.
fn operators_run() -> (HeadlessConfig, SimConfig) {
    fixture_run("seeded_headless_operators")
}

/// The operator scenario's world after its script's setup commands (those
/// before the first `step`), without running any step.
fn operators_application(config: &HeadlessConfig, sim: &SimConfig) -> MainContent {
    let mut application = fixture_application(config, sim);
    let script = config.script.as_ref().expect("operator scenario script");
    ScriptRun::new(script)
        .run_due(0, &mut application, None)
        .expect("setup commands");
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
        let (config, sim) = operators_run();
        let config = HeadlessConfig {
            seed_balls: Some(seed_balls),
            ..config
        };
        let mut application = operators_application(&config, &sim);
        let seeded_balls = application.create_snapshot().balls.len();
        let mut energies = Vec::with_capacity(steps as usize);
        let mut wall_momentum = Vec::with_capacity(steps as usize);
        for _ in 0..steps {
            application.simulation_step().expect("step");
            let snapshot = application.create_snapshot();
            energies.push([
                snapshot.frame.kinetic_energy.to_bits(),
                snapshot.frame.potential_energy.to_bits(),
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

/// Golden output of the operator scenario (`fixtures/seeded_headless_operators.*`).
/// The script's setup commands appear as `command` records after
/// `run_start`, whose `config.script` lists the whole script.
#[test]
#[ignore = "suite:headless-regression"]
fn test_seeded_headless_operators_fixture_matches_checked_in_jsonl() {
    let (config, sim) = operators_run();
    assert_eq!(config.steps, 200);
    let application = fixture_application(&config, &sim);
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

fn test_world_rect() -> thermo_physics::objects2d::Rect {
    window_world_rect()
}

#[test]
fn test_prepare_application_applies_dt_and_line_segments() {
    let config = HeadlessConfig {
        steps: 4,
        rng_seed: Some(12345),
        seed_balls: Some(24),
        dt: Some(0.0025),
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
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
    assert_eq!(application.time_step(), 0.0025);

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
    let time = application.create_snapshot().frame.time;
    assert!((time - 4.0 * 0.0025).abs() < 1e-6, "time {time}");
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
        line_segments: vec![LineSegmentSpec {
            start: (-10.0, 5.0),
            end: (10.0, 5.0),
            thickness: 2.0,
        }],
        ..HeadlessConfig::default()
    };
    let mut sim = SimConfig::default();
    sim.world.scale = 1.5;
    let application =
        prepare_application(&config, &sim, &SaveStore::new("saves"), None).expect("prepare");
    let default_dt = application.time_step();
    HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
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
    assert_eq!(
        run_start["config"]["sim"]["world"]["scale"].as_f64(),
        Some(1.5)
    );
    let segment = &run_start["config"]["line_segments"][0];
    assert_eq!(segment["start"], serde_json::json!([-10.0, 5.0]));
    assert_eq!(segment["end"], serde_json::json!([10.0, 5.0]));
    assert_eq!(segment["thickness"], 2.0);
    assert_eq!(run_start["initial"]["operator_counts"]["line_segments"], 1);
    let world = &run_start["initial"]["world_rect"];
    assert_eq!(world["left"].as_f64(), Some(-84.0));
    assert_eq!(world["top"].as_f64(), Some(46.5));
    assert!(world["left"].as_f64() < world["right"].as_f64());
    assert!(world["bottom"].as_f64() < world["top"].as_f64());
}

/// A default world records `world_scale: null` and no `sim.world` section,
/// as before the key moved into `SimConfig` (#351).
#[test]
fn test_run_start_of_the_default_world_has_no_world_scale() {
    let output_path = std::env::temp_dir().join(format!(
        "headless-default-world-{}.jsonl",
        std::process::id()
    ));
    let config = HeadlessConfig {
        output: Some(output_path.clone()),
        stdout: false,
        steps: 1,
        ..HeadlessConfig::default()
    };
    let application = prepare_application(
        &config,
        &SimConfig::default(),
        &SaveStore::new("saves"),
        None,
    )
    .expect("prepare");
    assert_eq!(application.world().world_region(), window_world_rect());
    HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
        .expect("runner")
        .run()
        .expect("run");
    let content = fs::read_to_string(&output_path).expect("read output");
    let _ = fs::remove_file(&output_path);

    let run_start = &parse_jsonl_records(&content)[0];
    assert!(run_start["config"]["world_scale"].is_null());
    assert!(run_start["config"]["sim"].get("world").is_none());
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
        .commit("abc1234")
        .build()
        .run()
        .expect("run");
    let content = fs::read_to_string(&timings_path).expect("read timings");
    let _ = fs::remove_file(&timings_path);

    let value: serde_json::Value = serde_json::from_str(&content).expect("valid json");
    assert_eq!(value["schema_version"], "headless-timings.v1");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["commit"], "abc1234", "the commit the binary set");
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
    use thermo_physics::application::phase_timings::PhaseTimings;

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
    let result = write_timings(
        &path,
        "abc1234",
        &clock,
        Duration::from_millis(5),
        &phases,
        &snapshot,
    );
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
    let application =
        prepare_application(&config, &sim_config, &SaveStore::new("saves"), None).expect("prepare");
    assert_eq!(application.config(), sim_config);
}

#[test]
fn test_dt_flag_overrides_the_sim_config() {
    let mut sim_config = SimConfig::default();
    sim_config.integration.time_step = 0.02;
    let config = HeadlessConfig {
        dt: Some(0.0025),
        ..HeadlessConfig::default()
    };
    let application =
        prepare_application(&config, &sim_config, &SaveStore::new("saves"), None).expect("prepare");
    assert_eq!(application.time_step(), 0.0025);
    assert_eq!(application.config().integration.time_step, 0.0025);
}

/// The species of a `[[species]]` config, N2 + Ar.
fn n2_ar_sim_config() -> SimConfig {
    SimConfig::from_toml(
        "[[species]]\n\
         name = \"N2\"\n\
         sigma_angstrom = 3.698\n\
         epsilon_kelvin = 95.05\n\
         mass_u = 28.0134\n\
         colour = [0.0, 1.0, 1.0]\n\
         [[species]]\n\
         name = \"Ar\"\n\
         sigma_angstrom = 3.405\n\
         epsilon_kelvin = 119.8\n\
         mass_u = 39.948\n\
         colour = [0.6, 0.45, 1.0]\n",
    )
    .expect("valid config")
}

fn mixture_run() -> String {
    let config = HeadlessConfig {
        steps: 60,
        snapshot_every: 30,
        detail: DetailLevel::Balls,
        rng_seed: Some(11),
        seed_balls: Some(60),
        seed_speed: Some(0.5),
        seed_species: vec![("N2".to_string(), 1), ("Ar".to_string(), 2)],
        ..HeadlessConfig::default()
    };
    let application =
        prepare_application(&config, &n2_ar_sim_config(), &SaveStore::new("saves"), None)
            .expect("prepare");
    headless_output(config, application).0
}

/// An N2 + Ar mixture from a config's species list runs, keeps its 1:2
/// split, names both species in the per-ball records and is reproducible.
#[test]
fn test_mixture_from_the_config_species_runs_reproducibly() {
    let output = mixture_run();
    assert_eq!(output, mixture_run(), "same seed, same bytes");
    let records = parse_jsonl_records(&output);
    let run_start = &records[0];
    assert_eq!(
        run_start["config"]["seed_species"],
        serde_json::json!([
            {"species": "N2", "weight": 1},
            {"species": "Ar", "weight": 2},
        ])
    );
    let frames: Vec<_> = records
        .iter()
        .filter(|r| r["record_kind"] == "frame")
        .collect();
    assert_eq!(frames.len(), 3);
    for frame in frames {
        let balls = frame["balls"].as_array().expect("balls detail");
        let count = |name: &str| balls.iter().filter(|b| b["species"] == name).count();
        assert_eq!(
            (count("N2"), count("Ar")),
            (20, 40),
            "step {}",
            frame["step"]
        );
    }
}

#[test]
fn test_seed_species_must_be_in_the_runs_table() {
    let config = HeadlessConfig {
        seed_balls: Some(5),
        seed_species: vec![("XS".to_string(), 1)],
        ..HeadlessConfig::default()
    };
    let error = prepare_application(&config, &n2_ar_sim_config(), &SaveStore::new("saves"), None)
        .err()
        .expect("XS is not in the config's species");
    assert!(error.to_string().contains("unknown species"), "{error}");
}

/// Runs `config` with `sim_config` and returns its `run_start` record.
fn run_start_record(
    config: HeadlessConfig,
    sim_config: &SimConfig,
    name: &str,
) -> serde_json::Value {
    let output_path = std::env::temp_dir().join(format!(
        "moldyat_run_start_{name}_{}.jsonl",
        std::process::id()
    ));
    let config = HeadlessConfig {
        output: Some(output_path.clone()),
        stdout: false,
        ..config
    };
    let application =
        prepare_application(&config, sim_config, &SaveStore::new("saves"), None).expect("prepare");
    HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
        .expect("runner")
        .run()
        .expect("run");
    let content = fs::read_to_string(&output_path).expect("read output");
    let _ = fs::remove_file(&output_path);
    parse_jsonl_records(&content).remove(0)
}

#[test]
fn test_run_start_records_the_default_sim_config() {
    let run_start = run_start_record(HeadlessConfig::default(), &SimConfig::default(), "default");
    let sim = &run_start["config"]["sim"];
    assert_eq!(*sim, serde_json::to_value(SimConfig::default()).unwrap());
    for section in ["integration", "potential", "neighbours", "display"] {
        assert!(sim[section].is_object(), "{section}: {sim}");
    }
    assert!(sim.get("species").is_none(), "built-in library: {sim}");
}

#[test]
fn test_run_start_records_the_effective_sim_config() {
    let sim_config = SimConfig::from_toml(
        "[integration]\n\
         speed_limit_factor = 0.25\n\
         [potential]\n\
         max_force = 80.0\n\
         [neighbours]\n\
         skin_factor = 0.3\n\
         [[species]]\n\
         name = \"Ar\"\n\
         sigma_angstrom = 3.405\n\
         epsilon_kelvin = 119.8\n\
         mass_u = 39.948\n\
         colour = [0.6, 0.45, 1.0]\n",
    )
    .unwrap();
    // `--dt` beats the config's time step, and the record shows the result.
    let config = HeadlessConfig {
        dt: Some(0.002),
        ..HeadlessConfig::default()
    };
    let run_start = run_start_record(config, &sim_config, "custom");
    let sim = &run_start["config"]["sim"];
    let expected = SimConfig {
        integration: thermo_physics::application::sim_config::IntegrationConfig {
            time_step: 0.002,
            ..sim_config.integration.clone()
        },
        ..sim_config.clone()
    };
    assert_eq!(*sim, serde_json::to_value(&expected).unwrap());
    assert_eq!(
        sim["integration"]["time_step"].as_f64(),
        Some(0.002_f32 as f64)
    );
    assert_eq!(sim["potential"]["max_force"], 80.0);
    assert_eq!(sim["species"][0]["name"], "Ar");
    // The record reads back into the same config.
    let read_back: SimConfig = serde_json::from_value(sim.clone()).unwrap();
    assert_eq!(read_back, expected);
}

#[test]
fn test_run_start_records_a_loaded_saves_sim_config_with_dt_winning() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("headless-load-sim-{}-{nanos}", std::process::id()));
    let store = SaveStore::new(&dir);
    let saved_config = SimConfig::from_toml(
        "[neighbours]\nskin_factor = 0.3\n\
         [[species]]\nname = \"Ar\"\nsigma_angstrom = 3.405\nepsilon_kelvin = 119.8\n\
         mass_u = 39.948\ncolour = [0.6, 0.45, 1.0]\n",
    )
    .unwrap();
    let saved = build_application(test_world_rect(), &saved_config, Some(5))
        .expect("build")
        .save_world(&store)
        .expect("save");

    // The run itself uses the default config; the save brings its own, and
    // `--dt` beats the save's time step.
    let config = HeadlessConfig {
        output: Some(dir.join("run.jsonl")),
        stdout: false,
        dt: Some(0.002),
        ..HeadlessConfig::default()
    };
    let application =
        prepare_application(&config, &SimConfig::default(), &store, Some(saved)).expect("prepare");
    HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
        .expect("runner")
        .run()
        .expect("run");
    let content = fs::read_to_string(dir.join("run.jsonl")).expect("read output");
    let run_start = parse_jsonl_records(&content).remove(0);

    let mut expected = saved_config.clone();
    expected.integration.time_step = 0.002;
    let sim = &run_start["config"]["sim"];
    assert_eq!(*sim, serde_json::to_value(&expected).unwrap());

    // The record is a valid `--config` as a `.json` file.
    let reused = dir.join("run.json");
    fs::write(&reused, sim.to_string()).unwrap();
    assert_eq!(SimConfig::load(&reused).unwrap(), expected);
    let _ = fs::remove_dir_all(&dir);
}

fn export_scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "moldyat_headless_export_{name}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn export_config(dir: &std::path::Path, format: crate::export::ExportFormat) -> HeadlessConfig {
    HeadlessConfig {
        steps: 6,
        snapshot_every: 2,
        rng_seed: Some(12345),
        seed_balls: Some(24),
        export_dir: Some(dir.to_path_buf()),
        export_format: format,
        ..HeadlessConfig::default()
    }
}

fn seeded_application(config: &HeadlessConfig) -> MainContent {
    let mut application =
        build_application(window_world_rect(), &SimConfig::default(), config.rng_seed)
            .expect("build deterministic app");
    application.seed_random_balls(config.seed_balls.expect("seed ball count") as usize);
    application
}

#[test]
fn test_export_dir_receives_the_series_and_the_final_snapshot() {
    let dir = export_scratch_dir("csv");
    let config = export_config(&dir, crate::export::ExportFormat::Csv);
    let application = seeded_application(&config);
    headless_output(config, application);

    let series = fs::read_to_string(dir.join("series.csv")).expect("series.csv");
    let lines: Vec<&str> = series.lines().collect();
    for column in [
        "E_kin [eps*]",
        "E_pot [eps*]",
        "E_tot [eps*]",
        "wall_momentum [m*sigma*/tau*]",
        "N2:count",
    ] {
        assert!(
            lines[0].split(',').any(|c| c == column),
            "{column} in {}",
            lines[0]
        );
    }
    let steps: Vec<&str> = lines[1..]
        .iter()
        .map(|l| l.split(',').next().unwrap())
        .collect();
    assert_eq!(steps, ["0", "2", "4", "6"]);

    let snapshot = fs::read_to_string(dir.join("snapshot.csv")).expect("snapshot.csv");
    assert_eq!(snapshot.lines().count(), 24 + 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_distributions_add_a_frame_field_and_an_export_file() {
    let dir = export_scratch_dir("distributions");
    let mut config = export_config(&dir, crate::export::ExportFormat::Jsonl);
    config.distributions = true;
    let mut application =
        build_application(window_world_rect(), &SimConfig::default(), config.rng_seed)
            .expect("build deterministic app");
    application.seed_random_balls_with_speed(24, 2.0);
    let (output, _) = headless_output(config, application);

    let records: Vec<serde_json::Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records[0]["config"]["distributions"], true);
    let frames: Vec<_> = records
        .iter()
        .filter(|r| r["record_kind"] == "frame")
        .collect();
    assert!(!frames.is_empty());
    for frame in frames {
        let speed = &frame["distributions"]["speed"];
        let counts = speed["counts"].as_array().expect("speed counts");
        assert_eq!(counts.len(), crate::distributions::SPEED_BINS);
        let total: u64 = counts.iter().map(|c| c.as_u64().unwrap()).sum();
        assert_eq!(total, 24, "every ball is in the histogram");
        let pairs = frame["distributions"]["rdf"]["pairs"].as_array().unwrap();
        assert_eq!(pairs[0]["species"], serde_json::json!(["N2", "N2"]));
    }

    let export = fs::read_to_string(dir.join("distributions.jsonl")).expect("export file");
    let kinds: Vec<String> = export
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["record_kind"].to_string()
        })
        .collect();
    assert_eq!(
        kinds,
        ["\"distributions_header\"", "\"speed_histogram\"", "\"rdf\""]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_without_distributions_frames_and_exports_stay_as_they_were() {
    let dir = export_scratch_dir("no_distributions");
    let config = export_config(&dir, crate::export::ExportFormat::Csv);
    let application = seeded_application(&config);
    let (output, _) = headless_output(config, application);
    assert!(!output.contains("distributions"));
    assert!(!dir.join("distributions.csv").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_export_is_deterministic_and_jsonl_series_selection_works() {
    let run = |name: &str| {
        let dir = export_scratch_dir(name);
        let mut config = export_config(&dir, crate::export::ExportFormat::Jsonl);
        config.export_series = Some(vec!["E_kin".to_string()]);
        let application = seeded_application(&config);
        headless_output(config, application);
        let series = fs::read_to_string(dir.join("series.jsonl")).expect("series.jsonl");
        let snapshot = fs::read_to_string(dir.join("snapshot.jsonl")).expect("snapshot.jsonl");
        fs::remove_dir_all(dir).unwrap();
        (series, snapshot)
    };
    let (series_a, snapshot_a) = run("det_a");
    let (series_b, snapshot_b) = run("det_b");
    assert_eq!(series_a, series_b);
    assert_eq!(snapshot_a, snapshot_b);

    let header: serde_json::Value = serde_json::from_str(series_a.lines().next().unwrap()).unwrap();
    assert_eq!(header["series"].as_array().unwrap().len(), 1);
    assert_eq!(header["rows"], 4);
}

#[test]
#[should_panic(expected = "unknown series")]
fn test_export_of_an_unknown_series_fails_the_run() {
    let dir = export_scratch_dir("unknown");
    let mut config = export_config(&dir, crate::export::ExportFormat::Csv);
    config.export_series = Some(vec!["nope".to_string()]);
    let application = seeded_application(&config);
    headless_output(config, application);
}

#[test]
fn test_export_of_an_unknown_series_fails_before_the_first_step() {
    let dir = export_scratch_dir("early");
    let mut config = export_config(&dir, crate::export::ExportFormat::Csv);
    config.export_series = Some(vec!["E_kni".to_string()]);
    let (output, _guard) = run_output_path("early");
    config.output = Some(output.clone());
    config.stdout = false;
    let application = seeded_application(&config);
    let error = HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
        .expect("runner")
        .run()
        .err()
        .expect("the unknown series fails the run");
    assert!(
        error.to_string().contains("unknown series E_kni"),
        "{error}"
    );
    // Nothing was written: not even `run_start`.
    assert_eq!(fs::read_to_string(&output).unwrap_or_default(), "");
    assert!(!dir.exists());
}

#[test]
fn test_failed_export_still_writes_run_end_and_timings() {
    let dir = export_scratch_dir("failing");
    // A regular file where the export directory should be.
    fs::create_dir_all(dir.parent().unwrap()).unwrap();
    let blocker = dir.with_extension("file");
    fs::write(&blocker, "x").unwrap();
    let mut config = export_config(&blocker, crate::export::ExportFormat::Csv);
    let (output, _guard) = run_output_path("failing");
    let timings = output.with_extension("timings.json");
    config.output = Some(output.clone());
    config.stdout = false;
    config.timings = Some(timings.clone());
    let application = seeded_application(&config);
    let result = HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
        .expect("runner")
        .run();
    assert!(result.is_err(), "the export fails");
    let lines = fs::read_to_string(&output).unwrap();
    assert!(
        lines
            .lines()
            .last()
            .unwrap()
            .contains("\"record_kind\":\"run_end\"")
    );
    assert!(timings.exists(), "the timings report was written");
    let _ = fs::remove_file(blocker);
    let _ = fs::remove_file(timings);
}

#[test]
fn test_export_covers_the_whole_run_by_sampling_every_kth_frame() {
    let dir = export_scratch_dir("thinned");
    // 3000 frames do not fit the 1000 samples of the registry.
    let mut config = export_config(&dir, crate::export::ExportFormat::Csv);
    config.steps = 3000;
    config.snapshot_every = 1;
    config.seed_balls = Some(3);
    let application = seeded_application(&config);
    headless_output(config, application);

    let series = fs::read_to_string(dir.join("series.csv")).unwrap();
    let mut lines = series.lines();
    // Frames 0, 4, 8, … and the last: every 4th frame (ceil(3001 / 999)).
    assert_eq!(lines.next(), Some("# sample_interval_steps=4"));
    let steps: Vec<u64> = lines
        .skip(1)
        .map(|l| l.split(',').next().unwrap().parse().unwrap())
        .collect();
    assert!(steps.len() <= 1000, "{}", steps.len());
    assert_eq!(steps[0], 0, "the first step is exported");
    assert_eq!(*steps.last().unwrap(), 3000, "the last step is exported");
    assert!(steps.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(steps[1], 4);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_export_samples_every_frame_when_the_run_fits() {
    let dir = export_scratch_dir("fits");
    let config = export_config(&dir, crate::export::ExportFormat::Jsonl);
    let application = seeded_application(&config);
    headless_output(config, application);
    let series = fs::read_to_string(dir.join("series.jsonl")).unwrap();
    let header: serde_json::Value = serde_json::from_str(series.lines().next().unwrap()).unwrap();
    assert!(header["sample_interval_steps"].is_null());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_export_stream_writes_every_frame_and_leaves_the_frames_alone() {
    let dir = export_scratch_dir("stream");
    // 3000 frames: the series export is thinned, the stream is not.
    let mut config = export_config(&dir, crate::export::ExportFormat::Csv);
    config.steps = 3000;
    config.snapshot_every = 1;
    config.seed_balls = Some(3);
    let (plain, _) = headless_output(config.clone(), seeded_application(&config));
    config.export_stream = true;
    let (streamed, _) = headless_output(config.clone(), seeded_application(&config));
    assert_eq!(plain, streamed, "the stream does not change the records");

    let stream = fs::read_to_string(dir.join("stream.csv")).expect("stream.csv");
    let mut lines = stream.lines();
    assert!(lines.next().unwrap().starts_with("step,time,"));
    let steps: Vec<u64> = lines
        .map(|l| l.split(',').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(steps.len(), 3001, "one row per frame");
    assert_eq!(steps[0], 0);
    assert_eq!(*steps.last().unwrap(), 3000);
    assert!(steps.windows(2).all(|w| w[1] == w[0] + 1));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_failed_export_stream_lets_the_run_finish_and_fails_it_at_the_end() {
    let dir = export_scratch_dir("stream_failing");
    // A directory where the stream file should go: its header cannot be
    // renamed into place, while series.csv and snapshot.csv can be written.
    fs::create_dir_all(dir.join("stream.csv")).unwrap();
    let mut config = export_config(&dir, crate::export::ExportFormat::Csv);
    config.export_stream = true;
    let (output, _guard) = run_output_path("stream_failing");
    config.output = Some(output.clone());
    config.stdout = false;
    let application = seeded_application(&config);
    let error = HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)
        .expect("runner")
        .run()
        .err()
        .expect("the stream fails the run");
    assert!(error.to_string().contains("--export-stream"), "{error}");
    let lines = fs::read_to_string(&output).unwrap();
    assert_eq!(
        lines.matches("\"record_kind\":\"frame\"").count(),
        4,
        "every frame ran"
    );
    assert!(lines.lines().last().unwrap().contains("\"run_end\""));
    assert!(dir.join("series.csv").exists() && dir.join("snapshot.csv").exists());
    fs::remove_dir_all(dir).unwrap();
}

/// A temporary JSONL output path for a run, removed when the guard drops.
fn run_output_path(name: &str) -> (std::path::PathBuf, impl Drop) {
    struct Guard(std::path::PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "moldyat_headless_run_{name}_{}.jsonl",
        std::process::id()
    ));
    let _ = fs::remove_file(&path);
    (path.clone(), Guard(path))
}

/// Runs `script` on a seeded default world with a frame every `every`
/// steps; returns the JSONL records.
fn scripted_run(script: &str, every: u32) -> io::Result<Vec<serde_json::Value>> {
    let script = Script::parse(script).expect("script parses");
    let config = HeadlessConfig {
        steps: script.total_steps(),
        snapshot_every: every,
        rng_seed: Some(7),
        seed_balls: Some(10),
        script: Some(script),
        ..HeadlessConfig::default()
    };
    let application = prepare_application(
        &config,
        &SimConfig::default(),
        &SaveStore::new("saves"),
        None,
    )?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "headless-script-{}-{nanos}.jsonl",
        std::process::id()
    ));
    let config = HeadlessConfig {
        output: Some(path.clone()),
        stdout: false,
        ..config
    };
    let result = HeadlessRunner::from_config(config, application, UNKNOWN_COMMIT)?.run();
    let text = fs::read_to_string(&path).unwrap_or_default();
    let _ = fs::remove_file(&path);
    result.map(|_| parse_jsonl_records(&text))
}

fn kinds(records: &[serde_json::Value]) -> Vec<String> {
    records
        .iter()
        .map(|record| {
            let kind = record["record_kind"].as_str().unwrap_or_default();
            match kind {
                "frame" => format!("frame@{}", record["step"]),
                "command" => format!(
                    "{}@{}",
                    record["id"].as_str().unwrap_or_default(),
                    record["step"]
                ),
                _ => kind.to_string(),
            }
        })
        .collect()
}

#[test]
fn test_script_commands_run_at_their_steps_after_the_frame() {
    let records = scripted_run(
        "sim.cycle_gravity\nstep 2\nballs.add_random 5\nstep 2\nballs.clear\n",
        2,
    )
    .expect("run");
    assert_eq!(
        kinds(&records),
        [
            "run_start",
            "sim.cycle_gravity@0",
            "frame@0",
            "frame@2",
            "balls.add_random@2",
            "frame@4",
            "balls.clear@4",
            "run_end"
        ]
    );
    // The setup command is in effect before the first frame.
    assert_ne!(records[0]["initial"]["gravity_force_multiple"], 0);
    assert_eq!(records[0]["config"]["steps"], 4);
    assert_eq!(records[0]["config"]["script"][2], "balls.add_random 5");
    let balls = |index: usize| records[index]["ball_count"].as_u64().expect("ball count");
    assert_eq!(
        balls(3),
        10,
        "frame@2 shows the world before its step's commands"
    );
    assert!(balls(5) > 10, "balls.add_random added balls before step 4");
    let command = &records[4];
    assert_eq!(command["schema_version"], "headless.v2");
    assert_eq!(command["line"], 3);
    assert_eq!(command["args"], serde_json::json!(["5"]));
}

#[test]
fn test_script_of_steps_only_writes_no_command_records() {
    let records = scripted_run("step 2", 0).expect("run");
    assert_eq!(
        kinds(&records),
        ["run_start", "frame@0", "frame@2", "run_end"]
    );
}

#[test]
fn test_script_command_that_cannot_run_fails_the_run_with_its_line() {
    let error = scripted_run("step 1\n\noperator.toggle_thermostat 0\nstep 1", 0).unwrap_err();
    assert_eq!(
        error.to_string(),
        "script line 3: there is no thermostat 0 (0 placed)"
    );
    let error = scripted_run("world.save", 0).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("script line 1: saving failed"),
        "{error}"
    );
}

#[test]
fn test_paused_world_fails_the_run_without_timings() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("headless-stall-{}-{nanos}", std::process::id()));
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
    let _ = fs::remove_dir_all(&dir);

    let error = HeadlessRunner::builder()
        .application(paused)
        .sink(Box::new(RecordingSink(Rc::new(RefCell::new(Vec::new())))))
        .steps(5)
        .build()
        .run()
        .err()
        .expect("a paused run must fail");
    assert_eq!(error.to_string(), "step 0 did not run (simulation paused)");
}

#[test]
fn test_builder_script_sets_the_run_length() {
    let script = Script::parse("step 3\nballs.clear\nstep 2").expect("script");
    let frames = Rc::new(RefCell::new(Vec::new()));
    let application =
        build_application(window_world_rect(), &SimConfig::default(), None).expect("application");
    let runner = HeadlessRunner::builder()
        .application(application)
        .sink(Box::new(RecordingSink(frames.clone())))
        .steps(1)
        .script(Some(script))
        .build();
    assert_eq!(runner.steps, 5);
    runner.run().expect("run");
    assert_eq!(*frames.borrow(), [0, 5]);
}
