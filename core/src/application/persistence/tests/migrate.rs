// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::persistence::WORLD_FILE_VERSION;
use crate::application::persistence::migrate::{MIGRATIONS, Migration, upgrade, upgrade_with};
use crate::error::AppResult;
use serde_json::{Value, json};
use std::path::PathBuf;

fn v1_fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/application/persistence/tests/fixtures/v1.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn error_text(result: AppResult<u32>) -> String {
    format!("{:?}", result.unwrap_err())
}

/// Test-only v1 → v2: `time_step` is renamed to `dt`.
fn rename_time_step(file: &mut Value) -> AppResult {
    let object = file.as_object_mut().unwrap();
    let dt = object.remove("time_step").unwrap();
    object.insert("dt".into(), dt);
    Ok(())
}

/// Test-only v2 → v3: `dt` is stored in milliseconds; needs v2's `dt`.
fn dt_to_milliseconds(file: &mut Value) -> AppResult {
    let dt = file["dt"].as_f64().expect("v2 has dt");
    file["dt_ms"] = json!(dt * 1000.0);
    file.as_object_mut().unwrap().remove("dt");
    Ok(())
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
