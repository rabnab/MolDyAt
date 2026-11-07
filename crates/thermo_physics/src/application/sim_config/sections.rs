// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The `[world]`, `[integration]`, `[potential]`, `[walls]`, `[neighbours]`
//! and `[display]` sections of a [`SimConfig`].

use super::*;

use crate::error::AppResult;
use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::external_field::{Field, HarmonicTrap};
use crate::objects2d::ball_impl::pair_potential::{CapSettings, PairOverride};
use crate::objects2d::ball_impl::wall_law::{WallOverride, WallSettings};
use crate::objects2d::ball_impl::{DEFAULT_SKIN_FACTOR, SpeciesTable};
use serde::{Deserialize, Serialize};

/// The world box of a new run. A loaded save keeps its own `world_rect`; its
/// `world.scale` only records what the world was created with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldConfig {
    /// Factor on the width and height of [`DEFAULT_WORLD_RECT`], around its
    /// centre (`--world-scale` overrides it).
    pub scale: f32,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

impl WorldConfig {
    /// The box of a new world: [`DEFAULT_WORLD_RECT`] scaled by `scale`.
    pub fn rect(&self) -> Rect {
        DEFAULT_WORLD_RECT.scaled_about_centre(self.scale)
    }

    pub(super) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Time integration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IntegrationConfig {
    /// Time step at the start of the run (reduced units). The `+` / `-` keys
    /// change it while the window runs.
    pub time_step: f32,
    /// Balls faster than this share of the shorter world side are slowed
    /// down to it.
    pub speed_limit_factor: f32,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            time_step: DEFAULT_TIME_STEP,
            speed_limit_factor: DEFAULT_SPEED_LIMIT_FACTOR,
        }
    }
}

/// Force cap and cutoff of every pair potential (see `CapSettings`), and
/// the pairs that do not follow the mixing rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PotentialConfig {
    /// Repulsive force at `r_cap`, where the sloped core starts.
    pub max_force: f32,
    /// Force slope below `r_cap`.
    pub core_stiffness: f32,
    /// Cutoff of a pair as a multiple of its `σ_ij`.
    pub cutoff_factor: f32,
    /// `[[potential.pair_override]]`: pairs whose `σ_ij` / `ε_ij` replace the
    /// Lorentz–Berthelot values. Empty by default, and then not written, so
    /// a save without overrides stays readable for builds without the key.
    #[serde(rename = "pair_override", skip_serializing_if = "Vec::is_empty")]
    pub pair_overrides: Vec<PairOverrideConfig>,
}

impl Default for PotentialConfig {
    fn default() -> Self {
        Self::from(CapSettings::default())
    }
}

impl From<CapSettings> for PotentialConfig {
    fn from(cap: CapSettings) -> Self {
        Self {
            max_force: cap.max_force,
            core_stiffness: cap.core_stiffness,
            cutoff_factor: cap.cutoff_factor,
            pair_overrides: Vec::new(),
        }
    }
}

impl PotentialConfig {
    pub(crate) fn cap_settings(&self) -> CapSettings {
        CapSettings {
            max_force: self.max_force,
            core_stiffness: self.core_stiffness,
            cutoff_factor: self.cutoff_factor,
        }
    }

    /// The pair overrides with their species names looked up in `species`,
    /// checked as in [`SimConfig::validate`].
    pub(crate) fn resolve_pair_overrides(
        &self,
        species: &SpeciesTable,
    ) -> AppResult<Vec<PairOverride>> {
        let mut resolved: Vec<PairOverride> = Vec::with_capacity(self.pair_overrides.len());
        for (i, entry) in self.pair_overrides.iter().enumerate() {
            let key = format!("potential.pair_override[{i}]");
            let o = entry.resolve(species, &key)?;
            // A molecule (#406) interacts through its sites only: an override
            // would add a second law between the centres (sites mix by
            // Lorentz–Berthelot; there are no site overrides yet).
            if let Some(name) = [o.a, o.b]
                .into_iter()
                .find(|&id| species.sites(id).len() > 1)
                .map(|id| &species.species(id).name)
            {
                return Err(invalid(format!(
                    "{key} names the molecule species {name}: molecules interact through \
                     their sites, which mix by Lorentz–Berthelot and cannot be overridden"
                )));
            }
            if let Some(j) = resolved.iter().position(|r| r.is_for(o.a, o.b)) {
                return Err(invalid(format!(
                    "{key} overrides the pair {} – {} again (first in potential.pair_override[{j}])",
                    entry.a, entry.b
                )));
            }
            resolved.push(o);
        }
        Ok(resolved)
    }
}

/// One `[[potential.pair_override]]` entry: species `a` and `b` (in either
/// order) interact with this `σ_ij` and / or `ε_ij` instead of the mixed one.
/// The pair's cutoff (`cutoff_factor · σ_ij`) and force cap follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairOverrideConfig {
    /// Name of a species (`N2`, `XS`, ...).
    pub a: String,
    /// Name of a species; may equal `a` (the like pair).
    pub b: String,
    /// `σ_ij`; missing keeps the mixed value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma: Option<f32>,
    /// `ε_ij`; missing keeps the mixed value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epsilon: Option<f32>,
}

impl PairOverrideConfig {
    /// `o` with its species named after `species`.
    pub(crate) fn named(o: &PairOverride, species: &SpeciesTable) -> Self {
        Self {
            a: species.species(o.a).name.clone(),
            b: species.species(o.b).name.clone(),
            sigma: o.sigma,
            epsilon: o.epsilon,
        }
    }

    /// Look up the species and check the values; `key` names the entry in
    /// errors.
    pub(super) fn resolve(&self, species: &SpeciesTable, key: &str) -> AppResult<PairOverride> {
        let lookup = |name: &str, field: &str| {
            species.by_name(name).ok_or_else(|| {
                let known: Vec<&str> = species.iter().map(|s| s.name.as_str()).collect();
                invalid(format!(
                    "{key}.{field}: unknown species \"{name}\" (known: {})",
                    known.join(", ")
                ))
            })
        };
        let (a, b) = (lookup(&self.a, "a")?, lookup(&self.b, "b")?);
        if self.sigma.is_none() && self.epsilon.is_none() {
            return Err(invalid(format!(
                "{key} ({} – {}) sets neither sigma nor epsilon",
                self.a, self.b
            )));
        }
        if let Some(sigma) = self.sigma {
            check_range(
                sigma,
                MIN_PAIR_SIGMA,
                MAX_PAIR_SIGMA,
                &format!("{key}.sigma"),
            )?;
        }
        if let Some(epsilon) = self.epsilon {
            check_range(
                epsilon,
                MIN_PAIR_EPSILON,
                MAX_PAIR_EPSILON,
                &format!("{key}.epsilon"),
            )?;
        }
        Ok(PairOverride {
            a,
            b,
            sigma: self.sigma,
            epsilon: self.epsilon,
        })
    }
}

/// Which wall model a run uses (`[walls] model`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WallModel {
    /// Wall potential with a hard backstop (#280), the default.
    #[default]
    Soft,
    /// Hard contact at `σ_i / 2` only, as before #280.
    Hard,
}

/// The walls (`[walls]`): box walls, wall pieces and closed valves. See
/// `wall_law` for the models. Not written at its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WallsConfig {
    pub model: WallModel,
    /// `σ_w` of the wall pseudo-species: `σ_iw = (σ_i + σ_w) / 2`.
    pub sigma: f32,
    /// `ε_w` of the wall pseudo-species: `ε_iw = √(ε_i · ε_w)`.
    pub epsilon: f32,
    /// Keep the attractive well up to the pair cutoff instead of cutting the
    /// potential at its minimum (WCA, purely repulsive).
    pub attractive: bool,
    /// Distance of the hard backstop behind a soft wall, as a share of `σ_i`.
    pub backstop: f32,
    /// `[[walls.override]]`: species whose `σ_iw` / `ε_iw` replace the mixed
    /// values. Not written while empty.
    #[serde(rename = "override", skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<WallOverrideConfig>,
}

impl Default for WallsConfig {
    fn default() -> Self {
        Self::from_settings(&WallSettings::default(), SpeciesTable::builtin())
    }
}

impl WallsConfig {
    /// The hard walls of a run from before #280.
    pub fn hard() -> Self {
        Self {
            model: WallModel::Hard,
            ..Self::default()
        }
    }

    pub(super) fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `settings` with species named after `species`.
    pub(crate) fn from_settings(settings: &WallSettings, species: &SpeciesTable) -> Self {
        Self {
            model: if settings.soft {
                WallModel::Soft
            } else {
                WallModel::Hard
            },
            sigma: settings.sigma,
            epsilon: settings.epsilon,
            attractive: settings.attractive,
            backstop: settings.backstop,
            overrides: settings
                .overrides
                .iter()
                .map(|o| WallOverrideConfig {
                    species: species.species(o.species).name.clone(),
                    sigma: o.sigma,
                    epsilon: o.epsilon,
                })
                .collect(),
        }
    }

    /// Check the values and look up the species of the overrides.
    pub(crate) fn resolve(&self, species: &SpeciesTable) -> AppResult<WallSettings> {
        check_range(self.sigma, MIN_WALL_SIGMA, MAX_WALL_SIGMA, "walls.sigma")?;
        check_range(
            self.epsilon,
            MIN_PAIR_EPSILON,
            MAX_PAIR_EPSILON,
            "walls.epsilon",
        )?;
        check_range(
            self.backstop,
            MIN_BACKSTOP_FACTOR,
            MAX_BACKSTOP_FACTOR,
            "walls.backstop",
        )?;
        let mut overrides: Vec<WallOverride> = Vec::with_capacity(self.overrides.len());
        for (i, entry) in self.overrides.iter().enumerate() {
            let key = format!("walls.override[{i}]");
            let id = species.by_name(&entry.species).ok_or_else(|| {
                let known: Vec<&str> = species.iter().map(|s| s.name.as_str()).collect();
                invalid(format!(
                    "{key}.species: unknown species \"{}\" (known: {})",
                    entry.species,
                    known.join(", ")
                ))
            })?;
            if species.sites(id).len() > 1 {
                return Err(invalid(format!(
                    "{key}: {} is a molecule species; its sites meet the walls with the \
                     mixed site laws, which cannot be overridden",
                    entry.species
                )));
            }
            if overrides.iter().any(|o| o.species == id) {
                return Err(invalid(format!(
                    "{key} overrides species {} again",
                    entry.species
                )));
            }
            if entry.sigma.is_none() && entry.epsilon.is_none() {
                return Err(invalid(format!(
                    "{key} ({}) sets neither sigma nor epsilon",
                    entry.species
                )));
            }
            if let Some(sigma) = entry.sigma {
                check_range(
                    sigma,
                    MIN_WALL_PAIR_SIGMA,
                    MAX_PAIR_SIGMA,
                    &format!("{key}.sigma"),
                )?;
            }
            if let Some(epsilon) = entry.epsilon {
                check_range(
                    epsilon,
                    MIN_PAIR_EPSILON,
                    MAX_PAIR_EPSILON,
                    &format!("{key}.epsilon"),
                )?;
            }
            overrides.push(WallOverride {
                species: id,
                sigma: entry.sigma,
                epsilon: entry.epsilon,
            });
        }
        Ok(WallSettings {
            soft: self.model == WallModel::Soft,
            sigma: self.sigma,
            epsilon: self.epsilon,
            attractive: self.attractive,
            backstop: self.backstop,
            overrides,
        })
    }
}

/// One `[[walls.override]]` entry: species `species` meets the walls with
/// this `σ_iw` and / or `ε_iw` instead of the mixed one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallOverrideConfig {
    pub species: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epsilon: Option<f32>,
}

/// Kind of a configured external field (`[[fields]] kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    /// Harmonic trap `U = ½·k·|r - c|²` (`center`, `stiffness`).
    Harmonic,
}

/// One `[[fields]]` entry (#347): an external field besides gravity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldConfig {
    pub kind: FieldKind,
    /// Centre of the trap, world coordinates.
    pub center: [f32; 2],
    /// Spring constant `k` (ε/σ²).
    pub stiffness: f32,
}

impl FieldConfig {
    pub(crate) fn from_field(field: &Field) -> Self {
        match field {
            Field::Harmonic(trap) => Self {
                kind: FieldKind::Harmonic,
                center: [trap.center.x, trap.center.y],
                stiffness: trap.stiffness,
            },
        }
    }

    /// Check the values; `key` names the entry in errors.
    pub(crate) fn resolve(&self, key: &str) -> AppResult<Field> {
        for (axis, value) in ["x", "y"].iter().zip(self.center) {
            check_range(
                value,
                -MAX_FIELD_CENTER,
                MAX_FIELD_CENTER,
                &format!("{key}.center.{axis}"),
            )?;
        }
        check_range(
            self.stiffness,
            MIN_FIELD_STIFFNESS,
            MAX_FIELD_STIFFNESS,
            &format!("{key}.stiffness"),
        )?;
        Ok(match self.kind {
            FieldKind::Harmonic => Field::Harmonic(HarmonicTrap {
                center: Vec2D::new(self.center[0], self.center[1]),
                stiffness: self.stiffness,
            }),
        })
    }
}

/// Verlet neighbour lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NeighbourConfig {
    /// Skin as a share of the `N2` cutoff (`skin = skin_factor ·
    /// cutoff_factor · σ_N2`). A ball rebuilds its list after moving a third
    /// of the skin.
    pub skin_factor: f32,
    /// Every this many steps the balls are sorted by cell, so balls that
    /// are close in space are close in memory (P4, #345). 0 (the default)
    /// never sorts. Sorting changes the ball order and so the order of f32
    /// sums: runs with and without it are not bit-identical. Not written
    /// while 0, so saves stay readable for builds without the key.
    #[serde(skip_serializing_if = "is_zero")]
    pub reorder_interval: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl Default for NeighbourConfig {
    fn default() -> Self {
        Self {
            skin_factor: DEFAULT_SKIN_FACTOR,
            reorder_interval: 0,
        }
    }
}

/// How the physics thread feeds the window. Does not change results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayConfig {
    /// Least time between two snapshots for the window, in microseconds.
    pub snapshot_interval_us: u64,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            snapshot_interval_us: DEFAULT_SNAPSHOT_INTERVAL_US,
        }
    }
}
