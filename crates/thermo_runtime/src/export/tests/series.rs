// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

#[test]
fn test_series_csv_round_trips_columns_units_and_gaps() {
    let registry = stepped_registry();
    let rows = parse_csv(&series_to_string(&registry, None, ExportFormat::Csv, None));
    assert_eq!(
        rows[0],
        [
            "step",
            "time",
            "E_kin [J]",
            "E_pot [J]",
            "E_tot [J]",
            "late",
            "wall_momentum [kg·m/s]"
        ]
    );
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[1][..2], ["10", "1"]);
    assert_eq!(rows[3][1], "3", "the simulation time of the sample");
    assert_eq!(rows[3][2].parse::<f32>().unwrap(), 2.5);
    // `late` has no value at the first step; E_tot and wall_momentum never had any.
    assert_eq!(rows[1][5], "");
    assert_eq!(rows[2][5].parse::<f32>().unwrap(), 100.0);
}

#[test]
fn test_series_selection_and_non_finite_values() {
    let mut registry = stepped_registry();
    registry.push_step_for_test(40);
    registry.record("E_kin", f32::NAN);
    registry.record("E_pot", f32::INFINITY);
    let names = ["E_kin".to_string()];
    let rows = parse_csv(&series_to_string(
        &registry,
        Some(&names),
        ExportFormat::Csv,
        None,
    ));
    assert_eq!(rows[0], ["step", "time", "E_kin [J]"]);
    assert_eq!(rows[4], ["40", "4", ""]);
}

#[test]
fn test_series_jsonl_round_trips_with_null_for_gaps() {
    let mut registry = stepped_registry();
    registry.push_step_for_test(40);
    registry.record("E_kin", f32::NAN);
    let text = series_to_string(&registry, None, ExportFormat::Jsonl, None);
    let records: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(records[0]["schema_version"], SCHEMA_VERSION);
    assert_eq!(records[0]["record_kind"], "series_header");
    assert_eq!(records[0]["rows"], 4);
    let e_kin = records[0]["series"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "E_kin")
        .unwrap();
    assert_eq!(e_kin["unit"], "J");
    assert_eq!(records[1]["record_kind"], "series_row");
    assert_eq!(records[1]["values"]["E_kin"], 0.5);
    assert_eq!(records[1]["step"], 10);
    assert_eq!(records[1]["time"], 1.0);
    assert!(records[1]["values"]["late"].is_null());
    assert!(records[4]["values"]["E_kin"].is_null());
}

#[test]
fn test_sample_interval_is_stated_only_when_the_run_was_thinned() {
    let registry = stepped_registry();
    let thinned = series_to_string(&registry, None, ExportFormat::Csv, Some(30));
    assert_eq!(thinned.lines().next(), Some("# sample_interval_steps=30"));
    assert!(thinned.lines().nth(1).unwrap().starts_with("step,"));

    let plain = series_to_string(&registry, None, ExportFormat::Csv, None);
    assert!(plain.starts_with("step,"));

    let header = |interval| {
        let text = series_to_string(&registry, None, ExportFormat::Jsonl, interval);
        serde_json::from_str::<serde_json::Value>(text.lines().next().unwrap()).unwrap()
    };
    assert_eq!(header(Some(30))["sample_interval_steps"], 30);
    assert!(header(None)["sample_interval_steps"].is_null());
}
