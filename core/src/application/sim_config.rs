// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Central simulation configuration (#231, plan item M6).
//!
//! [`SimConfig`] holds the settings that used to be constants spread over the
//! code: the time step, the speed limit, the force cap and cutoff of the pair
//! potential, the Verlet skin and the snapshot interval of the physics thread,
//! and per-pair overrides of the mixed Lennard-Jones parameters (#235).
//! Its defaults are today's values, so a run without a config file is
//! bit-identical to a build without `SimConfig`.
//!
//! Where a value comes from, lowest priority first:
//! 1. the defaults ([`SimConfig::default`]),
//! 2. a config file (`--config <path>`, TOML, see [`SimConfig::load`]); keys
//!    it leaves out keep their default,
//! 3. command-line flags that set the same value (`--dt`).
//!
//! A save stores the config of the run (`WorldFile::config`), and loading it
//! restores that config, so a loaded world runs as it did when it was saved.
//! Saves written before the config existed load with the defaults.
//!
//! Every key has an accepted range (the `MIN_*` / `MAX_*` constants), checked
//! by [`SimConfig::validate`] for a config file, `--dt` and a save.
//!
//! Unknown keys are rejected, here and in a save's `config`. That makes every
//! key part of the save format: a key added later is unknown to this build,
//! so this build refuses a save written by a newer build of the same format
//! version (with a hint that the save is newer) instead of running it with
//! settings it cannot see. That is accepted; bump `WORLD_FILE_VERSION` if a
//! new key must not lock older builds out. Builds from before `SimConfig`
//! ignore the whole `config` of a save and run it with the defaults (#273).
//!
//! `[[species]]` (#237) replaces the species table: without it a world uses
//! the built-in library (`SpeciesTable::builtin()`: `N2`, the former size
//! classes `XS` to `XL`, `Ar`, `Ne`, `CH4`, `O2`); with it, exactly the listed
//! species, in that order, the first being the default for new balls. Entries
//! give literature single-site Lennard-Jones values (Å, K, u), converted into
//! N₂ units. Pair overrides and saved balls name species of this table.

use crate::error::{AppError, AppResult};
use crate::objects2d::ball_impl::pair_potential::{CapSettings, PairOverride};
use crate::objects2d::ball_impl::{
    DEFAULT_SKIN_FACTOR, N2_EPSILON_KELVIN, N2_MASS_U, N2_SIGMA_ANGSTROM, Species, SpeciesSet,
    SpeciesTable,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// Time step of a new world.
pub const DEFAULT_TIME_STEP: f32 = 0.01;
/// The speed limit is this share of the shorter world side.
pub const DEFAULT_SPEED_LIMIT_FACTOR: f32 = 0.2;
/// The physics thread publishes a snapshot at most this often (about 60 Hz).
pub const DEFAULT_SNAPSHOT_INTERVAL_US: u64 = 16_667;

// Accepted ranges (inclusive). A config file is written by hand, so a value
// far outside them is a typo, not a setting: it would make the run unstable,
// crawl (one huge cell, every ball rebuilding every step) or freeze the window.

/// Time steps the `+` / `-` keys stay within; a config, `--dt` and a save
/// must too. Above the maximum the sloped core (`core_stiffness`) is unstable.
pub const MIN_TIME_STEP: f32 = 0.000_625;
pub const MAX_TIME_STEP: f32 = 0.16;
pub const MIN_SPEED_LIMIT_FACTOR: f32 = 0.01;
pub const MAX_SPEED_LIMIT_FACTOR: f32 = 10.0;
pub const MIN_MAX_FORCE: f32 = 1.0;
pub const MAX_MAX_FORCE: f32 = 10_000.0;
/// 0 is a flat core (no slope below `r_cap`).
pub const MIN_CORE_STIFFNESS: f32 = 0.0;
pub const MAX_CORE_STIFFNESS: f32 = 10_000.0;
/// Above the Lennard-Jones minimum (`2^(1/6) ≈ 1.12`), so the attractive well
/// is not cut away.
pub const MIN_CUTOFF_FACTOR: f32 = 1.2;
pub const MAX_CUTOFF_FACTOR: f32 = 10.0;
pub const MIN_SKIN_FACTOR: f32 = 0.01;
pub const MAX_SKIN_FACTOR: f32 = 1.0;
/// `σ_ij` of a pair override. The built-in species span 1 to 5.3; much
/// smaller pairs stiffen the core beyond what the time step resolves.
pub const MIN_PAIR_SIGMA: f32 = 0.5;
pub const MAX_PAIR_SIGMA: f32 = 8.0;
/// `ε_ij` of a pair override. The built-in species all have `ε = 1`.
pub const MIN_PAIR_EPSILON: f32 = 0.1;
pub const MAX_PAIR_EPSILON: f32 = 10.0;
/// Mass of a `[[species]]` entry in N₂ units (about 1.4 u to 2800 u).
pub const MIN_SPECIES_MASS: f32 = 0.05;
pub const MAX_SPECIES_MASS: f32 = 100.0;
/// Drawing radius of a `[[species]]` entry in N₂ units.
pub const MIN_DISPLAY_RADIUS: f32 = 0.05;
pub const MAX_DISPLAY_RADIUS: f32 = MAX_PAIR_SIGMA;
/// 1 ms to 1 s between two snapshots for the window.
pub const MIN_SNAPSHOT_INTERVAL_US: u64 = 1_000;
pub const MAX_SNAPSHOT_INTERVAL_US: u64 = 1_000_000;

/// Every setting of a run that is not part of the world itself. Each section
/// and key is optional in a config file or save; missing ones keep their
/// default. Unknown keys are rejected, so a typo does not silently fall back
/// to the default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SimConfig {
    pub integration: IntegrationConfig,
    pub potential: PotentialConfig,
    pub neighbours: NeighbourConfig,
    pub display: DisplayConfig,
    /// `[[species]]`: the species table of the run, replacing the built-in
    /// library. Empty means the built-in library, and is then not written, so
    /// such a save stays readable for builds without the key.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub species: Vec<SpeciesConfig>,
}

/// One `[[species]]` entry: a single-site Lennard-Jones particle type in
/// literature units. Every key but `display_radius_angstrom` and `source` is
/// required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesConfig {
    /// Unique name; saves and pair overrides refer to it.
    pub name: String,
    /// `σ` in Å.
    pub sigma_angstrom: f64,
    /// `ε / k_B` in K.
    pub epsilon_kelvin: f64,
    /// Molecular mass in u.
    pub mass_u: f64,
    /// RGB or RGBA, each in `[0, 1]`.
    pub colour: Vec<f32>,
    /// Drawing radius in Å; missing draws `σ / 2`. No effect on physics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_radius_angstrom: Option<f64>,
    /// Where the parameters come from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl SpeciesConfig {
    /// The species in N₂ units, checked against the `MIN_*` / `MAX_*` ranges;
    /// `key` names the entry in errors.
    fn resolve(&self, key: &str) -> AppResult<Species> {
        if self.name.trim().is_empty() {
            return Err(invalid(format!("{key}.name must not be empty")));
        }
        let colour = match self.colour[..] {
            [r, g, b] => [r, g, b, 1.0],
            [r, g, b, a] => [r, g, b, a],
            _ => {
                return Err(invalid(format!(
                    "{key}.colour needs 3 (RGB) or 4 (RGBA) values, got {}",
                    self.colour.len()
                )));
            }
        };
        if !colour.iter().all(|c| (0.0..=1.0).contains(c)) {
            return Err(invalid(format!(
                "{key}.colour values must be within 0..=1, got {:?}",
                self.colour
            )));
        }
        let species = Species::from_literature(
            &self.name,
            self.sigma_angstrom,
            self.epsilon_kelvin,
            self.mass_u,
            self.display_radius_angstrom,
            colour,
        );
        check_literature(
            species.sigma,
            MIN_PAIR_SIGMA,
            MAX_PAIR_SIGMA,
            N2_SIGMA_ANGSTROM,
            self.sigma_angstrom,
            &format!("{key}.sigma_angstrom"),
        )?;
        check_literature(
            species.epsilon,
            MIN_PAIR_EPSILON,
            MAX_PAIR_EPSILON,
            N2_EPSILON_KELVIN,
            self.epsilon_kelvin,
            &format!("{key}.epsilon_kelvin"),
        )?;
        check_literature(
            species.mass,
            MIN_SPECIES_MASS,
            MAX_SPECIES_MASS,
            N2_MASS_U,
            self.mass_u,
            &format!("{key}.mass_u"),
        )?;
        if let Some(radius) = self.display_radius_angstrom {
            check_literature(
                species.display_radius,
                MIN_DISPLAY_RADIUS,
                MAX_DISPLAY_RADIUS,
                N2_SIGMA_ANGSTROM,
                radius,
                &format!("{key}.display_radius_angstrom"),
            )?;
        }
        Ok(species)
    }
}

/// A literature value whose reduced form `reduced` (`value / unit`) must lie
/// in `min..=max`; the error gives the range in literature units.
fn check_literature(
    reduced: f32,
    min: f32,
    max: f32,
    unit: f64,
    value: f64,
    key: &str,
) -> AppResult {
    if (min..=max).contains(&reduced) {
        Ok(())
    } else {
        let (lo, hi) = (min as f64 * unit, max as f64 * unit);
        Err(invalid(format!(
            "{key} must be within {lo:.4}..={hi:.4}, got {value}"
        )))
    }
}

/// The species table of a config and its pair overrides resolved against it:
/// what the physics of a new world is built from.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSpecies {
    pub table: SpeciesTable,
    pub pair_overrides: Vec<PairOverride>,
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

/// Force cap and cutoff of every pair potential (see [`CapSettings`]), and
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
    pub fn cap_settings(&self) -> CapSettings {
        CapSettings {
            max_force: self.max_force,
            core_stiffness: self.core_stiffness,
            cutoff_factor: self.cutoff_factor,
        }
    }

    /// The pair overrides with their species names looked up in `species`,
    /// checked as in [`SimConfig::validate`].
    pub fn resolve_pair_overrides(&self, species: &SpeciesTable) -> AppResult<Vec<PairOverride>> {
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
    pub fn named(o: &PairOverride, species: &SpeciesTable) -> Self {
        Self {
            a: species.species(o.a).name.clone(),
            b: species.species(o.b).name.clone(),
            sigma: o.sigma,
            epsilon: o.epsilon,
        }
    }

    /// Look up the species and check the values; `key` names the entry in
    /// errors.
    fn resolve(&self, species: &SpeciesTable, key: &str) -> AppResult<PairOverride> {
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

fn invalid(message: impl Into<String>) -> AppError {
    AppError::CustomError(format!("invalid config: {}", message.into()))
}

/// `value` must lie in `min..=max` (NaN never does).
fn check_range<T: PartialOrd + Copy + std::fmt::Display>(
    value: T,
    min: T,
    max: T,
    key: &str,
) -> AppResult {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{key} must be within {min}..={max}, got {value}"
        )))
    }
}

/// Check a time step against [`MIN_TIME_STEP`]`..=`[`MAX_TIME_STEP`].
pub fn check_time_step(time_step: f32) -> AppResult {
    if (MIN_TIME_STEP..=MAX_TIME_STEP).contains(&time_step) {
        Ok(())
    } else {
        Err(AppError::CustomError(format!(
            "time step must be within {MIN_TIME_STEP}..={MAX_TIME_STEP}, got {time_step}"
        )))
    }
}

impl SimConfig {
    /// Read and validate a TOML config file.
    pub fn load(path: &Path) -> AppResult<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| invalid(format!("cannot read {}: {e}", path.display())))?;
        Self::from_toml(&text)
    }

    /// Parse and validate a config in TOML.
    pub fn from_toml(text: &str) -> AppResult<Self> {
        let config: Self = toml::from_str(text).map_err(|e| invalid(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Check every value against its range (the `MIN_*` / `MAX_*`
    /// constants), the species table and every pair override's species. The
    /// defaults pass.
    pub fn validate(&self) -> AppResult {
        self.validated_species().map(drop)
    }

    /// The species table of the run: the `[[species]]` list, or the built-in
    /// library if there is none. Checks the list (count, unique names,
    /// ranges).
    pub fn species_table(&self) -> AppResult<SpeciesTable> {
        if self.species.is_empty() {
            return Ok(SpeciesTable::builtin().clone());
        }
        if self.species.len() > SpeciesSet::CAPACITY {
            return Err(invalid(format!(
                "species lists {} species, at most {} are supported",
                self.species.len(),
                SpeciesSet::CAPACITY
            )));
        }
        let mut list: Vec<Species> = Vec::with_capacity(self.species.len());
        for (i, entry) in self.species.iter().enumerate() {
            let key = format!("species[{i}]");
            let species = entry.resolve(&key)?;
            if let Some(j) = list.iter().position(|s| s.name == species.name) {
                return Err(invalid(format!(
                    "{key}.name \"{}\" is used twice (first in species[{j}])",
                    species.name
                )));
            }
            list.push(species);
        }
        Ok(SpeciesTable::new(list))
    }

    /// [`Self::validate`], returning the species table and the pair
    /// overrides resolved against it, ready for the physics.
    pub fn validated_species(&self) -> AppResult<ResolvedSpecies> {
        let IntegrationConfig {
            time_step,
            speed_limit_factor,
        } = self.integration;
        check_range(
            time_step,
            MIN_TIME_STEP,
            MAX_TIME_STEP,
            "integration.time_step",
        )?;
        check_range(
            speed_limit_factor,
            MIN_SPEED_LIMIT_FACTOR,
            MAX_SPEED_LIMIT_FACTOR,
            "integration.speed_limit_factor",
        )?;

        let PotentialConfig {
            max_force,
            core_stiffness,
            cutoff_factor,
            pair_overrides: _, // resolved and checked below
        } = self.potential;
        check_range(
            max_force,
            MIN_MAX_FORCE,
            MAX_MAX_FORCE,
            "potential.max_force",
        )?;
        check_range(
            core_stiffness,
            MIN_CORE_STIFFNESS,
            MAX_CORE_STIFFNESS,
            "potential.core_stiffness",
        )?;
        check_range(
            cutoff_factor,
            MIN_CUTOFF_FACTOR,
            MAX_CUTOFF_FACTOR,
            "potential.cutoff_factor",
        )?;
        let table = self.species_table()?;
        let pair_overrides = self.potential.resolve_pair_overrides(&table)?;

        check_range(
            self.neighbours.skin_factor,
            MIN_SKIN_FACTOR,
            MAX_SKIN_FACTOR,
            "neighbours.skin_factor",
        )?;
        check_range(
            self.display.snapshot_interval_us,
            MIN_SNAPSHOT_INTERVAL_US,
            MAX_SNAPSHOT_INTERVAL_US,
            "display.snapshot_interval_us",
        )?;
        Ok(ResolvedSpecies {
            table,
            pair_overrides,
        })
    }

    pub fn snapshot_interval(&self) -> Duration {
        Duration::from_micros(self.display.snapshot_interval_us)
    }
}
