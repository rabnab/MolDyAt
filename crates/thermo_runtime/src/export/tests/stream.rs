// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::application::renderer_snapshot::RendererSnapshot;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("moldyat_stream_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn settings(dir: &std::path::Path, format: ExportFormat) -> StreamSettings {
    StreamSettings {
        dir: dir.to_path_buf(),
        stem: "stream".into(),
        format,
        replace_existing: true,
    }
}

/// A snapshot of a small world, to be relabelled per sample.
fn base_snapshot() -> RendererSnapshot {
    let (mut content, _) = create_main_content();
    content.add_random_candidates(3);
    content.create_snapshot()
}

/// Sample `snapshot` as `step` of `generation`, then stream it.
fn sample(
    registry: &mut MetricsRegistry,
    stream: &mut SampleStream,
    snapshot: &mut RendererSnapshot,
    generation: u64,
    step: u64,
) -> Option<PathBuf> {
    snapshot.world_generation = generation;
    snapshot.simulation_step = step;
    snapshot.time = step as f32 * 0.5;
    snapshot.kinetic_energy = step as f32;
    registry.sample(snapshot);
    stream.record(registry).unwrap()
}

#[test]
fn test_stream_writes_every_sample_although_the_registry_evicts() {
    let dir = temp_dir("evict");
    let mut registry = MetricsRegistry::new(4);
    let mut stream = SampleStream::new(settings(&dir, ExportFormat::Csv), &registry);
    let mut snapshot = base_snapshot();
    let mut started = Vec::new();
    for step in 0..10 {
        started.extend(sample(
            &mut registry,
            &mut stream,
            &mut snapshot,
            0,
            step * 10,
        ));
    }
    assert_eq!(registry.steps().len(), 4, "the registry kept only four");
    assert_eq!(stream.rows(), 10);
    let files = stream.finish().unwrap();
    assert_eq!(files, started, "one file, reported when it was started");
    assert!(files[0].ends_with("stream.csv"), "{files:?}");

    let rows = parse_csv(&std::fs::read_to_string(&files[0]).unwrap());
    assert_eq!(rows[0][..2], ["step", "time"]);
    assert!(rows[0].contains(&"wall_momentum [kg·m/s]".to_string()));
    let e_kin = rows[0].iter().position(|c| c == "E_kin [J]").unwrap();
    assert_eq!(rows.len(), 11, "header and one row per sample");
    assert!(rows.iter().all(|row| row.len() == rows[0].len()));
    assert_eq!([&rows[1][0], &rows[1][1], &rows[1][e_kin]], ["0", "0", "0"]);
    assert_eq!(
        [&rows[10][0], &rows[10][1], &rows[10][e_kin]],
        ["90", "45", "90"]
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_stream_starts_with_the_next_sample() {
    let dir = temp_dir("next");
    let mut registry = MetricsRegistry::new(100);
    let mut snapshot = base_snapshot();
    snapshot.simulation_step = 1;
    registry.sample(&snapshot);
    let mut stream = SampleStream::new(settings(&dir, ExportFormat::Csv), &registry);
    assert_eq!(stream.record(&registry).unwrap(), None, "nothing new yet");
    assert!(stream.files().is_empty(), "no file before the first sample");
    sample(&mut registry, &mut stream, &mut snapshot, 0, 2);
    let files = stream.finish().unwrap();
    let rows = parse_csv(&std::fs::read_to_string(&files[0]).unwrap());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1][0], "2");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_stream_jsonl_round_trips_units_and_non_finite_values() {
    let dir = temp_dir("jsonl");
    let mut registry = MetricsRegistry::new(100);
    let mut stream = SampleStream::new(settings(&dir, ExportFormat::Jsonl), &registry);
    let mut snapshot = base_snapshot();
    sample(&mut registry, &mut stream, &mut snapshot, 0, 1);
    snapshot.potential_energy = f32::NAN;
    sample(&mut registry, &mut stream, &mut snapshot, 0, 2);
    let files = stream.finish().unwrap();

    let records: Vec<serde_json::Value> = std::fs::read_to_string(&files[0])
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    let header = &records[0];
    assert_eq!(header["schema_version"], SCHEMA_VERSION);
    assert_eq!(header["record_kind"], "stream_header");
    let e_kin = header["series"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "E_kin")
        .unwrap();
    assert_eq!(e_kin["unit"], "J");
    let row = &records[2];
    assert_eq!(row["record_kind"], "series_row");
    assert_eq!(row["step"], 2);
    assert_eq!(row["time"], 1.0);
    assert_eq!(row["values"]["E_kin"], 2.0);
    assert!(row["values"]["E_pot"].is_null(), "NaN is null: {row}");
    assert!(
        row["values"]["E_tot"].is_null(),
        "E_kin + NaN is null: {row}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_stream_starts_a_new_file_for_a_new_world() {
    let dir = temp_dir("generation");
    let mut registry = MetricsRegistry::new(100);
    let mut stream = SampleStream::new(settings(&dir, ExportFormat::Csv), &registry);
    let mut snapshot = base_snapshot();
    for step in 1..=3 {
        sample(&mut registry, &mut stream, &mut snapshot, 0, step);
    }
    let second = sample(&mut registry, &mut stream, &mut snapshot, 1, 100);
    sample(&mut registry, &mut stream, &mut snapshot, 1, 101);
    let files = stream.finish().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(second.as_ref(), Some(&files[1]));
    assert!(files[1].ends_with("stream_01.csv"), "{files:?}");

    let steps = |path: &PathBuf| -> Vec<String> {
        parse_csv(&std::fs::read_to_string(path).unwrap())[1..]
            .iter()
            .map(|row| row[0].clone())
            .collect()
    };
    assert_eq!(steps(&files[0]), ["1", "2", "3"]);
    assert_eq!(steps(&files[1]), ["100", "101"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_stream_replaces_or_keeps_an_existing_file_as_asked() {
    let dir = temp_dir("existing");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("stream.csv"), "old\n").unwrap();
    let mut snapshot = base_snapshot();
    for (replace_existing, expected) in [(false, "stream_01.csv"), (true, "stream.csv")] {
        let mut registry = MetricsRegistry::new(100);
        let settings = StreamSettings {
            replace_existing,
            ..settings(&dir, ExportFormat::Csv)
        };
        let mut stream = SampleStream::new(settings, &registry);
        let path = sample(&mut registry, &mut stream, &mut snapshot, 0, 1).unwrap();
        stream.finish().unwrap();
        assert!(path.ends_with(expected), "{path:?}");
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("step,"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_stream_into_an_unwritable_dir_fails_at_the_first_sample() {
    let dir = temp_dir("unwritable");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("not_a_dir");
    std::fs::write(&file, "").unwrap();
    let mut registry = MetricsRegistry::new(100);
    let mut stream = SampleStream::new(settings(&file.join("sub"), ExportFormat::Csv), &registry);
    let mut snapshot = base_snapshot();
    snapshot.simulation_step = 1;
    registry.sample(&snapshot);
    assert!(stream.record(&registry).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
