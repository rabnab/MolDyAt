// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Files written by a newer build (#273): a newer version and unknown
//! `config` keys are rejected, unknown fields elsewhere are skipped and
//! reported.

use super::{fixture, scratch_dir};
use crate::application::MainContent;
use crate::application::persistence::{LoadSource, SaveStore, WORLD_FILE_VERSION, WorldFile};
use crate::objects2d::Rect;
use serde_json::{Value, json};
use std::fs;

/// The v2 reference save with fields no build knows, at every depth: top
/// level, in a list entry, in a value type, in a tagged enum and in an
/// optional field.
fn v2_with_unknown_fields() -> Value {
    let mut file = fixture("v2.json");
    file["pistons"] = json!([{"id": 0}]);
    file["balls"][3]["charge"] = json!(-1.0);
    file["world_rect"]["z"] = json!(0.0);
    file["wall_chains"][0]["joints"][0]["colour"] = json!("red");
    file["thermostats"][0]["label"] = json!("hot");
    file
}

#[test]
fn unknown_fields_outside_config_are_skipped_and_named() {
    let json = v2_with_unknown_fields().to_string();
    let (file, ignored) = WorldFile::from_json_reporting(&json).unwrap();
    assert_eq!(
        file,
        WorldFile::from_json(&fixture("v2.json").to_string()).unwrap()
    );
    let mut ignored = ignored;
    ignored.sort();
    assert_eq!(
        ignored,
        [
            "balls[3].charge",
            "pistons",
            "thermostats[0].label",
            "wall_chains[0].joints[0].colour",
            "world_rect.z",
        ]
    );
}

#[test]
fn a_known_file_reports_no_unknown_fields() {
    let (_, ignored) = WorldFile::from_json_reporting(&fixture("v2.json").to_string()).unwrap();
    assert!(ignored.is_empty(), "{ignored:?}");
}

#[test]
fn unknown_fields_of_an_upgraded_file_are_named_too() {
    let mut file = fixture("v1.json");
    file["balls"][0]["charge"] = json!(1.0);
    let (_, ignored) = WorldFile::from_json_reporting(&file.to_string()).unwrap();
    assert_eq!(ignored, ["balls[0].charge"]);
}

#[test]
fn load_world_reports_unknown_fields_in_the_load_message() {
    let dir = scratch_dir("newer_files");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("newer.json");
    fs::write(&path, v2_with_unknown_fields().to_string()).unwrap();

    let mut app = MainContent::new(Rect::new(-20.0, 20.0, 40.0, -40.0)).unwrap();
    let loaded = app
        .load_world(&SaveStore::new(&dir), &LoadSource::Path(path), true)
        .unwrap();
    assert_eq!(loaded.ignored_fields.len(), 5);
    let message = loaded.message(true);
    assert!(message.contains("(paused)"), "{message}");
    assert!(message.contains("skipped 5 unknown fields"), "{message}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_field_repeated_in_a_list_is_reported_once_with_its_count() {
    let mut file = fixture("v2.json");
    let balls = file["balls"].as_array_mut().unwrap();
    let count = balls.len();
    for ball in balls.iter_mut() {
        ball["label"] = json!("a");
    }
    file["balls"][2]["charge"] = json!(1.0);
    let (_, ignored) = WorldFile::from_json_reporting(&file.to_string()).unwrap();
    assert_eq!(
        ignored,
        [
            format!("balls[*].label ({count} entries)"),
            "balls[2].charge".to_string()
        ]
    );
}

#[test]
fn the_warning_names_at_most_twenty_fields() {
    let fields: Vec<String> = (0..25).map(|i| format!("f{i}")).collect();
    let list = super::super::capped_list(&fields, 20);
    assert!(list.starts_with("f0, f1,"), "{list}");
    assert!(list.ends_with("f19 and 5 more"), "{list}");
    assert_eq!(super::super::capped_list(&fields[..2], 20), "f0, f1");
}

#[test]
fn newer_version_is_rejected_with_the_version_message() {
    let mut file = v2_with_unknown_fields();
    file["version"] = json!(WORLD_FILE_VERSION + 1);
    let error = format!("{:?}", WorldFile::from_json(&file.to_string()).unwrap_err());
    let expected = format!(
        "version {} is newer than this build supports (up to {WORLD_FILE_VERSION})",
        WORLD_FILE_VERSION + 1
    );
    assert!(error.contains(&expected), "{error}");
}

#[test]
fn unknown_config_key_is_rejected_with_the_newer_build_hint() {
    let mut file = fixture("v2.json");
    file["config"] = json!({"potential": {"charge_scale": 2.0}});
    let error = format!("{:?}", WorldFile::from_json(&file.to_string()).unwrap_err());
    assert!(error.contains("charge_scale"), "{error}");
    assert!(error.contains("newer MolDyAt build"), "{error}");
}
