// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{sample_file, scratch_dir};
use crate::application::persistence::store::{civil_from_days, save_order, timestamp_utc};
use crate::application::persistence::{LoadSource, SaveStore};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn civil_date_conversion() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    assert_eq!(civil_from_days(19_782), (2024, 2, 29));
}

#[test]
fn timestamp_format() {
    let time = UNIX_EPOCH + Duration::from_secs(1_709_210_096); // 2024-02-29 12:34:56 UTC
    assert_eq!(timestamp_utc(time), "20240229_123456");
}

#[test]
fn saves_never_overwrite_and_latest_is_newest() {
    let dir = scratch_dir("latest");
    let store = SaveStore::new(&dir);
    assert_eq!(store.latest(), None);

    let file = sample_file();
    let first = store.save(&file).unwrap();
    let second = store.save(&file).unwrap();
    let third = store.save(&file).unwrap();
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_eq!(store.latest(), Some(third.clone()));

    // Stray files and leftover temporary files are not saves.
    fs::write(dir.join("notes.txt"), "x").unwrap();
    fs::write(dir.join("world_99999999_999999.json.tmp"), "x").unwrap();
    assert_eq!(store.latest(), Some(third.clone()));

    assert_eq!(store.load(&third).unwrap(), file);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn resolve_finds_latest_or_takes_the_given_path() {
    let dir = scratch_dir("resolve");
    let store = SaveStore::new(&dir);
    let error = store.resolve(&LoadSource::Latest).unwrap_err();
    assert!(format!("{error:?}").contains("no saved world"));

    let saved = store.save(&sample_file()).unwrap();
    assert_eq!(store.resolve(&LoadSource::Latest).unwrap(), saved);
    let other = PathBuf::from("elsewhere/world.json");
    assert_eq!(
        store.resolve(&LoadSource::Path(other.clone())).unwrap(),
        other
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_names_the_file_on_error() {
    let dir = scratch_dir("bad");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("world_bad.json");
    fs::write(&path, "{ not json").unwrap();
    let error = SaveStore::new(&dir).load(&path).unwrap_err();
    assert!(format!("{error:?}").contains("world_bad.json"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_timestamped_names_are_saves() {
    assert_eq!(
        save_order("world_20260101_120000.json"),
        Some(("20260101_120000", 0))
    );
    assert_eq!(
        save_order("world_20260101_120000_07.json"),
        Some(("20260101_120000", 7))
    );
    assert_eq!(
        save_order("world_20260101_120000_100.json"),
        Some(("20260101_120000", 100))
    );
    for name in [
        "world_backup.json",
        "world_test.json",
        "world_2026010_120000.json",
        "world_20260101-120000.json",
        "world_20260101_120000_.json",
        "world_20260101_120000_x1.json",
        "world_20260101_120000.json.tmp",
        "notes.txt",
    ] {
        assert_eq!(save_order(name), None, "{name}");
    }
}

#[test]
fn latest_ignores_hand_named_files_and_orders_suffixes_numerically() {
    let dir = scratch_dir("naming");
    fs::create_dir_all(&dir).unwrap();
    let json = sample_file().to_json().unwrap();
    for name in [
        "world_20260101_120000.json",
        "world_20260101_120000_99.json",
        "world_20260101_120000_100.json",
        "world_backup.json",
        "world_zzz.json",
    ] {
        fs::write(dir.join(name), &json).unwrap();
    }
    let store = SaveStore::new(&dir);
    assert_eq!(
        store.latest(),
        Some(dir.join("world_20260101_120000_100.json"))
    );
    let _ = fs::remove_dir_all(&dir);
}
