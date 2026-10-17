// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The world file: versioned JSON (`serde_json`), human-readable, diffable
//! and easy to hand-edit or generate for test scenarios. `f32` values are
//! written as the shortest decimal that parses back to the same bits, so a
//! save/load round trip is exact.
//!
//! Types that are saved as they are (`Vec2D`, `Rect`, `SimLineSegment`,
//! `ValveSegment`, `WallChain`) derive serde themselves, so the format follows
//! them when they change. Only where the file differs from the simulation type
//! (balls drop radius and mass; source spheres and thermostats drop runtime
//! state) is there a separate type here. Runtime state (neighbour lists, cell
//! membership, tails, energy plot, `SimStats`) and the view (window size,
//! zoom, pan) are never written.

use crate::error::{AppError, AppResult};
use crate::objects2d::ball_impl::NUM_BALL_SIZES;
use crate::objects2d::operator::{
    DEFAULT_THERMOSTAT_CHAIN_LENGTH, MAX_EMISSION_SPEED, MAX_EMIT_PERIOD, MAX_TARGET_TEMPERATURE,
    MAX_THERMOSTAT_RADIUS, MIN_EMISSION_SPEED, MIN_EMIT_PERIOD, MIN_TARGET_TEMPERATURE,
    MIN_THERMOSTAT_RADIUS, THERMOSTAT_CHAIN_MAX, THERMOSTAT_CHAIN_MIN,
};
use crate::objects2d::wall_chain::{Joint, WallChain};
use crate::objects2d::{Rect, SimLineSegment, ValveSegment, Vec2D};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Layout version written to every file. Bump it for a breaking format change
/// and add the matching migration (see [`migrate`](super::migrate)).
pub const WORLD_FILE_VERSION: u32 = 1;

/// Everything needed to rebuild a world. Energy plot data and simulation
/// statistics are deliberately not part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldFile {
    pub version: u32,
    /// World rectangle (`x`, `y` is the top-left corner, `h` is negative).
    pub world_rect: Rect,
    pub time_step: f32,
    pub gravity: f32,
    pub balls: Vec<BallData>,
    pub line_segments: Vec<SimLineSegment>,
    pub valves: Vec<ValveSegment>,
    pub wall_chains: Vec<WallChain>,
    pub source_spheres: Vec<SourceSphereData>,
    pub thermostats: Vec<ThermostatData>,
}

/// A ball. The radius and mass follow from `size_idx` and are not stored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BallData {
    pub id: usize,
    pub size_idx: usize,
    pub pos: Vec2D,
    pub v: Vec2D,
    pub pre_update_pos: Vec2D,
    pub accel: Vec2D,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SourceSphereData {
    pub id: usize,
    pub center: Vec2D,
    pub active: bool,
    pub emit_period: usize,
    pub ball_size_idx: usize,
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
    /// the running world is touched.
    pub fn validate(&self) -> AppResult {
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
        if !(self.time_step.is_finite() && self.time_step > 0.0) {
            return Err(invalid("time_step must be finite and positive"));
        }
        check_finite(&[self.gravity], "gravity")?;

        check_unique_ids(self.balls.iter().map(|b| b.id), "ball")?;
        for ball in &self.balls {
            if ball.size_idx >= NUM_BALL_SIZES {
                return Err(invalid(format!(
                    "ball {} has size_idx {} (must be below {NUM_BALL_SIZES})",
                    ball.id, ball.size_idx
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
            if sphere.ball_size_idx >= NUM_BALL_SIZES {
                return Err(invalid(format!(
                    "source sphere {} has ball_size_idx {}",
                    sphere.id, sphere.ball_size_idx
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
        Ok(())
    }

    fn validate_chain(
        &self,
        chain: &WallChain,
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
                Joint::Valve {
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
                Joint::Gap { t_start, t_end } => check_finite(&[t_start, t_end], &what)?,
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> AppResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| AppError::CustomError(format!("cannot serialize world: {e}")))
    }

    /// Parse a world file of any supported version, upgrade it to the
    /// current one (see [`migrate`](super::migrate)) and validate it.
    pub fn from_json(json: &str) -> AppResult<Self> {
        let file: Self = super::migrate::parse(json)?;
        file.validate()?;
        Ok(file)
    }
}
