// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `[[species]]` entries and the species table they resolve to.

use super::*;

use crate::error::AppResult;
use crate::objects2d::ball_impl::pair_potential::PairOverride;
use crate::objects2d::ball_impl::{
    N2_EPSILON_KELVIN, N2_MASS_U, N2_SIGMA_ANGSTROM, Species, SpeciesTable,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    pub(super) fn resolve(&self, key: &str) -> AppResult<Species> {
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
/// what the physics of a new world is built from. Built once by
/// [`SimConfig::validated_species`] (or `WorldFile::validate`) and handed to
/// the world's constructor, which does not resolve them again.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedSpecies {
    pub table: Arc<SpeciesTable>,
    pub pair_overrides: Vec<PairOverride>,
}

impl ResolvedSpecies {
    /// The built-in library without pair overrides: a config without
    /// `[[species]]` and `[[potential.pair_override]]`.
    pub(crate) fn builtin() -> Self {
        Self {
            table: SpeciesTable::builtin_shared(),
            pair_overrides: Vec::new(),
        }
    }
}
