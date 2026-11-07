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
use crate::application::units::{UnitScale, UnitSystem};
use crate::error::AppResult;
use crate::objects2d::ball_impl::species::{CH4_HCB_NAME, SIZE_CLASS_NAMES};
use crate::objects2d::line_segment::{MAX_THICKNESS, MIN_THICKNESS};
use crate::objects2d::operator::{
    MAX_EMISSION_SPEED, MAX_THERMOSTAT_RADIUS, MIN_EMISSION_SPEED, MIN_THERMOSTAT_RADIUS,
    THERMOSTAT_CHAIN_MAX, THERMOSTAT_CHAIN_MIN,
};
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
    Migration {
        from: 3,
        apply: migrate_v3_to_v4,
    },
    Migration {
        from: 4,
        apply: migrate_v4_to_v5,
    },
    Migration {
        from: 5,
        apply: migrate_v5_to_v6,
    },
    Migration {
        from: 6,
        apply: migrate_v6_to_v7,
    },
    Migration {
        from: 7,
        apply: migrate_v7_to_v8,
    },
    Migration {
        from: 8,
        apply: migrate_v8_to_v9,
    },
];

/// The built-in species table of format v2 to v8 in literature units, and
/// the units of v6 to v8 (Ar), frozen: what [`migrate_v8_to_v9`] writes.
const V8_SPECIES: &str = include_str!("v8_species.json");

/// v8 → v9 (#426): every file states its `units` (v6 to v8: Ar) and has a
/// `config` with the whole species table. A file without a config gets the
/// one it ran with (hard walls, the defaults otherwise), a file without a
/// `[[species]]` list the built-in table of v8.
fn migrate_v8_to_v9(file: &mut Value) -> AppResult {
    let frozen: Value = serde_json::from_str(V8_SPECIES).map_err(|e| invalid(e.to_string()))?;
    let time_step = file
        .get("time_step")
        .cloned()
        .ok_or_else(|| invalid("missing field `time_step`"))?;
    let object = object_mut(file, "the world file")?;
    object.insert("units".into(), frozen["units"].clone());
    let config = object.entry("config").or_insert(Value::Null);
    if config.is_null() {
        *config = serde_json::json!({
            "walls": { "model": "hard" },
            "integration": { "time_step": time_step },
        });
    }
    let config = object_mut(config, "`config`")?;
    let own_table = config
        .get("species")
        .and_then(Value::as_array)
        .is_some_and(|list| !list.is_empty());
    if !own_table {
        config.insert("species".into(), frozen["species"].clone());
    }
    Ok(())
}

/// v7 → v8 (#348): the orientation of balls and the moment of inertia of
/// `[[species]]` entries. Older files have none, which the defaults fill
/// in; the version only changes so that older builds refuse rotating
/// worlds.
fn migrate_v7_to_v8(_file: &mut Value) -> AppResult {
    Ok(())
}

/// v6 → v7 (#409): the built-in `CH4` became TraPPE-UA methane; the former
/// one (Hirschfelder, Curtiss & Bird) stays as `CH4_HCB`. A file that uses
/// the built-in table (no `[[species]]` list in its config) names its
/// `CH4` balls, source spheres, pair overrides and wall overrides
/// `CH4_HCB`, so it keeps its physics. A file with its own list keeps its
/// names: they are that list's species.
fn migrate_v6_to_v7(file: &mut Value) -> AppResult {
    let config = file.get("config").filter(|c| !c.is_null());
    let own_table = config
        .and_then(|c| c.get("species"))
        .and_then(Value::as_array)
        .is_some_and(|list| !list.is_empty());
    if own_table {
        return Ok(());
    }
    let rename = |value: &mut Value| {
        if value.as_str() == Some("CH4") {
            *value = Value::from(CH4_HCB_NAME);
        }
    };
    for ball in list_mut(file, "balls")? {
        if let Some(name) = ball.get_mut("species") {
            rename(name);
        }
    }
    for entry in list_mut(file, "operators")? {
        if let Some(name) = entry
            .get_mut("source_sphere")
            .and_then(|s| s.get_mut("ball_species"))
        {
            rename(name);
        }
    }
    if let Some(config) = file.get_mut("config").filter(|c| !c.is_null()) {
        if let Some(overrides) = config
            .get_mut("potential")
            .and_then(|p| p.get_mut("pair_override"))
            .and_then(Value::as_array_mut)
        {
            for o in overrides {
                for key in ["a", "b"] {
                    if let Some(name) = o.get_mut(key) {
                        rename(name);
                    }
                }
            }
        }
        if let Some(overrides) = config
            .get_mut("walls")
            .and_then(|w| w.get_mut("override"))
            .and_then(Value::as_array_mut)
        {
            for o in overrides {
                if let Some(name) = o.get_mut("species") {
                    rename(name);
                }
            }
        }
    }
    Ok(())
}

/// v5 → v6 (#405): Ar replaced N₂ as the unit species. Every physical
/// quantity is rescaled from N₂ to Ar units, so the file holds the same
/// physical state: lengths (world, balls, walls, operators, wall and field
/// parameters, pair `σ`), speeds, accelerations (gravity), energies and
/// temperatures, and the force cap (`max_force`, `core_stiffness`), whose
/// defaults keep their physical value too. Numerical settings keep their
/// numbers: the time step (and so emission periods in steps) and the cutoff
/// and skin factors. A config's `[[species]]` list is in Å, K and u already.
/// Wall thicknesses and emission speeds are clamped to their ranges.
///
/// Soft walls left at their default `ε_w` (a config without `walls`, or
/// without its `epsilon`) ran with `ε_w = 1 ε_N2`: the migrated config states
/// that value in Ar units, since the default is now `1 ε_Ar`. The config's
/// `world.scale` keeps its number: it only records how the world was created
/// (the save keeps its own `world_rect`), so a new world made from a
/// migrated config is `1 / 1.0861` of the old size in physical units.
fn migrate_v5_to_v6(file: &mut Value) -> AppResult {
    if let Some(config) = file.get_mut("config").filter(|c| !c.is_null()) {
        let walls = object_mut(config, "`config`")?
            .entry("walls")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let hard = walls.get("model").and_then(Value::as_str) == Some("hard");
        if !hard && walls.get("epsilon").is_none() {
            // The v5 default ε_w, in N₂ units; scaled below.
            object_mut(walls, "`config.walls`")?.insert("epsilon".into(), Value::from(1.0));
        }
    }
    rescale(file, &UnitSystem::N2.scale_to(&UnitSystem::AR))
}

/// Convert every reduced value of `file` by `s`, so it holds the same
/// physical state in other units (see [`migrate_v5_to_v6`]; a v9 file in
/// other units than `UnitSystem::REDUCED`, #426). Angular velocities and
/// accelerations scale with time only.
fn rescale(file: &mut Value, s: &UnitScale) -> AppResult {
    scale_rect(file, "world_rect", s.length)?;
    scale_number(file, "gravity", s.acceleration())?;
    for ball in list_mut(file, "balls")? {
        for key in ["pos", "pre_update_pos"] {
            scale_vec(ball, key, s.length)?;
        }
        scale_vec(ball, "v", s.speed())?;
        scale_vec(ball, "accel", s.acceleration())?;
        scale_optional(ball, "omega", 1.0 / s.time)?;
        scale_optional(ball, "alpha", 1.0 / (s.time * s.time))?;
    }
    for entry in list_mut(file, "operators")? {
        let object = object_mut(entry, "operators")?;
        if let Some(chain) = object.get_mut("wall_chain") {
            scale_vec(chain, "start", s.length)?;
            scale_vec(chain, "end", s.length)?;
            for piece in list_mut(chain, "pieces")? {
                scale_vec(piece, "start", s.length)?;
                scale_vec(piece, "end", s.length)?;
                scale_clamped(piece, "thickness", s.length, MIN_THICKNESS, MAX_THICKNESS)?;
            }
        } else if let Some(sphere) = object.get_mut("source_sphere") {
            scale_vec(sphere, "center", s.length)?;
            scale_clamped(
                sphere,
                "emission_speed",
                s.speed(),
                MIN_EMISSION_SPEED,
                MAX_EMISSION_SPEED,
            )?;
        } else if let Some(thermostat) = object.get_mut("thermostat") {
            scale_vec(thermostat, "position", s.length)?;
            scale_clamped(
                thermostat,
                "radius",
                s.length,
                MIN_THERMOSTAT_RADIUS,
                MAX_THERMOSTAT_RADIUS,
            )?;
            scale_number(thermostat, "target_temperature", s.energy)?;
            scale_number(thermostat, "tau", s.time)?;
            for xi in list_mut(thermostat, "chain")? {
                scale_value(xi, 1.0 / s.time, "thermostat chain")?;
                let x = xi.as_f64().unwrap_or_default();
                *xi = Value::from(x.clamp(THERMOSTAT_CHAIN_MIN, THERMOSTAT_CHAIN_MAX));
            }
        }
        // Unknown kinds are skipped on load anyway.
    }
    if let Some(config) = file.get_mut("config").filter(|c| !c.is_null()) {
        scale_config(config, s)?;
    }
    Ok(())
}

/// The reduced keys of a saved config (see [`rescale`]); `[[species]]` is
/// in literature units already.
fn scale_config(config: &mut Value, s: &UnitScale) -> AppResult {
    let config = object_mut(config, "`config`")?;
    if let Some(potential) = config.get_mut("potential") {
        scale_optional(potential, "max_force", s.energy / s.length)?;
        scale_optional(potential, "core_stiffness", s.stiffness())?;
    }
    if let Some(overrides) = config
        .get_mut("potential")
        .and_then(|p| p.get_mut("pair_override"))
    {
        for o in list_mut_value(overrides, "pair_override")? {
            scale_optional(o, "sigma", s.length)?;
            scale_optional(o, "epsilon", s.energy)?;
        }
    }
    if let Some(walls) = config.get_mut("walls") {
        scale_optional(walls, "sigma", s.length)?;
        scale_optional(walls, "epsilon", s.energy)?;
        if let Some(overrides) = walls.get_mut("override") {
            for o in list_mut_value(overrides, "walls.override")? {
                scale_optional(o, "sigma", s.length)?;
                scale_optional(o, "epsilon", s.energy)?;
            }
        }
    }
    if let Some(fields) = config.get_mut("fields") {
        for field in list_mut_value(fields, "fields")? {
            let center = field
                .get_mut("center")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| invalid("`fields[].center` must be a list"))?;
            for c in center {
                scale_value(c, s.length, "fields[].center")?;
            }
            scale_number(field, "stiffness", s.stiffness())?;
        }
    }
    Ok(())
}

/// Multiply the number `value` by `factor`.
fn scale_value(value: &mut Value, factor: f64, what: &str) -> AppResult {
    let x = value
        .as_f64()
        .ok_or_else(|| invalid(format!("`{what}` must be a number")))?;
    *value = Value::from(x * factor);
    Ok(())
}

/// Multiply the number at `key` of `entry` by `factor`.
fn scale_number(entry: &mut Value, key: &str, factor: f64) -> AppResult {
    let value = entry
        .get_mut(key)
        .ok_or_else(|| invalid(format!("missing field `{key}`")))?;
    scale_value(value, factor, key)
}

/// [`scale_number`], if `key` is present.
fn scale_optional(entry: &mut Value, key: &str, factor: f64) -> AppResult {
    match entry.get_mut(key) {
        Some(value) => scale_value(value, factor, key),
        None => Ok(()),
    }
}

/// [`scale_number`], clamped to `min..=max` (logged).
fn scale_clamped(entry: &mut Value, key: &str, factor: f64, min: f32, max: f32) -> AppResult {
    scale_number(entry, key, factor)?;
    let value = entry.get(key).and_then(Value::as_f64).unwrap_or_default();
    let clamped = value.clamp(f64::from(min), f64::from(max));
    if clamped != value {
        log::warn!("`{key}` {value} clamped to {clamped} while converting units");
        entry[key] = Value::from(clamped);
    }
    Ok(())
}

/// Multiply both coordinates of the `{x, y}` at `key` by `factor`.
fn scale_vec(entry: &mut Value, key: &str, factor: f64) -> AppResult {
    let vec = entry
        .get_mut(key)
        .ok_or_else(|| invalid(format!("missing field `{key}`")))?;
    for axis in ["x", "y"] {
        scale_number(vec, axis, factor)?;
    }
    Ok(())
}

/// Multiply every field of the `{x, y, w, h}` at `key` by `factor`.
fn scale_rect(entry: &mut Value, key: &str, factor: f64) -> AppResult {
    let rect = entry
        .get_mut(key)
        .ok_or_else(|| invalid(format!("missing field `{key}`")))?;
    for field in ["x", "y", "w", "h"] {
        scale_number(rect, field, factor)?;
    }
    Ok(())
}

/// The entries of the list `value`.
fn list_mut_value<'a>(value: &'a mut Value, name: &str) -> AppResult<&'a mut Vec<Value>> {
    value
        .as_array_mut()
        .ok_or_else(|| invalid(format!("`{name}` must be a list")))
}

/// v4 → v5 (#347): `config.fields` (external fields besides gravity).
/// Older files have none, which the default fills in; the version only
/// changes so that older builds refuse files with fields.
fn migrate_v4_to_v5(_file: &mut Value) -> AppResult {
    Ok(())
}

/// v3 → v4 (#280): soft walls became the default. A save with a `config`
/// ran with hard walls, so it gets `config.walls.model = "hard"`; a save
/// without one gets hard walls with its config in [`migrate_v8_to_v9`].
fn migrate_v3_to_v4(file: &mut Value) -> AppResult {
    let Some(config) = file.get_mut("config") else {
        return Ok(());
    };
    if config.is_null() {
        return Ok(());
    }
    let config = object_mut(config, "`config`")?;
    if config.contains_key("walls") {
        return Err(invalid("a v3 `config` cannot have `walls`"));
    }
    config.insert("walls".into(), serde_json::json!({ "model": "hard" }));
    Ok(())
}

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
        units: Option<UnitSystem>,
    }
    let header: Header = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
    let version = check_version(header.version.as_ref(), current)?;
    if version == current && header.units.is_none_or(|u| u == UnitSystem::REDUCED) {
        // Fast path: building a `Value` tree first is many times slower for
        // large saves.
        let mut deserializer = serde_json::Deserializer::from_str(json);
        let parsed = deserialize_reporting(&mut deserializer)?;
        deserializer.end().map_err(|e| invalid(e.to_string()))?;
        return Ok(parsed);
    }
    let mut value: Value = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
    upgrade_with(&mut value, migrations, current)?;
    if version < current {
        log::info!("upgraded world file from format v{version} to v{current}");
    }
    to_reduced_units(&mut value)?;
    deserialize_reporting(value)
}

/// Convert a file written in other units than `UnitSystem::REDUCED` to it
/// (#426). A file without `units` is left alone: reading it reports the
/// missing field.
fn to_reduced_units(file: &mut Value) -> AppResult {
    let Some(units) = file.get("units") else {
        return Ok(());
    };
    let units: UnitSystem =
        serde_json::from_value(units.clone()).map_err(|e| invalid(format!("`units`: {e}")))?;
    if units == UnitSystem::REDUCED {
        return Ok(());
    }
    let UnitSystem {
        sigma_angstrom,
        epsilon_kelvin,
        mass_u,
    } = units;
    if ![sigma_angstrom, epsilon_kelvin, mass_u]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
    {
        return Err(invalid(format!("`units` must be positive, got {units:?}")));
    }
    rescale(file, &units.scale_to(&UnitSystem::REDUCED))?;
    file["units"] = serde_json::json!(UnitSystem::REDUCED);
    log::info!(
        "converted world file from {units:?} to {:?}",
        UnitSystem::REDUCED
    );
    Ok(())
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
