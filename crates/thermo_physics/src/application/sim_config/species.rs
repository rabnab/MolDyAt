// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `[[species]]` entries and the species table they resolve to.

use super::*;

use crate::application::units::UnitSystem;
use crate::error::AppResult;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::external_field::Field;
use crate::objects2d::ball_impl::interaction::Model;
use crate::objects2d::ball_impl::molecule::{LinearMolecule, Site};
use crate::objects2d::ball_impl::pair_potential::PairOverride;
use crate::objects2d::ball_impl::species::molecule_species;
use crate::objects2d::ball_impl::wall_law::WallSettings;
use crate::objects2d::ball_impl::water::MbWater;
use crate::objects2d::ball_impl::{Species, SpeciesTable};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// One `[[species]]` entry: a particle type in literature units. An atom
/// or MB water gives `sigma_angstrom`, `epsilon_kelvin` and `mass_u`; a
/// rigid molecule gives `sites` instead, from which they follow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesConfig {
    /// Unique name; saves and pair overrides refer to it.
    pub name: String,
    /// `σ` in Å.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigma_angstrom: Option<f64>,
    /// `ε / k_B` in K.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epsilon_kelvin: Option<f64>,
    /// Molecular mass in u.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_u: Option<f64>,
    /// RGB or RGBA, each in `[0, 1]`.
    pub colour: Vec<f32>,
    /// Drawing radius in Å; missing draws `σ / 2` (a molecule: half its
    /// largest site `σ`). No effect on physics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_radius_angstrom: Option<f64>,
    /// Where the parameters come from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Moment of inertia about the centre in u·Å² (#348); missing means the
    /// species has no orientation. Required for `water`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moment_of_inertia_u_angstrom2: Option<f64>,
    /// Hydrogen bonds of MB water (#349).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water: Option<WaterConfig>,
    /// The Lennard-Jones sites of a rigid molecule (#406).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<SiteConfig>,
}

/// The hydrogen bonds of an MB water species.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterConfig {
    pub epsilon_hb_kelvin: f64,
    pub r_hb_angstrom: f64,
}

/// One site of a rigid molecule.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    /// Offset from the centre of mass in the body frame, in Å.
    pub offset_angstrom: [f64; 2],
    pub sigma_angstrom: f64,
    pub epsilon_kelvin: f64,
    pub mass_u: f64,
}

/// The reduced units entries are converted into.
const UNITS: UnitSystem = UnitSystem::REDUCED;

impl SpeciesConfig {
    /// The species in reduced units, checked against the `MIN_*` / `MAX_*`
    /// ranges; `key` names the entry in errors.
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
        let mut species = match (self.sigma_angstrom, self.epsilon_kelvin, self.mass_u) {
            (Some(sigma), Some(epsilon), Some(mass)) if self.sites.is_empty() => {
                let lj = check_lennard_jones(sigma, epsilon, mass, key)?;
                Species::new(&self.name, lj.mass, lj.sigma, lj.epsilon, colour)
            }
            (None, None, None)
                if !self.sites.is_empty()
                    && self.water.is_none()
                    && self.moment_of_inertia_u_angstrom2.is_none() =>
            {
                let sites = self
                    .sites
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let key = format!("{key}.sites[{i}]");
                        let [x, y] = s.offset_angstrom;
                        check_finite_f64(&[x, y], &format!("{key}.offset_angstrom"))?;
                        let lj = check_lennard_jones(
                            s.sigma_angstrom,
                            s.epsilon_kelvin,
                            s.mass_u,
                            &key,
                        )?;
                        Ok(Site {
                            offset: Vec2D::new(UNITS.from_angstrom(x), UNITS.from_angstrom(y)),
                            ..lj
                        })
                    })
                    .collect::<AppResult<Vec<_>>>()?;
                let molecule = LinearMolecule { sites };
                check_literature(
                    molecule.inertia(),
                    MIN_SPECIES_INERTIA,
                    MAX_SPECIES_INERTIA,
                    UNITS.inertia_u_angstrom2(),
                    f64::from(molecule.inertia()) * UNITS.inertia_u_angstrom2(),
                    &format!("{key}.sites (moment of inertia in u·Å²)"),
                )?;
                molecule_species(&self.name, molecule, colour)
            }
            _ => {
                return Err(invalid(format!(
                    "{key} needs sigma_angstrom, epsilon_kelvin and mass_u, or only sites \
                     (without water or moment_of_inertia_u_angstrom2)"
                )));
            }
        };
        if let Some(inertia) = self.moment_of_inertia_u_angstrom2 {
            let unit = UNITS.inertia_u_angstrom2();
            let reduced = (inertia / unit) as f32;
            check_literature(
                reduced,
                MIN_SPECIES_INERTIA,
                MAX_SPECIES_INERTIA,
                unit,
                inertia,
                &format!("{key}.moment_of_inertia_u_angstrom2"),
            )?;
            species.model = Model::Atom {
                inertia: Some(reduced),
            };
        }
        if let Some(water) = self.water {
            let Model::Atom {
                inertia: Some(inertia),
            } = species.model
            else {
                return Err(invalid(format!(
                    "{key}.water needs moment_of_inertia_u_angstrom2"
                )));
            };
            let epsilon_hb = UNITS.from_kelvin(water.epsilon_hb_kelvin);
            let r_hb = UNITS.from_angstrom(water.r_hb_angstrom);
            check_literature(
                epsilon_hb,
                MIN_PAIR_EPSILON,
                MAX_PAIR_EPSILON,
                UNITS.epsilon_kelvin,
                water.epsilon_hb_kelvin,
                &format!("{key}.water.epsilon_hb_kelvin"),
            )?;
            check_literature(
                r_hb,
                MIN_PAIR_SIGMA,
                MAX_PAIR_SIGMA,
                UNITS.sigma_angstrom,
                water.r_hb_angstrom,
                &format!("{key}.water.r_hb_angstrom"),
            )?;
            species.model = Model::MbWater(MbWater::new(epsilon_hb, r_hb, inertia));
        }
        if let Some(radius) = self.display_radius_angstrom {
            species.display_radius = UNITS.from_angstrom(radius);
            check_literature(
                species.display_radius,
                MIN_DISPLAY_RADIUS,
                MAX_DISPLAY_RADIUS,
                UNITS.sigma_angstrom,
                radius,
                &format!("{key}.display_radius_angstrom"),
            )?;
        }
        Ok(species)
    }
}

/// `species` in literature units: the entry that resolves to it exactly.
impl From<&Species> for SpeciesConfig {
    fn from(species: &Species) -> Self {
        let length = |x| literature(x, UNITS.sigma_angstrom);
        let lennard_jones = |site: &Site| SiteConfig {
            offset_angstrom: [length(site.offset.x), length(site.offset.y)],
            sigma_angstrom: length(site.sigma),
            epsilon_kelvin: literature(site.epsilon, UNITS.epsilon_kelvin),
            mass_u: literature(site.mass, UNITS.mass_u),
        };
        let sites: Vec<_> = match &species.model {
            Model::RigidLinear(molecule) => molecule.sites.iter().map(lennard_jones).collect(),
            Model::Atom { .. } | Model::MbWater(_) => Vec::new(),
        };
        let centre = sites.is_empty().then(|| lennard_jones(&species.sites()[0]));
        SpeciesConfig {
            name: species.name.clone(),
            sigma_angstrom: centre.map(|c| c.sigma_angstrom),
            epsilon_kelvin: centre.map(|c| c.epsilon_kelvin),
            mass_u: centre.map(|c| c.mass_u),
            colour: species.colour.to_vec(),
            display_radius_angstrom: Some(length(species.display_radius)),
            source: None,
            moment_of_inertia_u_angstrom2: centre
                .and(species.model.inertia())
                .map(|inertia| literature(inertia, UNITS.inertia_u_angstrom2())),
            water: match &species.model {
                Model::MbWater(w) => Some(WaterConfig {
                    epsilon_hb_kelvin: literature(w.epsilon_hb, UNITS.epsilon_kelvin),
                    r_hb_angstrom: length(w.r_hb),
                }),
                _ => None,
            },
            sites,
        }
    }
}

/// The shortest decimal of `reduced · unit` that converts back to `reduced`
/// (`3.698`, not `3.6979999583959575`).
fn literature(reduced: f32, unit: f64) -> f64 {
    let exact = f64::from(reduced) * unit;
    (0..17)
        .filter_map(|digits| format!("{exact:.digits$e}").parse::<f64>().ok())
        .find(|value| (value / unit) as f32 == reduced)
        .unwrap_or(exact)
}

/// A Lennard-Jones site from literature values, each checked against its
/// range; at the centre.
fn check_lennard_jones(sigma: f64, epsilon: f64, mass: f64, key: &str) -> AppResult<Site> {
    let site = Site {
        offset: Vec2D::zero(),
        sigma: UNITS.from_angstrom(sigma),
        epsilon: UNITS.from_kelvin(epsilon),
        mass: UNITS.from_atomic_mass(mass),
    };
    check_literature(
        site.sigma,
        MIN_PAIR_SIGMA,
        MAX_PAIR_SIGMA,
        UNITS.sigma_angstrom,
        sigma,
        &format!("{key}.sigma_angstrom"),
    )?;
    check_literature(
        site.epsilon,
        MIN_PAIR_EPSILON,
        MAX_PAIR_EPSILON,
        UNITS.epsilon_kelvin,
        epsilon,
        &format!("{key}.epsilon_kelvin"),
    )?;
    check_literature(
        site.mass,
        MIN_SPECIES_MASS,
        MAX_SPECIES_MASS,
        UNITS.mass_u,
        mass,
        &format!("{key}.mass_u"),
    )?;
    Ok(site)
}

fn check_finite_f64(values: &[f64], key: &str) -> AppResult {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(invalid(format!("{key} must be finite")))
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

/// The species table of a config, its pair overrides and its wall model
/// resolved against it: what the physics of a new world is built from.
/// Built once by [`SimConfig::validated_species`] (or `WorldFile::validate`)
/// and handed to the world's constructor, which does not resolve them again.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedSpecies {
    pub table: Arc<SpeciesTable>,
    pub pair_overrides: Vec<PairOverride>,
    pub walls: WallSettings,
    /// The `[[fields]]` (#347).
    pub fields: Vec<Field>,
}
