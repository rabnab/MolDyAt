// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Upgrading old world files to the current format (#272).
//!
//! [`parse`] reads only the file's `version` first. A file of the current
//! version is deserialized directly. An older file is parsed into a
//! `serde_json::Value`, the migrations from its version up to
//! [`WORLD_FILE_VERSION`] run in order, and only then is it deserialized into
//! the current [`WorldFile`](super::WorldFile). Validation follows in both
//! cases. Saving always writes the current version.
//!
//! # Changing the format
//! - An additive change (a new optional field whose default reproduces the old
//!   behaviour) keeps the version.
//! - A breaking change (rename, removal, changed type, unit or meaning) bumps
//!   [`WORLD_FILE_VERSION`] to `N + 1` and adds one function
//!   `migrate_vN_to_vN+1(&mut Value)` to [`MIGRATIONS`], which rewrites a
//!   version-`N` file into the version-`N + 1` layout.
//! - **A migration must not panic on unexpected input.** It runs on users'
//!   files (Ctrl+O on the physics thread, `--load` before the run), so a
//!   missing or mistyped field must return an error, which leaves the running
//!   world untouched, not `unwrap` or index into the value. Use [`take_field`]
//!   and return `invalid(..)` for anything else unexpected.
//! - The new version also gets a reference save, and the migration a test that
//!   loads the version-`N` reference save (see `tests/reference_saves.rs`).

use super::format::{WORLD_FILE_VERSION, invalid};
use crate::error::AppResult;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// One step of the chain: rewrites a version-`from` file into version `from + 1`.
pub(crate) struct Migration {
    pub from: u32,
    /// Must not panic: return `invalid(..)` for missing or mistyped fields
    /// (see [`take_field`]).
    pub apply: fn(&mut Value) -> AppResult,
}

/// Every migration, oldest first. Entry `i` upgrades version `i + 1`, so the
/// list has `WORLD_FILE_VERSION - 1` entries. Version 1 is the first format;
/// there are no migrations yet.
pub(crate) const MIGRATIONS: &[Migration] = &[];

/// Parse a world file of any supported version into `T`, upgrading it first
/// if it is older than [`WORLD_FILE_VERSION`].
pub(crate) fn parse<T: DeserializeOwned>(json: &str) -> AppResult<T> {
    parse_with(json, MIGRATIONS, WORLD_FILE_VERSION)
}

/// [`parse`] with an explicit chain and current version, for tests.
pub(crate) fn parse_with<T: DeserializeOwned>(
    json: &str,
    migrations: &[Migration],
    current: u32,
) -> AppResult<T> {
    /// Only the version; serde skips every other field without building it.
    #[derive(serde::Deserialize)]
    struct Header {
        version: Option<Value>,
    }
    let header: Header = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
    let version = check_version(header.version.as_ref(), current)?;
    if version == current {
        // Fast path: building a `Value` tree first is many times slower for
        // large saves.
        return serde_json::from_str(json).map_err(|e| invalid(e.to_string()));
    }
    let mut value: Value = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
    upgrade_with(&mut value, migrations, current)?;
    log::info!("upgraded world file from format v{version} to v{current}");
    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))
}

/// Upgrade `file` in place to [`WORLD_FILE_VERSION`]; returns the version it
/// was written with.
#[cfg(test)]
pub(crate) fn upgrade(file: &mut Value) -> AppResult<u32> {
    upgrade_with(file, MIGRATIONS, WORLD_FILE_VERSION)
}

/// [`upgrade`] with an explicit chain and target version, for tests.
pub(crate) fn upgrade_with(
    file: &mut Value,
    migrations: &[Migration],
    current: u32,
) -> AppResult<u32> {
    let version = check_version(file.get("version"), current)?;
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

/// Remove and return `key`, or a clean error if the file has no such field.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "for migrations; the first one comes with I3 (#234)"
    )
)]
pub(crate) fn take_field(file: &mut Value, key: &str) -> AppResult<Value> {
    file.as_object_mut()
        .and_then(|object| object.remove(key))
        .ok_or_else(|| invalid(format!("missing field `{key}`")))
}

/// The file's version, if it is one this build can load (1 up to `current`).
fn check_version(version: Option<&Value>, current: u32) -> AppResult<u32> {
    let version = version
        .ok_or_else(|| invalid("missing version"))?
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| invalid("version must be a positive integer"))?;
    if version == 0 {
        return Err(invalid("version 0 is not a valid format version"));
    }
    if version > current {
        return Err(invalid(format!(
            "version {version} is newer than this build supports (up to {current})"
        )));
    }
    Ok(version)
}
