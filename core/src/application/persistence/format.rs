// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The world file: versioned JSON (`serde_json`), human-readable, diffable
//! and easy to hand-edit or generate for test scenarios. `f32` values are
//! written as the shortest decimal that parses back to the same bits, so a
//! save/load round trip is exact.
//!
//! Every saved object has its own file type here (`BallData`,
//! `LineSegmentData`, `ValveData`, `WallChainData`, `SourceSphereData`,
//! `ThermostatData`), converted to and from the simulation types in
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
///
/// Since v2 the built-in species names and their mass, `σ` and `ε` are part
/// of the file format: a file stores only the name, the rest comes from
/// `SpeciesTable::builtin()` when it is loaded, unless the file's config has
/// its own `[[species]]` list (#237), which then defines every species the
/// file uses. Changing a built-in species is a breaking change: bump this
/// version and add a migration, or add a new species instead of changing an
/// existing one (`builtin_table_is_pinned`).
pub const WORLD_FILE_VERSION: u32 = 2;

/// Everything needed to rebuild a world. Energy plot data and simulation
/// statistics are deliberately not part of it.
///
/// A field added to this or any saved type after its format version gets
/// `#[serde(default)]` (or `#[serde(default = "…")]`) with a default that
/// reproduces the old behaviour, and `validate()` accepts that default, so
/// older files keep loading without a version bump. See the
/// [`persistence`](super) module doc for what is additive and what is breaking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldFile {
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
    pub line_segments: Vec<LineSegmentData>,
    pub valves: Vec<ValveData>,
    pub wall_chains: Vec<WallChainData>,
    pub source_spheres: Vec<SourceSphereData>,
    pub thermostats: Vec<ThermostatData>,
}

/// A ball. `species` is the name of a species of the file's table (the
/// config's `species` list, or the built-in library); mass and `σ` follow from
/// it and are not stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BallData {
    pub id: usize,
    pub species: String,
    pub pos: Vec2D,
    pub v: Vec2D,
    pub pre_update_pos: Vec2D,
    pub accel: Vec2D,
}

/// A wall piece (`SimLineSegment`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineSegmentData {
    pub id: usize,
    pub start: Vec2D,
    pub end: Vec2D,
    pub thickness: f32,
}

/// A valve's state (`ValveSegment`); where it sits is in its wall chain's
/// joints.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ValveData {
    pub id: usize,
    pub is_closed: bool,
}

/// A user-drawn wall (`WallChain`): its endpoints, the pieces (wall
/// segments) and the joints between them. `pieces.len() == joints.len() + 1`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WallChainData {
    pub id: usize,
    pub start: Vec2D,
    pub end: Vec2D,
    pub pieces: Vec<ChainPieceData>,
    pub joints: Vec<JointData>,
}

/// One piece of a wall chain: the line segment `line_id`, over the fixed
/// fractions `t_start..t_end` of the chain.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChainPieceData {
    pub line_id: usize,
    pub t_start: f32,
    pub t_end: f32,
}

/// What sits between two pieces of a wall chain, tagged by `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JointData {
    /// The valve `valve_id`.
    Valve {
        valve_id: usize,
        t_start: f32,
        t_end: f32,
    },
    /// The hole a deleted valve leaves.
    Gap { t_start: f32, t_end: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSphereData {
    pub id: usize,
    pub center: Vec2D,
    pub active: bool,
    pub emit_period: usize,
    /// Name of the species it emits.
    pub ball_species: String,
    pub emission_speed: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermostatData {
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
pub const MAX_ID: usize = u32::MAX as usize;

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
    pub fn validate(&self) -> AppResult<ResolvedSpecies> {
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

        let line_ids = check_unique_ids(self.line_segments.iter().map(|s| s.id), "line segment")?;
        for segment in &self.line_segments {
            check_finite(
                &[
                    segment.start.x,
                    segment.start.y,
                    segment.end.x,
                    segment.end.y,
                    segment.thickness,
                ],
                &format!("line segment {}", segment.id),
            )?;
            if segment.thickness <= 0.0 {
                return Err(invalid(format!(
                    "line segment {} needs a positive thickness",
                    segment.id
                )));
            }
        }
        let valve_ids = check_unique_ids(self.valves.iter().map(|v| v.id), "valve")?;
        check_unique_ids(self.wall_chains.iter().map(|c| c.id), "wall chain")?;
        for chain in &self.wall_chains {
            self.validate_chain(chain, &line_ids, &valve_ids)?;
        }

        check_unique_ids(self.source_spheres.iter().map(|s| s.id), "source sphere")?;
        for sphere in &self.source_spheres {
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

        check_unique_ids(self.thermostats.iter().map(|t| t.id), "thermostat")?;
        for thermostat in &self.thermostats {
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

    fn validate_chain(
        &self,
        chain: &WallChainData,
        line_ids: &HashSet<usize>,
        valve_ids: &HashSet<usize>,
    ) -> AppResult {
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
            check_finite(&[piece.t_start, piece.t_end], &what)?;
            if !line_ids.contains(&piece.line_id) {
                return Err(invalid(format!(
                    "{what} refers to missing line segment {}",
                    piece.line_id
                )));
            }
        }
        for joint in &chain.joints {
            match *joint {
                JointData::Valve {
                    valve_id,
                    t_start,
                    t_end,
                } => {
                    check_finite(&[t_start, t_end], &what)?;
                    if !valve_ids.contains(&valve_id) {
                        return Err(invalid(format!(
                            "{what} refers to missing valve {valve_id}"
                        )));
                    }
                }
                JointData::Gap { t_start, t_end } => check_finite(&[t_start, t_end], &what)?,
            }
        }
        Ok(())
    }

    /// The config to run this world with: the saved one, or the default for
    /// files without one, with the file's time step.
    pub fn sim_config(&self) -> SimConfig {
        let mut config = self.config.clone().unwrap_or_default();
        config.integration.time_step = self.time_step;
        config
    }

    pub fn to_json(&self) -> AppResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| AppError::CustomError(format!("cannot serialize world: {e}")))
    }

    /// Parse a world file of any supported version, upgrade it to the
    /// current one (see [`migrate`](super::migrate)) and validate it.
    pub fn from_json(json: &str) -> AppResult<Self> {
        let file: Self = super::migrate::parse(json).map_err(newer_build_hint)?;
        file.validate()?;
        Ok(file)
    }
}

/// Only `config` rejects unknown keys (`SimConfig` uses
/// `deny_unknown_fields`), and a key it does not know was most likely added
/// by a newer build that still writes the same format version. Say so.
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
