use super::*;
use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;
use std::fs;

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

    assert_eq!(value["schema_version"], "headless.v1");
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
    };

    let line = run_start_to_jsonl(&config, &snapshot);
    let value: serde_json::Value = serde_json::from_str(&line).expect("valid json");

    assert_eq!(value["schema_version"], "headless.v1");
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

    assert_eq!(value["schema_version"], "headless.v1");
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

    let lines = vec![start, frame, end];
    let parsed: Vec<serde_json::Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();

    assert_eq!(parsed[0]["record_kind"], "run_start");
    assert_eq!(parsed[1]["record_kind"], "frame");
    assert_eq!(parsed[2]["record_kind"], "run_end");
    assert_eq!(parsed[0]["schema_version"], "headless.v1");
    assert_eq!(parsed[1]["schema_version"], "headless.v1");
    assert_eq!(parsed[2]["schema_version"], "headless.v1");
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
