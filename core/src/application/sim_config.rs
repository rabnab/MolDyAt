// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Central simulation configuration (#231, plan item M6).
//!
//! [`SimConfig`] holds the settings that used to be constants spread over the
//! code: the time step, the speed limit, the force cap and cutoff of the pair
//! potential, the Verlet skin and the snapshot interval of the physics thread.
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
//! The species library (#237) will be another section here (`[species]`),
//! added as a defaulted field like the others.

use crate::error::{AppError, AppResult};
use crate::objects2d::ball_impl::DEFAULT_SKIN_FACTOR;
use crate::objects2d::ball_impl::pair_potential::CapSettings;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// Time step of a new world.
pub const DEFAULT_TIME_STEP: f32 = 0.01;
/// The speed limit is this share of the shorter world side.
pub const DEFAULT_SPEED_LIMIT_FACTOR: f32 = 0.2;
/// The physics thread publishes a snapshot at most this often (about 60 Hz).
pub const DEFAULT_SNAPSHOT_INTERVAL_US: u64 = 16_667;

/// `2^(1/6)`: the Lennard-Jones minimum in units of `σ`. The cutoff must lie
/// beyond it, or the attractive well is cut away.
const LJ_MINIMUM_FACTOR: f32 = 1.122_462;

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

/// Force cap and cutoff of every pair potential (see [`CapSettings`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PotentialConfig {
    /// Repulsive force at `r_cap`, where the sloped core starts.
    pub max_force: f32,
    /// Force slope below `r_cap`.
    pub core_stiffness: f32,
    /// Cutoff of a pair as a multiple of its `σ_ij`.
    pub cutoff_factor: f32,
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

fn check_positive(value: f32, key: &str) -> AppResult {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(invalid(format!(
            "{key} must be finite and positive, got {value}"
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

    /// Check every value against the range the simulation accepts. The
    /// defaults pass.
    pub fn validate(&self) -> AppResult {
        let IntegrationConfig {
            time_step,
            speed_limit_factor,
        } = self.integration;
        check_positive(time_step, "integration.time_step")?;
        check_positive(speed_limit_factor, "integration.speed_limit_factor")?;

        let PotentialConfig {
            max_force,
            core_stiffness,
            cutoff_factor,
        } = self.potential;
        check_positive(max_force, "potential.max_force")?;
        if !(core_stiffness.is_finite() && core_stiffness >= 0.0) {
            return Err(invalid(format!(
                "potential.core_stiffness must be finite and not negative, got {core_stiffness}"
            )));
        }
        if !(cutoff_factor.is_finite() && cutoff_factor > LJ_MINIMUM_FACTOR) {
            return Err(invalid(format!(
                "potential.cutoff_factor must be finite and above {LJ_MINIMUM_FACTOR} \
                 (the potential minimum), got {cutoff_factor}"
            )));
        }

        check_positive(self.neighbours.skin_factor, "neighbours.skin_factor")?;
        if self.display.snapshot_interval_us == 0 {
            return Err(invalid("display.snapshot_interval_us must be positive"));
        }
        Ok(())
    }

    pub fn snapshot_interval(&self) -> Duration {
        Duration::from_micros(self.display.snapshot_interval_us)
    }
}
