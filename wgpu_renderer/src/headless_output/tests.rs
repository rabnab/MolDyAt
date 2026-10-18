// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use moldyat_core::application::renderer_snapshot::RendererSnapshot;
use moldyat_core::objects2d::ball_impl::SpeciesId;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_frame_record_contains_v1_schema_and_summary_fields() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 42;
    snapshot.time = 1.25;
    snapshot.kinetic_energy = 3.5;
    snapshot.potential_energy = -1.0;
    snapshot.gravity_force_multiple = 2;
    snapshot.physics_fps = 144.0;

    let line = frame_to_jsonl(&snapshot, Some(2.0));
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert_eq!(value["schema_version"], "headless.v2");
    assert_eq!(value["record_kind"], "frame");
    assert_eq!(value["step"], 42);
    assert_eq!(value["ball_count"], 0);
    assert_eq!(value["delta_energy"], 0.5);
    assert!(value.get("status_text").is_none());
}

#[test]
fn test_run_start_record_contains_config_and_operator_counts() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 0;
    snapshot.time = 0.0;

    let config = HeadlessConfig {
        steps: 1000,
        snapshot_every: 100,
        output: None,
        format: OutputFormat::Jsonl,
        detail: DetailLevel::Summary,
        rng_seed: Some(123),
        stdout: true,
        seed_balls: Some(50),
        dt: None,
        world_scale: None,
        seed_speed: None,
        line_segments: Vec::new(),
        valves: Vec::new(),
        timings: None,
        timing_warmup: 0,
        save_setup: false,
    };

    let line = run_start_to_jsonl(&config, &snapshot);
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert_eq!(value["schema_version"], "headless.v2");
    assert_eq!(value["record_kind"], "run_start");
    assert_eq!(value["config"]["steps"], 1000);
    assert_eq!(value["config"]["snapshot_every"], 100);
    assert_eq!(value["config"]["seed_balls"], 50);
    assert_eq!(value["config"]["seeded_constructor"], true);
    assert_eq!(value["initial"]["operator_counts"]["line_segments"], 0);
}

#[test]
fn test_run_end_record_contains_v1_schema_and_summary_fields() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 10;
    snapshot.time = 0.5;

    let line = run_end_to_jsonl(&snapshot, 7);
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert_eq!(value["schema_version"], "headless.v2");
    assert_eq!(value["record_kind"], "run_end");
    assert_eq!(value["snapshots_emitted"], 7);
    assert_eq!(value["final_step"], 10);
    assert_eq!(value["final_ball_count"], 0);
}

#[test]
fn test_console_summary_contains_required_fields() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 12;
    snapshot.time = 0.12;
    snapshot.kinetic_energy = 2.5;
    snapshot.potential_energy = -1.25;

    let line = frame_to_console_summary(&snapshot, Some(1.0));

    assert!(line.contains("step=12"));
    assert!(line.contains("time=0.12000"));
    assert!(line.contains("balls=0"));
    assert!(line.contains("kinetic=2.50000"));
    assert!(line.contains("potential=-1.25000"));
    assert!(line.contains("total=1.25000"));
    assert!(line.contains("dE=0.25000"));
    assert!(line.contains("operators=0/0/0/0"));
}

#[test]
fn test_jsonl_records_are_valid_json_lines_and_ordered() {
    let mut start_snapshot = RendererSnapshot::new();
    start_snapshot.simulation_step = 0;
    start_snapshot.time = 0.0;

    let mut frame_snapshot = RendererSnapshot::new();
    frame_snapshot.simulation_step = 2;
    frame_snapshot.time = 0.02;

    let mut end_snapshot = RendererSnapshot::new();
    end_snapshot.simulation_step = 3;
    end_snapshot.time = 0.03;

    let start = run_start_to_jsonl(&HeadlessConfig::default(), &start_snapshot);
    let frame = frame_to_jsonl(&frame_snapshot, Some(0.0));
    let end = run_end_to_jsonl(&end_snapshot, 2);

    let lines = [start, frame, end];
    let parsed: Vec<serde_json::Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();

    assert_eq!(parsed[0]["record_kind"], "run_start");
    assert_eq!(parsed[1]["record_kind"], "frame");
    assert_eq!(parsed[2]["record_kind"], "run_end");
    assert_eq!(parsed[0]["schema_version"], "headless.v2");
    assert_eq!(parsed[1]["schema_version"], "headless.v2");
    assert_eq!(parsed[2]["schema_version"], "headless.v2");
}

#[test]
fn test_summary_frame_excludes_large_ball_payload() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 1;
    snapshot.time = 0.01;

    let line = frame_to_jsonl(&snapshot, Some(0.0));
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert!(value.get("balls").is_none());
}

#[test]
fn test_detail_balls_includes_ball_fields() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.balls.push(
        moldyat_core::application::renderer_snapshot::BallRenderData {
            id: 9,
            species: SpeciesId::new(2),
            position: moldyat_core::objects2d::Vec2D::new(1.5, -2.5),
            velocity: moldyat_core::objects2d::Vec2D::new(0.25, -0.5),
            mass: 3.75,
            color: [1.0, 0.0, 0.0, 1.0],
            contact_radius: 0.7,
            display_radius: 0.7,
            equilibrium_radius: 0.8,
            cutoff_radius: 2.8,
            show_neighbor_lines: false,
            show_main_neighbors_only: false,
            neighbors: Vec::new(),
            update_counter: 0,
            tail: Vec::new(),
        },
    );
    snapshot.balls.push(
        moldyat_core::application::renderer_snapshot::BallRenderData {
            id: 3,
            species: SpeciesId::new(1),
            position: moldyat_core::objects2d::Vec2D::new(-0.5, 0.5),
            velocity: moldyat_core::objects2d::Vec2D::new(-0.1, 0.2),
            mass: 1.5,
            color: [0.0, 1.0, 0.0, 1.0],
            contact_radius: 0.5,
            display_radius: 0.5,
            equilibrium_radius: 0.6,
            cutoff_radius: 2.0,
            show_neighbor_lines: false,
            show_main_neighbors_only: false,
            neighbors: Vec::new(),
            update_counter: 1,
            tail: Vec::new(),
        },
    );

    let config = HeadlessConfig {
        detail: DetailLevel::Balls,
        ..HeadlessConfig::default()
    };
    let line = frame_to_jsonl_with_config(&config, &snapshot, Some(0.0));
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert_eq!(value["balls"][0]["id"], 3);
    assert_eq!(value["balls"][0]["species"], "XS");
    assert_eq!(value["balls"][1]["id"], 9);
    assert_eq!(value["balls"][1]["species"], "S");
    assert!(value["balls"][1].get("size_idx").is_none());
    assert!((value["balls"][1]["position"]["x"].as_f64().unwrap() - 1.5).abs() < 1e-6);
    assert!((value["balls"][1]["position"]["y"].as_f64().unwrap() + 2.5).abs() < 1e-6);
    assert!((value["balls"][1]["velocity"]["vx"].as_f64().unwrap() - 0.25).abs() < 1e-6);
    assert!((value["balls"][1]["velocity"]["vy"].as_f64().unwrap() + 0.5).abs() < 1e-6);
    assert!((value["balls"][1]["contact_radius"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    assert!((value["balls"][1]["mass"].as_f64().unwrap() - 3.75).abs() < 1e-6);
}

#[test]
fn test_non_finite_values_serialize_as_null() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 1;
    snapshot.time = f32::NAN;
    snapshot.kinetic_energy = f32::INFINITY;
    snapshot.potential_energy = f32::NEG_INFINITY;
    snapshot.physics_fps = f32::NAN;

    let line = frame_to_jsonl(&snapshot, Some(0.0));
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert!(value["time"].is_null());
    assert!(value["kinetic_energy"].is_null());
    assert!(value["potential_energy"].is_null());
    assert!(value["physics_fps"].is_null());
}

#[test]
fn test_thermo_detail_serializes_grids_and_non_finite_values_as_null() {
    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 7;
    snapshot.cell_temperature =
        moldyat_core::application::thermo_field::ThermoGrid::filled(1, 3, 1.0);
    snapshot.cell_temperature.set(0, 1, f32::NAN);
    snapshot.cell_pressure = moldyat_core::application::thermo_field::ThermoGrid::filled(1, 3, 2.0);
    snapshot.cell_pressure.set(0, 2, f32::INFINITY);
    snapshot.thermo_grid_spec = Some(
        moldyat_core::application::renderer_snapshot::ThermoGridSpec {
            rows: 1,
            cols: 3,
            cell_width: 4.0,
            cell_height: 5.0,
            origin_x: -1.5,
            origin_y: 2.5,
        },
    );

    let config = HeadlessConfig {
        detail: DetailLevel::Thermo,
        ..HeadlessConfig::default()
    };
    let line = frame_to_jsonl_with_config(&config, &snapshot, Some(0.0));
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert_eq!(value["thermo"]["temperature"]["rows"], 1);
    assert_eq!(value["thermo"]["temperature"]["cols"], 3);
    assert!(value["thermo"]["temperature"]["values"][0].is_number());
    assert!(value["thermo"]["temperature"]["values"][1].is_null());
    assert_eq!(value["thermo"]["pressure"]["values"][0], 2.0);
    assert!(value["thermo"]["pressure"]["values"][2].is_null());
    assert_eq!(value["thermo"]["grid_spec"]["rows"], 1);
    assert_eq!(value["thermo"]["grid_spec"]["cols"], 3);
    assert_eq!(value["thermo"]["grid_spec"]["cell_width"], 4.0);
}

#[test]
fn test_stdout_and_file_outputs_stay_separated_for_frame_records() {
    let output_path =
        std::env::temp_dir().join(format!("headless-output-test-{}.jsonl", std::process::id()));

    let config = HeadlessConfig {
        output: Some(output_path.clone()),
        ..HeadlessConfig::default()
    };

    let mut writer = HeadlessWriter::builder()
        .stdout(true)
        .output(Some(output_path.clone()))
        .config(config)
        .build()
        .expect("writer");

    let mut snapshot = RendererSnapshot::new();
    snapshot.simulation_step = 0;

    writer.write_run_start(&snapshot).expect("run_start");
    writer.write_frame(&snapshot).expect("frame");
    writer.write_run_end(&snapshot, 1).expect("run_end");

    drop(writer);
    let file_content = fs::read_to_string(&output_path).expect("read output file");
    let lines: Vec<&str> = file_content.lines().collect();
    assert_eq!(lines.len(), 3);

    let kinds: Vec<String> = lines
        .iter()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("valid jsonl record")["record_kind"]
                .as_str()
                .expect("record kind")
                .to_string()
        })
        .collect();
    assert_eq!(kinds, vec!["run_start", "frame", "run_end"]);

    fs::remove_file(output_path).expect("remove output file");
}

#[test]
fn test_runs_with_same_seed_and_config_emit_identical_jsonl() {
    struct TempFileGuard {
        path: PathBuf,
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
    let unique = format!(
        "{}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed"),
        nanos
    );
    let output_path_a = std::env::temp_dir().join(format!("headless-determinism-a-{unique}.jsonl"));
    let output_path_b = std::env::temp_dir().join(format!("headless-determinism-b-{unique}.jsonl"));
    let _guard_a = TempFileGuard {
        path: output_path_a.clone(),
    };
    let _guard_b = TempFileGuard {
        path: output_path_b.clone(),
    };

    let world_rect = moldyat_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        moldyat_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
    );

    let config_a = HeadlessConfig {
        steps: 6,
        snapshot_every: 2,
        output: Some(output_path_a.clone()),
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
    let config_b = HeadlessConfig {
        output: Some(output_path_b.clone()),
        ..config_a.clone()
    };

    let mut app_a = super::super::headless_runner::build_application(world_rect, config_a.rng_seed)
        .expect("build app a");
    app_a.seed_random_balls(config_a.seed_balls.expect("seed ball count") as usize);
    let runner_a = super::super::headless_runner::HeadlessRunner::from_config(config_a, app_a)
        .expect("runner a");
    let report_a = runner_a.run().expect("run a");
    assert_eq!(report_a.snapshots_emitted, 4);

    let mut app_b = super::super::headless_runner::build_application(world_rect, config_b.rng_seed)
        .expect("build app b");
    app_b.seed_random_balls(config_b.seed_balls.expect("seed ball count") as usize);
    let runner_b = super::super::headless_runner::HeadlessRunner::from_config(config_b, app_b)
        .expect("runner b");
    let report_b = runner_b.run().expect("run b");
    assert_eq!(report_b.snapshots_emitted, 4);

    let content_a = fs::read_to_string(&output_path_a).expect("read file a");
    let content_b = fs::read_to_string(&output_path_b).expect("read file b");
    assert_eq!(content_a, content_b);

    let lines_a: Vec<&str> = content_a.lines().collect();
    assert_eq!(lines_a.len(), 6);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines_a[0]).expect("run_start json")["record_kind"],
        "run_start"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines_a[1]).expect("frame0 json")["record_kind"],
        "frame"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(lines_a[5]).expect("run_end json")["record_kind"],
        "run_end"
    );
}
