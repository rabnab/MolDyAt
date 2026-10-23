// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The world file: versioned JSON (`serde_json`), human-readable, diffable
//! and easy to hand-edit or generate for test scenarios. `f32` values are
//! written as the shortest decimal that parses back to the same bits, so a
//! save/load round trip is exact.
//!
//! Every saved object has its own file type here (`BallData`, and the
//! operators in [`OperatorData`]: `WallChainData` with its pieces and valves,
//! `SourceSphereData`, `ThermostatData`), converted to and from the simulation types in
//! [`convert`](super::convert) (#270). So the format changes only when one
//! of these types changes, never as a side effect of refactoring a
//! simulation type, which no longer derives serde. The exceptions are the
//! two value types `Vec2D` (`{x, y}`) and `Rect` (`{x, y, w, h}`): their
//! layout is stable and they are written as they are. Runtime state
//! (neighbour lists, cell membership, tails, energy plot, `SimStats`) and the
//! view (window size, zoom, pan) are never written.

use crate::application::sim_config::{
    MAX_TIME_STEP, MIN_TIME_STEP, ResolvedSpecies, SimConfig, check_time_step,
};
use crate::error::{AppError, AppResult};
use crate::objects2d::operator::{
    DEFAULT_THERMOSTAT_CHAIN_LENGTH, MAX_EMISSION_SPEED, MAX_EMIT_PERIOD, MAX_TARGET_TEMPERATURE,
    MAX_THERMOSTAT_RADIUS, MIN_EMISSION_SPEED, MIN_EMIT_PERIOD, MIN_TARGET_TEMPERATURE,
    MIN_THERMOSTAT_RADIUS, THERMOSTAT_CHAIN_MAX, THERMOSTAT_CHAIN_MIN,
};
use crate::objects2d::{Rect, Vec2D};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Layout version written to every file. Bump it for a breaking format change
/// and add the matching migration (see [`migrate`](super::migrate)).
///
/// - v1: balls and source spheres name a size class by index (`size_idx`).
/// - v2 (#234): they name a species of the built-in table (`species`,
///   `ball_species`); interaction is Lennard-Jones on the centre distance.
/// - v3 (#274): one `operators` list instead of a list per kind; a wall
///   chain holds its pieces (the former `line_segments`) and its valves'
///   state (the former `valves`).
///
/// Since v2 the built-in species names and their mass, `σ` and `ε` are part
/// of the file format: a file stores only the name, the rest comes from
/// `SpeciesTable::builtin()` when it is loaded, unless the file's config has
/// its own `[[species]]` list (#237), which then defines every species the
/// file uses. Changing a built-in species is a breaking change: bump this
/// version and add a migration, or add a new species instead of changing an
/// existing one (`builtin_table_is_pinned`).
pub(crate) const WORLD_FILE_VERSION: u32 = 3;

/// Everything needed to rebuild a world. Energy plot data and simulation
/// statistics are deliberately not part of it.
///
/// A field added to this or any saved type after its format version gets
/// `#[serde(default)]` (or `#[serde(default = "…")]`) with a default that
/// reproduces the old behaviour, and `validate()` accepts that default, so
/// older files keep loading without a version bump. See the
/// [`persistence`](super) module doc for what is additive and what is breaking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorldFile {
    pub version: u32,
    /// World rectangle (`x`, `y` is the top-left corner, `h` is negative).
    pub world_rect: Rect,
    /// Time step of the run. It wins over `config.integration.time_step`,
    /// which must equal it when a config is present.
    pub time_step: f32,
    pub gravity: f32,
    /// The config the world ran with (#231). Saves written before it existed
    /// have none and load with the default config, which reproduces how
    /// those builds ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<SimConfig>,
    pub balls: Vec<BallData>,
    /// Every operator, grouped by kind in the order they were captured.
    /// Entries of a kind this build does not know are skipped and reported
    /// like unknown fields (#273, #274).
    #[serde(deserialize_with = "known_operators")]
    pub operators: Vec<OperatorData>,
}

/// An operator, written as an object whose only key is its kind:
/// `{"wall_chain": {...}}`. A new operator kind is a new variant here (and,
/// if it changes results, a version bump; see the
/// [`persistence`](super) module doc).
///
/// Not serde's internally tagged `{"kind": ..., ...}`: serde buffers such an
/// enum's fields, so the fields this build does not know would be skipped
/// without being reported (#273). With the kind as the key, the entry's
/// fields are read straight from the file, and an unknown kind is reported
/// as the skipped field `operators[i].<kind>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperatorData {
    WallChain(WallChainData),
    SourceSphere(SourceSphereData),
    Thermostat(ThermostatData),
}

/// One entry of `operators`; `None` for a kind this build does not know.
struct MaybeOperator(Option<OperatorData>);

impl<'de> Deserialize<'de> for MaybeOperator {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(OperatorVisitor)
    }
}

struct OperatorVisitor;

impl<'de> serde::de::Visitor<'de> for OperatorVisitor {
    type Value = MaybeOperator;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an operator: an object whose only key is its kind")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        use serde::de::{Error, IgnoredAny};
        let kind: String = map
            .next_key()?
            .ok_or_else(|| A::Error::custom("an operator needs its kind as its only key"))?;
        let operator = match kind.as_str() {
            "wall_chain" => Some(OperatorData::WallChain(map.next_value()?)),
            "source_sphere" => Some(OperatorData::SourceSphere(map.next_value()?)),
            "thermostat" => Some(OperatorData::Thermostat(map.next_value()?)),
            // Skipping the value is what reports it as an unknown field.
            _ => {
                map.next_value::<IgnoredAny>()?;
                None
            }
        };
        if map.next_key::<IgnoredAny>()?.is_some() {
            return Err(A::Error::custom(format!(
                "operator `{kind}` must be the only key of its entry"
            )));
        }
        Ok(MaybeOperator(operator))
    }
}

/// The `operators` list without the entries of unknown kinds.
fn known_operators<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<OperatorData>, D::Error> {
    let entries = Vec::<MaybeOperator>::deserialize(deserializer)?;
    Ok(entries.into_iter().filter_map(|entry| entry.0).collect())
}

/// A ball. `species` is the name of a species of the file's table (the
/// config's `species` list, or the built-in library); mass and `σ` follow from
/// it and are not stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BallData {
    pub id: usize,
    pub species: String,
    pub pos: Vec2D,
    pub v: Vec2D,
    pub pre_update_pos: Vec2D,
    pub accel: Vec2D,
}

/// A user-drawn wall (`WallChain`): its endpoints, the pieces (wall
/// segments) and the joints between them. `pieces.len() == joints.len() + 1`.
/// The chain owns its pieces and valves: they are saved here, not in lists
/// of their own (#274).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WallChainData {
    pub id: usize,
    pub start: Vec2D,
    pub end: Vec2D,
    pub pieces: Vec<ChainPieceData>,
    pub joints: Vec<JointData>,
}

/// One piece of a wall chain: the wall segment (`SimLineSegment`) with id
/// `id`, over the fixed fractions `t_start..t_end` of the chain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct ChainPieceData {
    pub id: usize,
    pub start: Vec2D,
    pub end: Vec2D,
    pub thickness: f32,
    pub t_start: f32,
    pub t_end: f32,
}

/// What sits between two pieces of a wall chain, tagged by `kind`.
///
/// Read through [`RawJoint`], not serde's internally tagged enum: serde
/// buffers such an enum's fields, so the fields this build does not know
/// would be skipped without being reported (#273).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum JointData {
    /// The valve `valve_id` (`ValveSegment`) and whether it is closed.
    Valve {
        valve_id: usize,
        is_closed: bool,
        t_start: f32,
        t_end: f32,
    },
    /// The hole a deleted valve leaves.
    Gap { t_start: f32, t_end: f32 },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum JointKind {
    Valve,
    Gap,
}

/// The fields of every [`JointData`] variant, read as a plain struct.
#[derive(Deserialize)]
struct RawJoint {
    kind: JointKind,
    valve_id: Option<usize>,
    is_closed: Option<bool>,
    t_start: f32,
    t_end: f32,
}

impl<'de> Deserialize<'de> for JointData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let RawJoint {
            kind,
            valve_id,
            is_closed,
            t_start,
            t_end,
        } = RawJoint::deserialize(deserializer)?;
        match kind {
            JointKind::Valve => Ok(Self::Valve {
                valve_id: valve_id.ok_or_else(|| D::Error::missing_field("valve_id"))?,
                is_closed: is_closed.ok_or_else(|| D::Error::missing_field("is_closed"))?,
                t_start,
                t_end,
            }),
            JointKind::Gap if valve_id.is_none() && is_closed.is_none() => {
                Ok(Self::Gap { t_start, t_end })
            }
            JointKind::Gap => Err(D::Error::custom(
                "a gap joint has no `valve_id` or `is_closed`",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SourceSphereData {
    pub id: usize,
    pub center: Vec2D,
    pub active: bool,
    pub emit_period: usize,
    /// Name of the species it emits.
    pub ball_species: String,
    pub emission_speed: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ThermostatData {
    pub id: usize,
    pub position: Vec2D,
    pub active: bool,
    pub target_temperature: f32,
    pub radius: f32,
    pub tau: f32,
    pub chain: Vec<f32>,
}

pub(super) fn invalid(message: impl Into<String>) -> AppError {
    AppError::CustomError(format!("invalid world file: {}", message.into()))
}

fn check_finite(values: &[f32], what: &str) -> AppResult {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(invalid(format!("{what} contains a non-finite number")))
    }
}

/// Largest id a file may use. Id counters continue at `id + 1`, so ids near
/// `usize::MAX` would overflow and hand out ids that are already taken.
pub(crate) const MAX_ID: usize = u32::MAX as usize;

fn check_unique_ids(ids: impl Iterator<Item = usize>, what: &str) -> AppResult<HashSet<usize>> {
    let mut seen = HashSet::new();
    for id in ids {
        if id > MAX_ID {
            return Err(invalid(format!("{what} id {id} is above {MAX_ID}")));
        }
        if !seen.insert(id) {
            return Err(invalid(format!("duplicate {what} id {id}")));
        }
    }
    Ok(seen)
}

impl WorldFile {
    /// Check everything the loader relies on, so a bad file is rejected before
    /// the running world is touched. Returns the file's species table and
    /// pair overrides, resolved once, for the world's constructor.
    pub(crate) fn validate(&self) -> AppResult<ResolvedSpecies> {
        if self.version != WORLD_FILE_VERSION {
            return Err(invalid(format!(
                "version {} is not supported (expected {WORLD_FILE_VERSION})",
                self.version
            )));
        }
        let rect = self.world_rect;
        check_finite(&[rect.x, rect.y, rect.w, rect.h], "world_rect")?;
        // The world rectangle is stored top-left first, so `h` is negative.
        if rect.w <= 0.0 || rect.h >= 0.0 {
            return Err(invalid("world_rect needs w > 0 and h < 0"));
        }
        check_time_step(self.time_step).map_err(|_| {
            invalid(format!(
                "time_step must be within {MIN_TIME_STEP}..={MAX_TIME_STEP}, got {}",
                self.time_step
            ))
        })?;
        check_finite(&[self.gravity], "gravity")?;
        // Saved balls and source spheres name species of the save's table:
        // the config's, or the built-in library for files without a config.
        let resolved = match &self.config {
            Some(config) => config.validated_species()?,
            None => ResolvedSpecies::builtin(),
        };
        let species = &*resolved.table;
        if let Some(config) = &self.config
            && config.integration.time_step != self.time_step
        {
            return Err(invalid(format!(
                "config.integration.time_step {} differs from time_step {}",
                config.integration.time_step, self.time_step
            )));
        }

        check_unique_ids(self.balls.iter().map(|b| b.id), "ball")?;
        for ball in &self.balls {
            if species.by_name(&ball.species).is_none() {
                return Err(invalid(format!(
                    "ball {} has unknown species {:?}",
                    ball.id, ball.species
                )));
            }
            check_finite(
                &[
                    ball.pos.x,
                    ball.pos.y,
                    ball.v.x,
                    ball.v.y,
                    ball.pre_update_pos.x,
                    ball.pre_update_pos.y,
                    ball.accel.x,
                    ball.accel.y,
                ],
                &format!("ball {}", ball.id),
            )?;
            // A ball outside the world gets no cell and would silently take
            // no part in the simulation, so reject it instead.
            let inside_x = ball.pos.x >= rect.x && ball.pos.x < rect.x + rect.w;
            let inside_y = ball.pos.y <= rect.y && ball.pos.y > rect.y + rect.h;
            if !(inside_x && inside_y) {
                return Err(invalid(format!(
                    "ball {} lies outside world_rect at ({}, {})",
                    ball.id, ball.pos.x, ball.pos.y
                )));
            }
        }

        check_unique_ids(self.wall_chains().map(|c| c.id), "wall chain")?;
        check_unique_ids(
            self.wall_chains()
                .flat_map(|c| c.pieces.iter().map(|p| p.id)),
            "wall piece",
        )?;
        check_unique_ids(
            self.wall_chains().flat_map(|c| {
                c.joints.iter().filter_map(|joint| match *joint {
                    JointData::Valve { valve_id, .. } => Some(valve_id),
                    JointData::Gap { .. } => None,
                })
            }),
            "valve",
        )?;
        for chain in self.wall_chains() {
            validate_chain(chain)?;
        }

        check_unique_ids(self.source_spheres().map(|s| s.id), "source sphere")?;
        for sphere in self.source_spheres() {
            check_finite(
                &[sphere.center.x, sphere.center.y, sphere.emission_speed],
                &format!("source sphere {}", sphere.id),
            )?;
            if !(MIN_EMIT_PERIOD..=MAX_EMIT_PERIOD).contains(&sphere.emit_period) {
                return Err(invalid(format!(
                    "source sphere {} has emit_period {} (allowed {MIN_EMIT_PERIOD}..={MAX_EMIT_PERIOD})",
                    sphere.id, sphere.emit_period
                )));
            }
            if !(MIN_EMISSION_SPEED..=MAX_EMISSION_SPEED).contains(&sphere.emission_speed) {
                return Err(invalid(format!(
                    "source sphere {} has emission_speed {} (allowed {MIN_EMISSION_SPEED}..={MAX_EMISSION_SPEED})",
                    sphere.id, sphere.emission_speed
                )));
            }
            if species.by_name(&sphere.ball_species).is_none() {
                return Err(invalid(format!(
                    "source sphere {} has unknown species {:?}",
                    sphere.id, sphere.ball_species
                )));
            }
        }

        check_unique_ids(self.thermostats().map(|t| t.id), "thermostat")?;
        for thermostat in self.thermostats() {
            let what = format!("thermostat {}", thermostat.id);
            check_finite(
                &[
                    thermostat.position.x,
                    thermostat.position.y,
                    thermostat.target_temperature,
                    thermostat.radius,
                    thermostat.tau,
                ],
                &what,
            )?;
            check_finite(&thermostat.chain, &what)?;
            let radius_ok =
                (MIN_THERMOSTAT_RADIUS..=MAX_THERMOSTAT_RADIUS).contains(&thermostat.radius);
            let target_ok = (MIN_TARGET_TEMPERATURE..=MAX_TARGET_TEMPERATURE)
                .contains(&thermostat.target_temperature);
            if !radius_ok || !target_ok || thermostat.tau <= 0.0 {
                return Err(invalid(format!(
                    "{what} has radius, target_temperature or tau out of range"
                )));
            }
            let chain_ok = thermostat.chain.len() == DEFAULT_THERMOSTAT_CHAIN_LENGTH
                && thermostat
                    .chain
                    .iter()
                    .all(|c| (THERMOSTAT_CHAIN_MIN..=THERMOSTAT_CHAIN_MAX).contains(c));
            if !chain_ok {
                return Err(invalid(format!(
                    "{what} needs {DEFAULT_THERMOSTAT_CHAIN_LENGTH} chain values within {THERMOSTAT_CHAIN_MIN}..={THERMOSTAT_CHAIN_MAX}"
                )));
            }
        }
        Ok(resolved)
    }

    /// The wall chains among the operators, in file order.
    pub(crate) fn wall_chains(&self) -> impl Iterator<Item = &WallChainData> {
        self.operators.iter().filter_map(|operator| match operator {
            OperatorData::WallChain(chain) => Some(chain),
            _ => None,
        })
    }

    /// The source spheres among the operators, in file order.
    pub(crate) fn source_spheres(&self) -> impl Iterator<Item = &SourceSphereData> {
        self.operators.iter().filter_map(|operator| match operator {
            OperatorData::SourceSphere(sphere) => Some(sphere),
            _ => None,
        })
    }

    /// The thermostats among the operators, in file order.
    pub(crate) fn thermostats(&self) -> impl Iterator<Item = &ThermostatData> {
        self.operators.iter().filter_map(|operator| match operator {
            OperatorData::Thermostat(thermostat) => Some(thermostat),
            _ => None,
        })
    }

    /// The config to run this world with: the saved one, or the default for
    /// files without one, with the file's time step.
    pub fn sim_config(&self) -> SimConfig {
        let mut config = self.config.clone().unwrap_or_default();
        config.integration.time_step = self.time_step;
        config
    }

    pub(crate) fn to_json(&self) -> AppResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| AppError::CustomError(format!("cannot serialize world: {e}")))
    }

    /// Parse a world file of any supported version, upgrade it to the
    /// current one (see [`migrate`](super::migrate)) and validate it.
    /// Fields this build does not know are skipped; see
    /// [`Self::from_json_reporting`] for their names.
    #[cfg(test)]
    pub(crate) fn from_json(json: &str) -> AppResult<Self> {
        Self::from_json_reporting(json).map(|(file, _)| file)
    }

    /// [`Self::from_json`], also returning the paths of the fields outside
    /// `config` that this build does not know and skipped
    /// (`balls[3].charge`, `pistons`), in file order (#273).
    pub(crate) fn from_json_reporting(json: &str) -> AppResult<(Self, Vec<String>)> {
        let (file, ignored): (Self, _) = super::migrate::parse(json).map_err(newer_build_hint)?;
        file.validate()?;
        Ok((file, ignored))
    }
}

/// Only `config` rejects unknown keys (`SimConfig` uses
/// `deny_unknown_fields`; unknown fields elsewhere are skipped with a
/// warning, #273), and a key it does not know was most likely added by a
/// newer build that still writes the same format version. Say so.
fn newer_build_hint(error: AppError) -> AppError {
    match error {
        AppError::CustomError(message) if message.contains("unknown field") => {
            AppError::CustomError(format!(
                "{message} (the save was probably written by a newer MolDyAt build)"
            ))
        }
        other => other,
    }
}

fn validate_chain(chain: &WallChainData) -> AppResult {
    let what = format!("wall chain {}", chain.id);
    check_finite(
        &[chain.start.x, chain.start.y, chain.end.x, chain.end.y],
        &what,
    )?;
    if chain.pieces.is_empty() || chain.pieces.len() != chain.joints.len() + 1 {
        return Err(invalid(format!(
            "{what} needs exactly one more piece than joints"
        )));
    }
    for piece in &chain.pieces {
        check_finite(
            &[
                piece.start.x,
                piece.start.y,
                piece.end.x,
                piece.end.y,
                piece.thickness,
                piece.t_start,
                piece.t_end,
            ],
            &format!("{what}, piece {}", piece.id),
        )?;
        if piece.thickness <= 0.0 {
            return Err(invalid(format!(
                "{what}, piece {} needs a positive thickness",
                piece.id
            )));
        }
    }
    for joint in &chain.joints {
        let (JointData::Valve { t_start, t_end, .. } | JointData::Gap { t_start, t_end }) = *joint;
        check_finite(&[t_start, t_end], &what)?;
    }
    Ok(())
}
