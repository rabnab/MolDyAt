// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The `[world]`, `[integration]`, `[potential]`, `[neighbours]` and
//! `[display]` sections of a [`SimConfig`].

use super::*;

use crate::error::AppResult;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::pair_potential::{CapSettings, PairOverride};
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

/// Verlet neighbour lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NeighbourConfig {
    /// Skin as a share of the `N2` cutoff (`skin = skin_factor ·
    /// cutoff_factor · σ_N2`). A ball rebuilds its list after moving a third
    /// of the skin.
    pub skin_factor: f32,
}

impl Default for NeighbourConfig {
    fn default() -> Self {
        Self {
            skin_factor: DEFAULT_SKIN_FACTOR,
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
