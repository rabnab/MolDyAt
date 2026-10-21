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
//! Both paths report the fields the file types do not know (#273): `serde`
//! skips them, and [`serde_ignored`] collects their paths (`balls[3].charge`)
//! so the load can warn about them. Unknown keys inside `config` are not
//! skipped but rejected (`SimConfig` uses `deny_unknown_fields`).
//!
//! # Changing the format
//! - An additive change (a new field with `#[serde(default)]` whose default
//!   reproduces the old behaviour, or a new `#[serde(default)]` list) keeps
//!   the version and needs no migration; see the module doc of
//!   [`persistence`](super) for the rule.
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
use crate::objects2d::ball_impl::species::SIZE_CLASS_NAMES;
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
/// list has `WORLD_FILE_VERSION - 1` entries.
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 1,
        apply: migrate_v1_to_v2,
    },
    Migration {
        from: 2,
        apply: migrate_v2_to_v3,
    },
];

/// v1 → v2 (#234): size classes became species. A ball's `size_idx` becomes
/// `species` with the name of the former size class (`XS` to `XL`), whose `σ`
/// keeps every pair's old equilibrium distance; a source sphere's
/// `ball_size_idx` becomes `ball_species` the same way.
fn migrate_v1_to_v2(file: &mut Value) -> AppResult {
    for (list, from, to) in [
        ("balls", "size_idx", "species"),
        ("source_spheres", "ball_size_idx", "ball_species"),
    ] {
        let entries = file
            .get_mut(list)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid(format!("`{list}` must be a list")))?;
        for entry in entries {
            let size_idx = take_field(entry, from)?;
            let name = size_idx
                .as_u64()
                .and_then(|i| SIZE_CLASS_NAMES.get(usize::try_from(i).ok()?))
                .ok_or_else(|| invalid(format!("`{from}` {size_idx} is not a size class")))?;
            entry
                .as_object_mut()
                .ok_or_else(|| invalid(format!("entries of `{list}` must be objects")))?
                .insert(to.to_string(), Value::from(*name));
        }
    }
    Ok(())
}

/// A parsed file and the fields `T` does not know, which were skipped, in
/// file order of their first occurrence (see [`IgnoredFields::summary`]).
pub(crate) type Parsed<T> = (T, Vec<String>);

/// v2 → v3 (#274): the lists `line_segments`, `valves`, `wall_chains`,
/// `source_spheres` and `thermostats` become one `operators` list of
/// `{"<kind>": {...}}` entries. A wall chain takes in its pieces' segments
/// (`line_id` becomes the piece's `id`, with `start`, `end`, `thickness`)
/// and its valves' `is_closed`. Segments and valves no chain refers to had
/// no effect (walls are built from chains) and are dropped with a warning.
/// A segment or valve that two chains share is an error, although v2
/// validation only checked that it existed: the app never writes such a
/// file, and in v3 each piece and valve belongs to exactly one chain.
fn migrate_v2_to_v3(file: &mut Value) -> AppResult {
    let mut segments = objects_by_id(take_field(file, "line_segments")?, "line_segments")?;
    let mut valves = objects_by_id(take_field(file, "valves")?, "valves")?;
    let mut operators = Vec::new();
    for mut chain in list(take_field(file, "wall_chains")?, "wall_chains")? {
        for piece in list_mut(&mut chain, "pieces")? {
            let line_id = take_field(piece, "line_id")?;
            let segment = take_by_id(&mut segments, &line_id, "line segment")?;
            let piece = object_mut(piece, "pieces")?;
            piece.insert("id".into(), line_id);
            for key in ["start", "end", "thickness"] {
                let value = segment
                    .get(key)
                    .ok_or_else(|| invalid(format!("line segment is missing `{key}`")))?;
                piece.insert(key.into(), value.clone());
            }
        }
        for joint in list_mut(&mut chain, "joints")? {
            let Some(valve_id) = joint.get("valve_id").cloned() else {
                continue; // a gap
            };
            let valve = take_by_id(&mut valves, &valve_id, "valve")?;
            let is_closed = valve
                .get("is_closed")
                .ok_or_else(|| invalid("valve is missing `is_closed`"))?;
            object_mut(joint, "joints")?.insert("is_closed".into(), is_closed.clone());
        }
        operators.push(serde_json::json!({ "wall_chain": chain }));
    }
    for (list_name, kind) in [
        ("source_spheres", "source_sphere"),
        ("thermostats", "thermostat"),
    ] {
        for entry in list(take_field(file, list_name)?, list_name)? {
            operators.push(serde_json::json!({ kind: entry }));
        }
    }
    if !segments.is_empty() || !valves.is_empty() {
        log::warn!(
            "dropped {} line segment(s) and {} valve(s) that belong to no wall chain",
            segments.len(),
            valves.len()
        );
    }
    object_mut(file, "the world file")?.insert("operators".into(), Value::Array(operators));
    Ok(())
}

/// The entries of the list `value` (named `name` in errors).
fn list(value: Value, name: &str) -> AppResult<Vec<Value>> {
    match value {
        Value::Array(entries) => Ok(entries),
        _ => Err(invalid(format!("`{name}` must be a list"))),
    }
}

/// The entries of the list field `key` of `entry`.
fn list_mut<'a>(entry: &'a mut Value, key: &str) -> AppResult<&'a mut Vec<Value>> {
    entry
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid(format!("`{key}` must be a list")))
}

fn object_mut<'a>(
    value: &'a mut Value,
    what: &str,
) -> AppResult<&'a mut serde_json::Map<String, Value>> {
    value
        .as_object_mut()
        .ok_or_else(|| invalid(format!("entries of {what} must be objects")))
}

/// The entries of the list `value`, keyed by their `id` (as JSON text, so
/// any id value works as a key).
fn objects_by_id(value: Value, name: &str) -> AppResult<Vec<(String, Value)>> {
    list(value, name)?
        .into_iter()
        .map(|entry| {
            let id = entry
                .get("id")
                .ok_or_else(|| invalid(format!("an entry of `{name}` has no `id`")))?;
            Ok((id.to_string(), entry))
        })
        .collect()
}

/// Remove and return the entry with `id`; each segment and valve belongs to
/// one chain.
fn take_by_id(entries: &mut Vec<(String, Value)>, id: &Value, what: &str) -> AppResult<Value> {
    let key = id.to_string();
    let index = entries
        .iter()
        .position(|(entry_id, _)| *entry_id == key)
        .ok_or_else(|| {
            invalid(format!(
                "a wall chain refers to {what} {key}, which is missing or belongs to another chain"
            ))
        })?;
    Ok(entries.swap_remove(index).1)
}

/// Parse a world file of any supported version into `T`, upgrading it first
/// if it is older than [`WORLD_FILE_VERSION`].
pub(crate) fn parse<T: DeserializeOwned>(json: &str) -> AppResult<Parsed<T>> {
    parse_with(json, MIGRATIONS, WORLD_FILE_VERSION)
}

/// [`parse`] with an explicit chain and current version, for tests.
pub(crate) fn parse_with<T: DeserializeOwned>(
    json: &str,
    migrations: &[Migration],
    current: u32,
) -> AppResult<Parsed<T>> {
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
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let parsed = deserialize_reporting(&mut deserializer)?;
        deserializer.end().map_err(|e| invalid(e.to_string()))?;
        return Ok(parsed);
    }
    let mut value: Value = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
    upgrade_with(&mut value, migrations, current)?;
    log::info!("upgraded world file from format v{version} to v{current}");
    deserialize_reporting(value)
}

/// Deserialize `T`, collecting the path of every field it skips.
fn deserialize_reporting<'de, T, D>(deserializer: D) -> AppResult<Parsed<T>>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    let mut ignored = IgnoredFields::default();
    let value = serde_ignored::deserialize(deserializer, |path| ignored.add(&path))
        .map_err(|e| invalid(e.to_string()))?;
    Ok((value, ignored.summary()))
}

/// The skipped fields, grouped by their path with list indices replaced by
/// `*`, so a field on every ball is one entry, not one per ball (100k balls
/// would otherwise make a multi-megabyte warning).
#[derive(Default)]
struct IgnoredFields {
    /// Pattern, path of its first occurrence and count, in order of first
    /// occurrence.
    entries: Vec<(String, String, usize)>,
    by_pattern: std::collections::HashMap<String, usize>,
}

impl IgnoredFields {
    fn add(&mut self, path: &serde_ignored::Path) {
        let pattern = field_path(path, true);
        match self.by_pattern.get(&pattern) {
            Some(&index) => self.entries[index].2 += 1,
            None => {
                self.by_pattern.insert(pattern.clone(), self.entries.len());
                self.entries.push((pattern, field_path(path, false), 1));
            }
        }
    }

    /// One line per pattern: the full path of a field that occurs once
    /// (`balls[3].charge`), the pattern and count otherwise
    /// (`balls[*].label (100000 entries)`).
    fn summary(self) -> Vec<String> {
        self.entries
            .into_iter()
            .map(|(pattern, first, count)| match count {
                1 => first,
                n => format!("{pattern} ({n} entries)"),
            })
            .collect()
    }
}

/// `balls[3].charge` for the field `charge` of the fourth ball, or
/// `balls[*].charge` with `wildcard`.
fn field_path(path: &serde_ignored::Path, wildcard: bool) -> String {
    use serde_ignored::Path;
    match path {
        Path::Root => String::new(),
        Path::Seq { parent, index } => {
            let parent = field_path(parent, wildcard);
            if wildcard {
                format!("{parent}[*]")
            } else {
                format!("{parent}[{index}]")
            }
        }
        Path::Map { parent, key } => match field_path(parent, wildcard) {
            parent if parent.is_empty() => key.clone(),
            parent => format!("{parent}.{key}"),
        },
        Path::Some { parent }
        | Path::NewtypeStruct { parent }
        | Path::NewtypeVariant { parent } => field_path(parent, wildcard),
    }
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
