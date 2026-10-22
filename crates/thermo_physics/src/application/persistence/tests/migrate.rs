// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::fixture;
use crate::application::persistence::WORLD_FILE_VERSION;
use crate::application::persistence::WorldFile;
use crate::application::persistence::migrate::{
    MIGRATIONS, Migration, parse_with, take_field, upgrade, upgrade_with,
};
use crate::error::AppResult;
use serde_json::{Value, json};

fn v1_fixture() -> Value {
    fixture("v1.json")
}

fn v1_json() -> String {
    serde_json::to_string(&v1_fixture()).unwrap()
}

fn error_text<T: std::fmt::Debug>(result: AppResult<T>) -> String {
    format!("{:?}", result.unwrap_err())
}

/// Test-only v1 → v2: `time_step` is renamed to `dt`. Written like a real
/// migration: no panics on unexpected input.
fn rename_time_step(file: &mut Value) -> AppResult {
    let dt = take_field(file, "time_step")?;
    file["dt"] = dt;
    Ok(())
}

/// Test-only v2 → v3: `dt` is stored in milliseconds; needs v2's `dt`.
fn dt_to_milliseconds(file: &mut Value) -> AppResult {
    let dt = take_field(file, "dt")?;
    let dt = dt
        .as_f64()
        .ok_or_else(|| crate::error::AppError::CustomError("dt is not a number".into()))?;
    file["dt_ms"] = json!(dt * 1000.0);
    Ok(())
}

fn must_not_run(_: &mut Value) -> AppResult {
    panic!("a current-version file went through a migration");
}

const TEST_CHAIN: &[Migration] = &[
    Migration {
        from: 1,
        apply: rename_time_step,
    },
    Migration {
        from: 2,
        apply: dt_to_milliseconds,
    },
];

#[test]
fn migrations_form_a_chain_up_to_the_current_version() {
    assert_eq!(MIGRATIONS.len() as u32, WORLD_FILE_VERSION - 1);
    for (i, migration) in MIGRATIONS.iter().enumerate() {
        assert_eq!(migration.from, i as u32 + 1, "migration {i}");
    }
}

#[test]
fn current_version_passes_through_unchanged() {
    let mut file = v1_fixture();
    let before = file.clone();
    assert_eq!(upgrade_with(&mut file, TEST_CHAIN, 1).unwrap(), 1);
    assert_eq!(file, before);
    assert_eq!(upgrade(&mut v1_fixture()).unwrap(), 1);
}

#[test]
fn v1_reference_save_upgrades_through_a_test_migration() {
    let mut file = v1_fixture();
    let before = file.clone();
    assert_eq!(upgrade_with(&mut file, TEST_CHAIN, 2).unwrap(), 1);

    assert_eq!(file["version"], 2);
    assert_eq!(file["dt"], before["time_step"]);
    assert!(file.get("time_step").is_none());
    assert_eq!(file["balls"], before["balls"]);
    assert_eq!(file["wall_chains"], before["wall_chains"]);
}

#[test]
fn migrations_run_in_order() {
    let mut file = v1_fixture();
    assert_eq!(upgrade_with(&mut file, TEST_CHAIN, 3).unwrap(), 1);
    assert_eq!(file["version"], 3);
    assert_eq!(file["dt_ms"], json!(5.0));
    assert!(file.get("dt").is_none());
}

#[test]
fn missing_migration_is_an_error() {
    let mut file = v1_fixture();
    let error = error_text(upgrade_with(&mut file, &TEST_CHAIN[..1], 3));
    assert!(error.contains("no migration from version 2"), "{error}");
}

#[test]
fn newer_version_is_rejected() {
    let mut file = json!({"version": WORLD_FILE_VERSION + 1});
    let error = error_text(upgrade(&mut file));
    assert!(error.contains("newer than this build"), "{error}");
}

#[test]
fn missing_zero_or_malformed_version_is_rejected() {
    for (file, expected) in [
        (json!({}), "missing version"),
        (json!({"version": 0}), "version 0"),
        (json!({"version": -1}), "positive integer"),
        (json!({"version": "1"}), "positive integer"),
        (json!({"version": 1.5}), "positive integer"),
    ] {
        let error = error_text(upgrade(&mut file.clone()));
        assert!(error.contains(expected), "{file}: {error}");
    }
}

#[test]
fn migration_on_a_file_missing_a_field_is_an_error_not_a_panic() {
    let mut file = v1_fixture();
    file.as_object_mut().unwrap().remove("time_step");
    let error = error_text(upgrade_with(&mut file, TEST_CHAIN, 2));
    assert!(error.contains("missing field `time_step`"), "{error}");
}

#[test]
fn current_version_file_skips_the_migration_path() {
    let chain = [Migration {
        from: WORLD_FILE_VERSION,
        apply: must_not_run,
    }];
    let current = format!("v{WORLD_FILE_VERSION}.json");
    let json = serde_json::to_string(&fixture(&current)).unwrap();
    let (file, _): (WorldFile, _) = parse_with(&json, &chain, WORLD_FILE_VERSION).unwrap();
    assert_eq!(file.version, WORLD_FILE_VERSION);
}

// ── v1 → v2 (#234): size classes become species ─────────────────

#[test]
fn v1_size_classes_become_species_names() {
    let mut file = v1_fixture();
    let before = file.clone();
    assert_eq!(upgrade_with(&mut file, MIGRATIONS, 2).unwrap(), 1);
    assert_eq!(file["version"], 2);

    let names = ["XS", "S", "M", "L"];
    let balls = file["balls"].as_array().unwrap();
    for (ball, old) in balls.iter().zip(before["balls"].as_array().unwrap()) {
        let size_idx = old["size_idx"].as_u64().unwrap() as usize;
        assert_eq!(ball["species"], names[size_idx], "ball {}", ball["id"]);
        assert!(ball.get("size_idx").is_none());
        assert_eq!(ball["pos"], old["pos"]);
    }
    let spheres = file["source_spheres"].as_array().unwrap();
    assert_eq!(spheres[0]["ball_species"], "M");
    assert_eq!(spheres[1]["ball_species"], "S");
    assert!(spheres[0].get("ball_size_idx").is_none());
    // Everything else is untouched.
    assert_eq!(file["wall_chains"], before["wall_chains"]);
    assert_eq!(file["thermostats"], before["thermostats"]);
}

#[test]
fn v1_size_class_out_of_range_is_an_error_not_a_panic() {
    for bad in [json!(5), json!(-1), json!("M"), json!(null)] {
        let mut file = v1_fixture();
        file["balls"][0]["size_idx"] = bad.clone();
        let error = error_text(upgrade(&mut file));
        assert!(error.contains("is not a size class"), "{bad}: {error}");
    }
}

#[test]
fn v1_ball_without_size_class_is_an_error_not_a_panic() {
    let mut file = v1_fixture();
    file["source_spheres"][1]
        .as_object_mut()
        .unwrap()
        .remove("ball_size_idx");
    let error = error_text(upgrade(&mut file));
    assert!(error.contains("missing field `ball_size_idx`"), "{error}");

    let mut file = v1_fixture();
    file["balls"] = json!({"not": "a list"});
    let error = error_text(upgrade(&mut file));
    assert!(error.contains("`balls` must be a list"), "{error}");
}

#[test]
fn older_file_is_parsed_through_the_migrations() {
    let (file, _): (Value, _) = parse_with(&v1_json(), TEST_CHAIN, 3).unwrap();
    assert_eq!(file["version"], 3);
    assert_eq!(file["dt_ms"], json!(5.0));
}

#[test]
fn parse_rejects_bad_versions_before_reading_the_rest() {
    for (json, expected) in [
        (r#"{"balls": []}"#, "missing version"),
        (r#"{"version": 0}"#, "version 0"),
        (r#"{"version": "1"}"#, "positive integer"),
        (r#"{"version": 99}"#, "newer than this build"),
    ] {
        let error = error_text(parse_with::<Value>(json, TEST_CHAIN, 3));
        assert!(error.contains(expected), "{json}: {error}");
    }
}

// ── v2 → v3 (#274): one operator list, chains own pieces and valves ──

fn v2_fixture() -> Value {
    fixture("v2.json")
}

fn upgrade_v2(file: &mut Value) -> AppResult<u32> {
    upgrade_with(file, MIGRATIONS, 3)
}

#[test]
fn v2_upgrades_to_the_v3_reference_save() {
    let mut file = v2_fixture();
    assert_eq!(upgrade_v2(&mut file).unwrap(), 2);
    assert_eq!(file, fixture("v3.json"));
}

#[test]
fn v2_chains_take_in_their_segments_and_valves() {
    let mut file = v2_fixture();
    let before = file.clone();
    upgrade_v2(&mut file).unwrap();
    for key in [
        "line_segments",
        "valves",
        "wall_chains",
        "source_spheres",
        "thermostats",
    ] {
        assert!(file.get(key).is_none(), "{key}");
    }
    let chain = &file["operators"][0]["wall_chain"];
    let segment = &before["line_segments"][0];
    assert_eq!(chain["pieces"][0]["id"], segment["id"]);
    assert_eq!(chain["pieces"][0]["start"], segment["start"]);
    assert_eq!(chain["pieces"][0]["thickness"], segment["thickness"]);
    assert!(chain["pieces"][0].get("line_id").is_none());
    assert_eq!(
        chain["joints"][0]["is_closed"],
        before["valves"][0]["is_closed"]
    );
    let kinds: Vec<&str> = file["operators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_object().unwrap().keys().next().unwrap().as_str())
        .collect();
    let chains = before["wall_chains"].as_array().unwrap().len();
    assert_eq!(kinds[..chains], vec!["wall_chain"; chains]);
    assert_eq!(
        kinds[chains..],
        ["source_sphere", "source_sphere", "thermostat", "thermostat"]
    );
}

#[test]
fn v2_segments_and_valves_without_a_chain_are_dropped() {
    let mut file = v2_fixture();
    file["line_segments"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": 99, "start": {"x": 0.0, "y": 0.0}, "end": {"x": 1.0, "y": 0.0}, "thickness": 1.0}));
    file["valves"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": 98, "is_closed": true}));
    upgrade_v2(&mut file).unwrap();
    let text = file.to_string();
    assert!(!text.contains("\"id\":99") && !text.contains("\"valve_id\":98"));
}

#[test]
fn v2_chain_with_a_missing_or_shared_segment_is_an_error_not_a_panic() {
    let mut file = v2_fixture();
    file["wall_chains"][0]["pieces"][0]["line_id"] = json!(77);
    let error = error_text(upgrade_v2(&mut file));
    assert!(error.contains("line segment 77"), "{error}");

    let mut file = v2_fixture();
    file["wall_chains"][1]["pieces"][0]["line_id"] = json!(0);
    let error = error_text(upgrade_v2(&mut file));
    assert!(error.contains("belongs to another chain"), "{error}");

    let mut file = v2_fixture();
    file["wall_chains"][1]["joints"][0]["valve_id"] = json!(0);
    let error = error_text(upgrade_v2(&mut file));
    assert!(error.contains("valve 0"), "{error}");
}

#[test]
fn v2_malformed_lists_are_an_error_not_a_panic() {
    for (key, bad) in [
        ("line_segments", json!({"not": "a list"})),
        ("valves", json!([{"is_closed": true}])),
        ("wall_chains", json!([{"pieces": 3}])),
        ("thermostats", json!(null)),
    ] {
        let mut file = v2_fixture();
        file[key] = bad.clone();
        assert!(upgrade_v2(&mut file).is_err(), "{key}: {bad}");
    }
    let mut file = v2_fixture();
    file.as_object_mut().unwrap().remove("valves");
    let error = error_text(upgrade_v2(&mut file));
    assert!(error.contains("missing field `valves`"), "{error}");
}
