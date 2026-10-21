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
        from: 2,
        apply: must_not_run,
    }];
    let v2_json = serde_json::to_string(&fixture("v2.json")).unwrap();
    let (file, _): (WorldFile, _) = parse_with(&v2_json, &chain, 2).unwrap();
    assert_eq!(file.version, 2);
}

// ── v1 → v2 (#234): size classes become species ─────────────────

#[test]
fn v1_size_classes_become_species_names() {
    let mut file = v1_fixture();
    let before = file.clone();
    assert_eq!(upgrade(&mut file).unwrap(), 1);
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
