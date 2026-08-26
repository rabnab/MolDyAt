use super::*;
use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;

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
