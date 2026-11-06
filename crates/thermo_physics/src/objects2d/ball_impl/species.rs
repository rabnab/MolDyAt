// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Particle types (species): the per-type data every ball of that type shares.
//!
//! A ball stores only its [`SpeciesId`]; mass, `σ`, `ε` and colour come from
//! the [`SpeciesTable`]. Two balls interact by Lennard-Jones on their centre
//! distance with the mixed `σ_ij`, `ε_ij` (see `pair_potential::PairTable`).
//! `display_radius` is for drawing only.
//!
//! The built-in table (the species library, #237) holds `N2`, the default
//! species, the five former size classes `XS` to `XL`, and the real gases
//! `Ar`, `Ne`, `CH4` and `O2` (single-site Lennard-Jones from the literature,
//! see [`LIBRARY`]). `Ar` is the reference of the reduced units (`σ = ε = m =
//! 1`, #405); every other species is converted from Å, K and u. A size class
//! of radius `R` (in the former N₂ units) became the species with
//! `σ = (2·R·2^(-1/6) + 1)·σ_N2`: with arithmetic mixing every pair then keeps
//! its old equilibrium centre distance (see `docs/simulation.md`). Old saves
//! migrate to these size classes, so they stay in the table.
//!
//! A config file can replace the whole table with its own `[[species]]` list
//! (`SimConfig::species_table`); a world keeps the table it was built with.

use crate::objects2d::ball_impl::molecule::{LinearMolecule, MoleculeSite};
use crate::objects2d::ball_impl::water::MbWater;
use std::sync::{Arc, LazyLock};

/// Index of a species in its [`SpeciesTable`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpeciesId(u16);

impl SpeciesId {
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One particle type.
#[derive(Clone, Debug, PartialEq)]
pub struct Species {
    pub name: String,
    pub mass: f32,
    /// Lennard-Jones length: like pairs have zero potential at centre
    /// distance `σ` and their minimum at `2^(1/6)·σ`.
    pub sigma: f32,
    /// Lennard-Jones well depth.
    pub epsilon: f32,
    /// Radius the renderer draws. Has no effect on physics.
    pub display_radius: f32,
    /// RGBA in `[0, 1]`.
    pub colour: [f32; 4],
    /// Moment of inertia about the centre (#348), `None` for a species
    /// without orientation (every single-site Lennard-Jones species).
    pub inertia: Option<f32>,
    /// Mercedes-Benz hydrogen-bond arms (#349), for water.
    pub water: Option<MbWater>,
    /// Lennard-Jones sites of a rigid linear molecule (#406). Its own `σ`
    /// is then the envelope of the sites (placement, the cell grid) and its
    /// `ε` is 0: it interacts only through its sites.
    pub molecule: Option<LinearMolecule>,
}

impl Species {
    /// A species drawn at its default size, `σ / 2`.
    pub(crate) fn new(name: &str, mass: f32, sigma: f32, epsilon: f32, colour: [f32; 4]) -> Self {
        Species {
            name: name.to_string(),
            mass,
            sigma,
            epsilon,
            display_radius: 0.5 * sigma,
            colour,
            inertia: None,
            water: None,
            molecule: None,
        }
    }

    /// A species from its literature single-site Lennard-Jones parameters
    /// (`σ` in Å, `ε / k_B` in K, mass in u), in Ar units. `display_radius`
    /// in Å; `None` draws it at `σ / 2`.
    pub(crate) fn from_literature(
        name: &str,
        sigma_angstrom: f64,
        epsilon_kelvin: f64,
        mass_u: f64,
        display_radius_angstrom: Option<f64>,
        colour: [f32; 4],
    ) -> Self {
        let sigma = (sigma_angstrom / AR_SIGMA_ANGSTROM) as f32;
        let mut species = Species::new(
            name,
            (mass_u / AR_MASS_U) as f32,
            sigma,
            (epsilon_kelvin / AR_EPSILON_KELVIN) as f32,
            colour,
        );
        if let Some(radius) = display_radius_angstrom {
            species.display_radius = (radius / AR_SIGMA_ANGSTROM) as f32;
        }
        species
    }

    /// Half of `σ`: the size of a ball for contact and overlap checks. Two
    /// balls touch (zero pair potential) when their centres are the sum of
    /// their contact radii apart, `σ_ij` under arithmetic mixing.
    pub(crate) fn contact_radius(&self) -> f32 {
        0.5 * self.sigma
    }
}

/// The species a world uses, indexed by [`SpeciesId`]. Holds at least one and
/// at most [`SpeciesSet::CAPACITY`] species.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeciesTable {
    species: Vec<Species>,
}

/// Single-site Lennard-Jones `σ` of Ar in Å: the reduced unit of length
/// (`UnitSystem::AR`, #405). Hirschfelder, Curtiss and Bird, *Molecular
/// Theory of Gases and Liquids* (Wiley, 1954), table I-A, from second virial
/// coefficients.
pub const AR_SIGMA_ANGSTROM: f64 = 3.405;
/// `ε / k_B` of Ar in K: the reduced unit of temperature (same source).
pub const AR_EPSILON_KELVIN: f64 = 119.8;
/// Atomic mass of Ar in u: the reduced unit of mass.
pub const AR_MASS_U: f64 = 39.948;

/// Single-site Lennard-Jones `σ` of N₂ in Å (same source); the length unit
/// before #405 (`UnitSystem::N2`).
pub const N2_SIGMA_ANGSTROM: f64 = 3.698;
/// `ε / k_B` of N₂ in K: the temperature unit before #405.
pub const N2_EPSILON_KELVIN: f64 = 95.05;
/// Molecular mass of N₂ in u: the mass unit before #405.
pub const N2_MASS_U: f64 = 28.0134;

/// Source of the library gases' `σ` and `ε`.
const HCB_1954: &str = "Hirschfelder, Curtiss & Bird, Molecular Theory of Gases and Liquids \
     (Wiley, 1954), table I-A, second virial coefficients; mass: IUPAC standard atomic weights";

/// Source of `CH4` (#409): TraPPE united-atom methane.
const TRAPPE_UA_CH4: &str = "TraPPE-UA: Martin & Siepmann, J. Phys. Chem. B 102, 2569 (1998); \
     mass: IUPAC standard atomic weights";

/// Source of `NH3` (#409): single-site Lennard-Jones from viscosity.
const SVEHLA_1962: &str = "Svehla, NASA Technical Report R-132 (1962), from viscosity; \
     mass: IUPAC standard atomic weights";

/// One real gas of the built-in library, in literature units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LibrarySpecies {
    pub name: &'static str,
    pub sigma_angstrom: f64,
    pub epsilon_kelvin: f64,
    pub mass_u: f64,
    pub colour: [f32; 4],
    /// Where `σ`, `ε` and the mass come from.
    pub source: &'static str,
}

/// The real gases of the built-in table, after `N2` and the size classes.
/// Append new ones at the end: saves name species, and the order is the order
/// the UI steps through.
pub const LIBRARY: [LibrarySpecies; 6] = [
    LibrarySpecies {
        name: "Ar",
        sigma_angstrom: AR_SIGMA_ANGSTROM,
        epsilon_kelvin: AR_EPSILON_KELVIN,
        mass_u: AR_MASS_U,
        colour: [0.6, 0.45, 1.0, 1.0], // violet
        source: HCB_1954,
    },
    LibrarySpecies {
        name: "Ne",
        sigma_angstrom: 2.749,
        epsilon_kelvin: 35.60,
        mass_u: 20.180,
        colour: [1.0, 0.6, 0.1, 1.0], // orange
        source: HCB_1954,
    },
    LibrarySpecies {
        name: "CH4",
        sigma_angstrom: 3.73,
        epsilon_kelvin: 148.0,
        mass_u: 16.043,
        colour: [0.55, 0.85, 0.35, 1.0], // light green
        source: TRAPPE_UA_CH4,
    },
    LibrarySpecies {
        name: "O2",
        sigma_angstrom: 3.58,
        epsilon_kelvin: 117.5,
        mass_u: 31.999,
        colour: [0.95, 0.45, 0.55, 1.0], // pink
        source: HCB_1954,
    },
    // #409: polar and hydrogen-bonding in reality; the single-site LJ model
    // ignores both (see `docs/units.md`).
    LibrarySpecies {
        name: "NH3",
        sigma_angstrom: 2.900,
        epsilon_kelvin: 558.3,
        mass_u: 17.031,
        colour: [0.3, 0.55, 1.0, 1.0], // sky blue
        source: SVEHLA_1962,
    },
    // `CH4` before #409, kept for saves that used it (format v7 renames
    // their `CH4` to this).
    LibrarySpecies {
        name: "CH4_HCB",
        sigma_angstrom: 3.817,
        epsilon_kelvin: 148.2,
        mass_u: 16.043,
        colour: [0.4, 0.65, 0.25, 1.0], // darker green
        source: HCB_1954,
    },
];

/// Name of the former `CH4` (Hirschfelder, Curtiss & Bird) in the table.
pub(crate) const CH4_HCB_NAME: &str = "CH4_HCB";
/// 2^(-1/6): a size class of radius `R` adds `2·R·2^(-1/6)` to `σ`.
const INV_SIXTH_ROOT_OF_TWO: f32 = 0.890_898_7;
/// Radius of the smallest former size class. Each further one doubles it.
const SIZE_CLASS_MIN_RADIUS: f32 = 0.15;
const SIZE_CLASSES: [(&str, [f32; 4]); 5] = [
    ("XS", [1.0, 0.0, 0.0, 1.0]), // red
    ("S", [0.0, 1.0, 0.0, 1.0]),  // green
    ("M", [0.0, 0.0, 1.0, 1.0]),  // blue
    ("L", [1.0, 1.0, 0.0, 1.0]),  // yellow
    ("XL", [1.0, 0.0, 1.0, 1.0]), // magenta
];
/// Names of the former size classes, by size index: what a version 1 save's
/// `size_idx` means.
pub(crate) const SIZE_CLASS_NAMES: [&str; 5] = ["XS", "S", "M", "L", "XL"];

/// Species new balls and new source spheres start with (`N2`).
pub const DEFAULT_SPECIES: SpeciesId = SpeciesId::new(0);

/// The former size class `M` (contact radius ≈ 1.12): for tests written
/// around balls of radius about 1.
#[cfg(any(test, feature = "test-support"))]
pub const SPECIES_M: SpeciesId = SpeciesId::new(3);

/// `Ar`, the unit species (`σ = ε = m = 1`, #405): for tests written around
/// reduced values of 1.
#[cfg(any(test, feature = "test-support"))]
pub const SPECIES_AR: SpeciesId = SpeciesId::new(6);

static BUILTIN_TABLE: LazyLock<Arc<SpeciesTable>> = LazyLock::new(|| {
    let n2 = Species::from_literature(
        "N2",
        N2_SIGMA_ANGSTROM,
        N2_EPSILON_KELVIN,
        N2_MASS_U,
        None,
        [0.0, 1.0, 1.0, 1.0],
    );
    // Defined in the former N₂ units (σ_N2 = ε_N2 = m_N2 = 1), so their
    // physical size, depth and mass stay as they were (#405).
    let size_classes = SIZE_CLASSES.iter().enumerate().map(|(i, &(name, colour))| {
        let radius = SIZE_CLASS_MIN_RADIUS * 2.0_f32.powi(i as i32);
        let sigma_n2 = 2.0 * radius * INV_SIXTH_ROOT_OF_TWO + 1.0;
        Species::from_literature(
            name,
            f64::from(sigma_n2) * N2_SIGMA_ANGSTROM,
            N2_EPSILON_KELVIN,
            f64::from(size_class_mass(radius)) * N2_MASS_U,
            None,
            colour,
        )
    });
    let library = LIBRARY.iter().map(|s| {
        Species::from_literature(
            s.name,
            s.sigma_angstrom,
            s.epsilon_kelvin,
            s.mass_u,
            None,
            s.colour,
        )
    });
    Arc::new(SpeciesTable::new(
        std::iter::once(n2)
            .chain(size_classes)
            .chain(library)
            .chain(std::iter::once(mb_water()))
            .chain(std::iter::once(n2_dumbbell()))
            .chain(std::iter::once(h2_dumbbell()))
            .chain(std::iter::once(co2_chain()))
            .collect(),
    ))
});

/// MB water (#349): the Mercedes-Benz model with `r_HB` = 2.8 Å (the O–O
/// distance of a hydrogen bond) and `ε_HB / k_B` = 1385 K, chosen so that the
/// model's density maximum (`T ≈ 0.2 ε_HB / k_B`) lies near 277 K (owner
/// decision). Its Lennard-Jones disk has `σ = 0.7 r_HB`, `ε = 0.1 ε_HB`; mass
/// and moment of inertia are those of H₂O (1.15 u·Å², the in-plane axis).
/// Rotations are fast: the alignment of its hydrogen bonds limits water
/// runs to `dt ≈ 0.0007` (`BallPhysics::time_step_limit`).
fn mb_water() -> Species {
    let mut water = Species::from_literature(
        "H2O",
        0.7 * MB_R_HB_ANGSTROM,
        0.1 * MB_EPSILON_HB_KELVIN,
        18.015,
        Some(0.5 * MB_R_HB_ANGSTROM),
        [0.95, 0.95, 1.0, 1.0], // white
    );
    water.inertia = Some((1.15 / (AR_MASS_U * AR_SIGMA_ANGSTROM * AR_SIGMA_ANGSTROM)) as f32);
    water.water = Some(MbWater::new(
        (MB_EPSILON_HB_KELVIN / AR_EPSILON_KELVIN) as f32,
        (MB_R_HB_ANGSTROM / AR_SIGMA_ANGSTROM) as f32,
    ));
    water
}

/// N₂ as a rigid dumbbell (#406), `N2_2S`: two TraPPE nitrogen sites
/// (Potoff & Siepmann, *AIChE J.* 47, 1676 (2001): `σ` = 3.31 Å, `ε/k_B` =
/// 36.0 K, bond 1.10 Å), without TraPPE's centre charges (no electrostatics).
/// Mass 2 × 14.007 u; moment of inertia from the sites. The single-site
/// `N2` stays the default species (owner decision).
fn n2_dumbbell() -> Species {
    let molecule = LinearMolecule::diatomic(
        (N2_BOND_ANGSTROM / AR_SIGMA_ANGSTROM) as f32,
        (TRAPPE_N_SIGMA_ANGSTROM / AR_SIGMA_ANGSTROM) as f32,
        (TRAPPE_N_EPSILON_KELVIN / AR_EPSILON_KELVIN) as f32,
        (14.007 / AR_MASS_U) as f32,
    );
    molecule_species("N2_2S", molecule, [0.2, 0.75, 0.95, 1.0])
}

/// H₂ as a rigid dumbbell (#407), `H2_2S`: two Lennard-Jones sites on the
/// atoms (Yang & Zhong, *J. Phys. Chem. B* 109, 11862 (2005): `σ` = 2.72 Å,
/// `ε/k_B` = 10.0 K, bond 0.74 Å), fitted to PVT data and self-diffusion of
/// H₂ at 80–300 K; one of the two best of eight LJ models in Barraco et al.,
/// *J. Phys. Chem. A* 127 (2023). Classical: zero-point motion and frozen
/// rotations at low temperature are not modelled. Light and fast: its
/// collisions limit runs to `dt ≈ 0.0016` (`BallPhysics::time_step_limit`).
fn h2_dumbbell() -> Species {
    let molecule = LinearMolecule::diatomic(
        (0.74 / AR_SIGMA_ANGSTROM) as f32,
        (2.72 / AR_SIGMA_ANGSTROM) as f32,
        (10.0 / AR_EPSILON_KELVIN) as f32,
        (1.008 / AR_MASS_U) as f32,
    );
    molecule_species("H2_2S", molecule, [1.0, 0.85, 0.85, 1.0])
}

/// CO₂ as a rigid linear O–C–O chain (#408), `CO2_3S`: TraPPE sites
/// (Potoff & Siepmann, *AIChE J.* 47, 1676 (2001): O σ = 3.05 Å, ε/k_B =
/// 79.0 K; C σ = 2.80 Å, ε/k_B = 27.0 K; C–O 1.16 Å), without the partial
/// charges: its quadrupole needs long-range electrostatics (out of scope).
fn co2_chain() -> Species {
    let ar = |angstrom: f64| (angstrom / AR_SIGMA_ANGSTROM) as f32;
    let kelvin = |k: f64| (k / AR_EPSILON_KELVIN) as f32;
    let mass = |u: f64| (u / AR_MASS_U) as f32;
    let oxygen = |offset| MoleculeSite {
        offset,
        sigma: ar(3.05),
        epsilon: kelvin(79.0),
        mass: mass(15.999),
    };
    let carbon = MoleculeSite {
        offset: 0.0,
        sigma: ar(2.80),
        epsilon: kelvin(27.0),
        mass: mass(12.011),
    };
    let molecule = LinearMolecule {
        sites: vec![oxygen(-ar(1.16)), carbon, oxygen(ar(1.16))],
    };
    molecule_species("CO2_3S", molecule, [0.6, 0.6, 0.6, 1.0])
}

/// A species whose balls are the rigid molecule `molecule` (#406).
pub(crate) fn molecule_species(name: &str, molecule: LinearMolecule, colour: [f32; 4]) -> Species {
    let mut species = Species::new(
        name,
        molecule.mass(),
        molecule.envelope_sigma(),
        0.0,
        colour,
    );
    species.inertia = Some(molecule.inertia());
    species.display_radius = 0.5 * molecule.max_site_sigma();
    species.molecule = Some(molecule);
    species
}

/// TraPPE N₂ site (#406).
const TRAPPE_N_SIGMA_ANGSTROM: f64 = 3.31;
const TRAPPE_N_EPSILON_KELVIN: f64 = 36.0;
/// N≡N bond length in Å.
const N2_BOND_ANGSTROM: f64 = 1.10;

/// `r_HB` of MB water in Å.
pub const MB_R_HB_ANGSTROM: f64 = 2.8;
/// `ε_HB / k_B` of MB water in K.
pub const MB_EPSILON_HB_KELVIN: f64 = 1385.0;

/// Mass of the former size classes: grows slowly with the radius.
fn size_class_mass(radius: f32) -> f32 {
    let density = 2. / 10.;
    1. + density * (radius / SIZE_CLASS_MIN_RADIUS - 1.).powf(1.2)
}

impl SpeciesTable {
    /// Panics if `species` is empty or longer than [`SpeciesSet::CAPACITY`].
    pub(crate) fn new(species: Vec<Species>) -> Self {
        assert!(
            !species.is_empty() && species.len() <= SpeciesSet::CAPACITY,
            "a species table holds 1 to {} species, not {}",
            SpeciesSet::CAPACITY,
            species.len()
        );
        SpeciesTable { species }
    }

    /// The built-in species: `N2`, the former size classes `XS` to `XL` and
    /// the library gases ([`LIBRARY`]). The table of every world that is not
    /// given its own.
    pub fn builtin() -> &'static SpeciesTable {
        &BUILTIN_TABLE
    }

    /// [`Self::builtin`], shared.
    pub fn builtin_shared() -> Arc<SpeciesTable> {
        Arc::clone(&BUILTIN_TABLE)
    }

    pub fn get(&self, id: SpeciesId) -> Option<&Species> {
        self.species.get(id.index())
    }

    /// The species with `id`. Panics if the table has no such species; use it
    /// only for ids that came from this table.
    pub fn species(&self, id: SpeciesId) -> &Species {
        &self.species[id.index()]
    }

    /// The id of the species at `index`, if the table has one.
    pub fn id(&self, index: usize) -> Option<SpeciesId> {
        (index < self.species.len()).then(|| SpeciesId::new(index as u16))
    }

    /// The id of the species called `name`.
    pub fn by_name(&self, name: &str) -> Option<SpeciesId> {
        self.species
            .iter()
            .position(|s| s.name == name)
            .and_then(|index| self.id(index))
    }

    /// Every species id, in table order.
    pub fn ids(&self) -> impl Iterator<Item = SpeciesId> + '_ {
        (0..self.species.len()).map(|index| SpeciesId::new(index as u16))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Species> {
        self.species.iter()
    }

    pub fn len(&self) -> usize {
        self.species.len()
    }

    /// Never true for a valid table (it holds at least the default species).
    pub fn is_empty(&self) -> bool {
        self.species.is_empty()
    }

    /// The species after `id`, wrapping around: what the UI steps through.
    pub(crate) fn next(&self, id: SpeciesId) -> SpeciesId {
        let len = self.species.len();
        SpeciesId::new(((id.index() % len + 1) % len) as u16)
    }

    /// The species before `id`, wrapping around.
    pub(crate) fn previous(&self, id: SpeciesId) -> SpeciesId {
        let len = self.species.len();
        SpeciesId::new(((id.index() % len + len - 1) % len) as u16)
    }
}

/// A set of species ids, e.g. the species present in a world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpeciesSet(u64);

impl SpeciesSet {
    /// Largest number of species a table may hold.
    pub const CAPACITY: usize = 64;

    pub(crate) fn insert(&mut self, id: SpeciesId) {
        self.0 |= 1 << id.index();
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: SpeciesId) -> bool {
        self.0 & (1 << id.index()) != 0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The ids in the set, ascending.
    pub(crate) fn iter(&self) -> impl Iterator<Item = SpeciesId> {
        let bits = self.0;
        (0..Self::CAPACITY as u16)
            .filter(move |&i| bits & (1 << i) != 0)
            .map(SpeciesId::new)
    }
}

impl FromIterator<SpeciesId> for SpeciesSet {
    fn from_iter<I: IntoIterator<Item = SpeciesId>>(iter: I) -> Self {
        let mut set = SpeciesSet::default();
        for id in iter {
            set.insert(id);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE_CLASS_RADII: [f32; 5] = [0.15, 0.3, 0.6, 1.2, 2.4];

    #[test]
    fn n2_is_the_default_in_argon_units() {
        let n2 = SpeciesTable::builtin().species(DEFAULT_SPECIES);
        assert_eq!(n2.name, "N2");
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(close(n2.sigma, 1.0860) && close(n2.epsilon, 0.7934) && close(n2.mass, 0.7012));
        assert_eq!(n2.display_radius, 0.5 * n2.sigma);
    }

    /// Ar defines the reduced units (#405).
    #[test]
    fn argon_is_the_unit() {
        let table = SpeciesTable::builtin();
        let ar = table.species(table.by_name("Ar").unwrap());
        assert_eq!((ar.mass, ar.sigma, ar.epsilon), (1.0, 1.0, 1.0));
        assert_eq!(table.by_name("Ar"), Some(SPECIES_AR));
    }

    /// A former size class keeps its mass and colour, and every pair of them
    /// keeps its old equilibrium centre distance `R_i + R_j + 2^(1/6)`, all
    /// in the former N₂ units (here converted to Ar units, #405).
    #[test]
    fn size_classes_keep_mass_colour_and_equilibrium_distance() {
        let table = SpeciesTable::builtin();
        // `1 + 0.2 * (r / 0.15 - 1)^1.2`, evaluated by the former
        // `BallFactory::mass_for_size_idx`.
        let masses = [1.0_f32, 1.2, 1.7474387, 3.0660827, 6.1563163];
        let colours = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
            [1.0, 0.0, 1.0, 1.0],
        ];
        let sixth_root_of_two = 2.0_f32.powf(1.0 / 6.0);
        let length = (N2_SIGMA_ANGSTROM / AR_SIGMA_ANGSTROM) as f32;
        for (i, name) in SIZE_CLASS_NAMES.iter().enumerate() {
            let s = table.species(table.by_name(name).unwrap());
            let mass = (f64::from(masses[i]) * N2_MASS_U / AR_MASS_U) as f32;
            assert_eq!(s.mass.to_bits(), mass.to_bits(), "mass {name}");
            assert_eq!(s.colour, colours[i], "colour {name}");
            assert_eq!(s.display_radius, 0.5 * s.sigma);
            for (j, other) in SIZE_CLASS_NAMES.iter().enumerate() {
                let o = table.species(table.by_name(other).unwrap());
                let sigma_ij = 0.5 * (s.sigma + o.sigma);
                let old = SIZE_CLASS_RADII[i] + SIZE_CLASS_RADII[j] + sixth_root_of_two;
                let new = sixth_root_of_two * sigma_ij / length;
                assert!((new - old).abs() < 1e-5, "{name}-{other}: {new} vs {old}");
            }
        }
    }

    #[test]
    fn next_wraps_around() {
        let table = SpeciesTable::builtin();
        let last = (table.len() - 1) as u16;
        let id = SpeciesId::new;
        assert_eq!(table.next(id(5)), id(6));
        assert_eq!(table.next(id(last)), id(0));
        assert_eq!(table.previous(id(0)), id(last));
        assert_eq!(table.previous(id(6)), id(5));
        // An id outside the table wraps into it in both directions.
        let outside = last + 2;
        let wrapped = outside as usize % table.len();
        assert_eq!(table.next(id(outside)).index(), (wrapped + 1) % table.len());
        assert_eq!(
            table.previous(id(outside)).index(),
            (wrapped + table.len() - 1) % table.len()
        );
    }

    #[test]
    fn id_rejects_indices_outside_the_table() {
        let table = SpeciesTable::builtin();
        let n = table.len();
        assert_eq!(table.id(n - 1), Some(SpeciesId::new((n - 1) as u16)));
        assert_eq!(table.id(n), None);
        assert!(table.get(SpeciesId::new(n as u16)).is_none());
        assert_eq!(table.by_name("M"), Some(SpeciesId::new(3)));
        assert_eq!(table.by_name("M"), Some(SPECIES_M));
        assert_eq!(table.by_name("Ar"), Some(SpeciesId::new(6)));
        assert_eq!(table.by_name("Kr"), None);
    }

    /// N₂ in literature units is exactly the default species.
    #[test]
    fn n2_from_literature_is_the_default() {
        let n2 = Species::from_literature(
            "N2",
            N2_SIGMA_ANGSTROM,
            N2_EPSILON_KELVIN,
            N2_MASS_U,
            None,
            [0.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(&n2, SpeciesTable::builtin().species(DEFAULT_SPECIES));
    }

    /// The library gases are their literature values in Ar units, drawn at
    /// `σ / 2`, after `N2` and the size classes.
    #[test]
    fn library_gases_follow_their_literature_values() {
        let table = SpeciesTable::builtin();
        for (i, lib) in LIBRARY.iter().enumerate() {
            let id = table.by_name(lib.name).unwrap();
            assert_eq!(id.index(), 1 + SIZE_CLASS_NAMES.len() + i);
            let s = table.species(id);
            let close = |a: f32, b: f64| ((a as f64) - b).abs() < 1e-6 * b;
            assert!(close(s.sigma, lib.sigma_angstrom / AR_SIGMA_ANGSTROM));
            assert!(close(s.epsilon, lib.epsilon_kelvin / AR_EPSILON_KELVIN));
            assert!(close(s.mass, lib.mass_u / AR_MASS_U));
            assert_eq!(s.display_radius, 0.5 * s.sigma);
            assert_eq!(s.colour, lib.colour);
            assert!(!lib.source.is_empty());
        }
    }

    /// CH₄ (TraPPE-UA) and NH₃ (Svehla 1962) in Ar units, as listed in
    /// `docs/units.md` (#409); the former CH₄ stays as `CH4_HCB`.
    #[test]
    fn ch4_and_nh3_have_their_documented_reduced_values() {
        let table = SpeciesTable::builtin();
        let close = |a: f32, b: f32| (a - b).abs() < 5e-5;
        for (name, sigma, epsilon, mass) in [
            ("CH4", 1.0954, 1.2354, 0.4016),
            ("NH3", 0.8517, 4.6603, 0.4263),
            ("CH4_HCB", 1.1210, 1.2371, 0.4016),
        ] {
            let s = table.species(table.by_name(name).unwrap());
            assert!(
                close(s.sigma, sigma) && close(s.epsilon, epsilon) && close(s.mass, mass),
                "{name}: {} {} {}",
                s.sigma,
                s.epsilon,
                s.mass
            );
        }
    }

    /// The built-in table is part of the save format: a file names a species,
    /// and its mass, `σ` and `ε` come from this table when the file is loaded.
    /// If this test fails, read "Built-in species" in the `save-format` skill
    /// before updating it: renaming or changing a species needs a format
    /// version bump and a migration; otherwise add a new species instead.
    #[test]
    fn builtin_table_is_pinned() {
        // (name, f32 bits of mass, sigma, epsilon, display_radius), in table
        // order, in Ar units (#405, format v6; #409, v7: TraPPE CH4, NH3,
        // the former CH4 as CH4_HCB; #349: MB water H2O; #406: N2_2S; #407: H2_2S; #408: CO2_3S).
        const PINNED: [(&str, u32, u32, u32, u32); 16] = [
            ("N2", 0x3f3384e6, 0x3f8b03af, 0x3f4b1ca2, 0x3f0b03af), // 0.7012466, 1.08605, 0.7934057
            ("XS", 0x3f3384e6, 0x3fb02b31, 0x3f4b1ca2, 0x3f302b31), // 0.7012466, 1.376318, 0.7934057
            ("S", 0x3f576c48, 0x3fd552b2, 0x3f4b1ca2, 0x3f5552b2),  // 0.841496, 1.666586, 0.7934057
            ("M", 0x3f9cd96e, 0x400fd0db, 0x3f4b1ca2, 0x3f8fd0db),  // 1.225385, 2.247123, 0.7934057
            ("L", 0x40099aea, 0x405a1fde, 0x3f4b1ca2, 0x3fda1fde),  // 2.15008, 3.408195, 0.7934057
            ("XL", 0x408a25a6, 0x40b75ef2, 0x3f4b1ca2, 0x40375ef2), // 4.317096, 5.73034, 0.7934057
            ("Ar", 0x3f800000, 0x3f800000, 0x3f800000, 0x3f000000), // 1, 1, 1
            ("Ne", 0x3f0151f3, 0x3f4eadfa, 0x3e98259c, 0x3eceadfa), // 0.5051567, 0.8073422, 0.2971619
            ("CH4", 0x3ecd9e22, 0x3f8c37a3, 0x3f9e2156, 0x3f0c37a3), // 0.4015971, 1.095448, 1.235392
            ("O2", 0x3f4d0f68, 0x3f86941d, 0x3f7b15cc, 0x3f06941d), // 0.8010163, 1.051395, 0.9808013
            ("NH3", 0x3eda47d3, 0x3f5a0845, 0x409520e8, 0x3eda0845), // 0.4263292, 0.8516887, 4.660267
            ("CH4_HCB", 0x3ecd9e22, 0x3f8f7ce1, 0x3f9e580a, 0x3f0f7ce1), // 0.4015971, 1.120999, 1.237062
            ("H2O", 0x3ee6e465, 0x3f135c1a, 0x3f93fadf, 0x3ed28392), // 0.4509613, 0.5756241, 1.156093
            ("N2_2S", 0x3f3385e2, 0x3fa5c79d, 0x00000000, 0x3ef8db89), // 0.7012616, 1.295154, 0
            ("H2_2S", 0x3d4eb506, 0x3f82114b, 0x00000000, 0x3ecc7fd0), // 0.05046561, 1.016153, 0
            ("CO2_3S", 0x3f8d031a, 0x3fc9de2b, 0x00000000, 0x3ee54f52), // 1.101657, 1.577093, 0
        ];
        let actual: Vec<_> = SpeciesTable::builtin()
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.mass.to_bits(),
                    s.sigma.to_bits(),
                    s.epsilon.to_bits(),
                    s.display_radius.to_bits(),
                )
            })
            .collect();
        assert_eq!(
            actual, PINNED,
            "the built-in species changed; this breaks saved worlds (see the save-format skill)"
        );
    }

    #[test]
    fn species_set_holds_ids() {
        let set: SpeciesSet = [SpeciesId::new(3), SpeciesId::new(0)].into_iter().collect();
        assert!(set.contains(SpeciesId::new(0)) && set.contains(SpeciesId::new(3)));
        assert!(!set.contains(SpeciesId::new(1)));
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [SpeciesId::new(0), SpeciesId::new(3)]
        );
    }
}
