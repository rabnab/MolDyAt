// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Upgrading old world files to the current format (#272).
//!
//! Loading has two stages: the file is parsed into a `serde_json::Value`, its
//! `version` is read and the migrations from that version up to
//! [`WORLD_FILE_VERSION`] run in order. Only then is it deserialized into the
//! current [`WorldFile`](super::WorldFile) and validated. Saving always
//! writes the current version.
//!
//! # Changing the format
//! - An additive change (a new optional field whose default reproduces the old
//!   behaviour) keeps the version.
//! - A breaking change (rename, removal, changed type, unit or meaning) bumps
//!   [`WORLD_FILE_VERSION`] to `N + 1` and adds one function
//!   `migrate_vN_to_vN+1(&mut Value)` to [`MIGRATIONS`], which rewrites a
//!   version-`N` file into the version-`N + 1` layout.
//! - The new version also gets a reference save, and the migration a test that
//!   loads the version-`N` reference save (see `tests/reference_saves.rs`).

use super::format::{WORLD_FILE_VERSION, invalid};
use crate::error::AppResult;
use serde_json::Value;

/// One step of the chain: rewrites a version-`from` file into version `from + 1`.
pub(crate) struct Migration {
    pub from: u32,
    pub apply: fn(&mut Value) -> AppResult,
}

/// Every migration, oldest first. Entry `i` upgrades version `i + 1`, so the
/// list has `WORLD_FILE_VERSION - 1` entries. Version 1 is the first format;
/// there are no migrations yet.
pub(crate) const MIGRATIONS: &[Migration] = &[];

/// Upgrade `file` in place to [`WORLD_FILE_VERSION`]; returns the version it
/// was written with.
pub(crate) fn upgrade(file: &mut Value) -> AppResult<u32> {
    upgrade_with(file, MIGRATIONS, WORLD_FILE_VERSION)
}

/// [`upgrade`] with an explicit chain and target version, for tests.
pub(crate) fn upgrade_with(
    file: &mut Value,
    migrations: &[Migration],
    current: u32,
) -> AppResult<u32> {
    let version = read_version(file)?;
    if version > current {
        return Err(invalid(format!(
            "version {version} is newer than this build supports (up to {current})"
        )));
    }
    for from in version..current {
        let migration = migrations
            .iter()
            .find(|m| m.from == from)
            .ok_or_else(|| invalid(format!("no migration from version {from}")))?;
        (migration.apply)(file)?;
        file["version"] = Value::from(from + 1);
    }
    Ok(version)
}

fn read_version(file: &Value) -> AppResult<u32> {
    let version = file
        .get("version")
        .ok_or_else(|| invalid("missing version"))?
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| invalid("version must be a positive integer"))?;
    if version == 0 {
        return Err(invalid("version 0 is not a valid format version"));
    }
    Ok(version)
}
