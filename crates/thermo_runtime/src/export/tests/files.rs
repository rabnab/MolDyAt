// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("moldyat_export_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn test_write_export_creates_dir_and_never_overwrites_when_asked() {
    let dir = temp_dir("keep").join("nested");
    let write = |text: &str| {
        write_export(
            &dir,
            "20260101_000000",
            ExportKind::Series,
            ExportFormat::Csv,
            text,
            true,
        )
        .unwrap()
    };
    let first = write("a\n");
    let second = write("b\n");
    assert!(first.ends_with("20260101_000000_series.csv"), "{first:?}");
    assert!(
        second.ends_with("20260101_000000_series_01.csv"),
        "{second:?}"
    );
    assert_eq!(std::fs::read_to_string(first).unwrap(), "a\n");
    assert_eq!(std::fs::read_to_string(second).unwrap(), "b\n");
    std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn test_write_export_with_a_fixed_name_overwrites() {
    let dir = temp_dir("fixed");
    let write = |text: &str| {
        write_export(
            &dir,
            "",
            ExportKind::Snapshot,
            ExportFormat::Jsonl,
            text,
            false,
        )
        .unwrap()
    };
    write("a\n");
    let path = write("b\n");
    assert!(path.ends_with("snapshot.jsonl"), "{path:?}");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "b\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_write_export_leaves_an_unrelated_tmp_file_alone() {
    let dir = temp_dir("tmp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("series.tmp"), "mine").unwrap();
    write_export(
        &dir,
        "",
        ExportKind::Series,
        ExportFormat::Csv,
        "x\n",
        false,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("series.tmp")).unwrap(),
        "mine"
    );
    assert!(
        !dir.join("series.csv.tmp").exists(),
        "temporary file is renamed away"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
