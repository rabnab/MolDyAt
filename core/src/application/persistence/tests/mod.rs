// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tests for saving and loading the world, one file per layer.

mod format;
mod migrate;
mod newer_files;
mod reference_saves;
mod roundtrip;
mod store;

use crate::application::persistence::WorldFile;
use std::fs;
use std::path::PathBuf;

pub(super) fn sample_file() -> WorldFile {
    WorldFile::from_json(
        r#"{
            "version": 2,
            "world_rect": {"x": -20.0, "y": 20.0, "w": 40.0, "h": -40.0},
            "time_step": 0.01,
            "gravity": 0.0,
            "balls": [
                {"id": 1, "species": "M", "pos": {"x": 1.0, "y": 2.0}, "v": {"x": 0.5, "y": -0.5},
                 "pre_update_pos": {"x": 0.9, "y": 2.1}, "accel": {"x": 0.0, "y": 0.0}}
            ],
            "line_segments": [], "valves": [], "wall_chains": [],
            "source_spheres": [], "thermostats": []
        }"#,
    )
    .expect("sample file is valid")
}

pub(super) fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mlj_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// A reference save from `tests/fixtures/` as JSON.
pub(super) fn fixture(name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/application/persistence/tests/fixtures")
        .join(name);
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}
