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
//! The built-in table (the species library, #237) holds `N2`, the default and
//! the reference of the reduced units (`σ = ε = m = 1`), the five former size
//! classes `XS` to `XL`, and the real gases `Ar`, `Ne`, `CH4` and `O2`
//! (single-site Lennard-Jones from the literature, see [`LIBRARY`]). A size
//! class of radius `R` became the species with `σ = 2·R·2^(-1/6) + σ_N2`: with
//! arithmetic mixing every pair then keeps its old equilibrium centre distance
//! `R_i + R_j + 2^(1/6)·σ_N2` (see `docs/simulation.md`). Old saves migrate to
//! these size classes, so they stay in the table.
//!
//! A config file can replace the whole table with its own `[[species]]` list
//! (`SimConfig::species_table`); a world keeps the table it was built with.

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
}

impl Species {
    /// A species drawn at its default size, `σ / 2`.
    pub fn new(name: &str, mass: f32, sigma: f32, epsilon: f32, colour: [f32; 4]) -> Self {
        Species {
            name: name.to_string(),
            mass,
            sigma,
            epsilon,
            display_radius: 0.5 * sigma,
            colour,
        }
    }

    /// A species from its literature single-site Lennard-Jones parameters
    /// (`σ` in Å, `ε / k_B` in K, mass in u), in N₂ units. `display_radius`
    /// in Å; `None` draws it at `σ / 2`.
    pub fn from_literature(
        name: &str,
        sigma_angstrom: f64,
        epsilon_kelvin: f64,
        mass_u: f64,
        display_radius_angstrom: Option<f64>,
        colour: [f32; 4],
    ) -> Self {
        let sigma = (sigma_angstrom / N2_SIGMA_ANGSTROM) as f32;
        let mut species = Species::new(
            name,
            (mass_u / N2_MASS_U) as f32,
            sigma,
            (epsilon_kelvin / N2_EPSILON_KELVIN) as f32,
            colour,
        );
        if let Some(radius) = display_radius_angstrom {
            species.display_radius = (radius / N2_SIGMA_ANGSTROM) as f32;
        }
        species
    }

    /// Half of `σ`: the size of a ball for contact and overlap checks. Two
    /// balls touch (zero pair potential) when their centres are the sum of
    /// their contact radii apart, `σ_ij` under arithmetic mixing.
    pub fn contact_radius(&self) -> f32 {
        0.5 * self.sigma
    }
}

/// The species a world uses, indexed by [`SpeciesId`]. Holds at least one and
/// at most [`SpeciesSet::CAPACITY`] species.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeciesTable {
    species: Vec<Species>,
}

/// `σ` of `N2`, the length unit.
pub(crate) const N2_SIGMA: f32 = 1.0;

/// Single-site Lennard-Jones `σ` of N₂ in Å: the reduced unit of length
/// (`UnitSystem::N2`). Hirschfelder, Curtiss and Bird, *Molecular Theory of
/// Gases and Liquids* (Wiley, 1954), table I-A, from second virial
/// coefficients.
pub const N2_SIGMA_ANGSTROM: f64 = 3.698;
/// `ε / k_B` of N₂ in K: the reduced unit of temperature (same source).
pub const N2_EPSILON_KELVIN: f64 = 95.05;
/// Molecular mass of N₂ in u: the reduced unit of mass.
pub const N2_MASS_U: f64 = 28.0134;

/// Source of the library gases' `σ` and `ε`.
const HCB_1954: &str = "Hirschfelder, Curtiss & Bird, Molecular Theory of Gases and Liquids \
     (Wiley, 1954), table I-A, second virial coefficients; mass: IUPAC standard atomic weights";

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
pub const LIBRARY: [LibrarySpecies; 4] = [
    LibrarySpecies {
        name: "Ar",
        sigma_angstrom: 3.405,
        epsilon_kelvin: 119.8,
        mass_u: 39.948,
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
        sigma_angstrom: 3.817,
        epsilon_kelvin: 148.2,
        mass_u: 16.043,
        colour: [0.55, 0.85, 0.35, 1.0], // light green
        source: HCB_1954,
    },
    LibrarySpecies {
        name: "O2",
        sigma_angstrom: 3.58,
        epsilon_kelvin: 117.5,
        mass_u: 31.999,
        colour: [0.95, 0.45, 0.55, 1.0], // pink
        source: HCB_1954,
    },
];
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

/// The former size class `M` (contact radius ≈ 1.03): for tests written
/// around balls of radius 1.
#[cfg(test)]
pub(crate) const SPECIES_M: SpeciesId = SpeciesId::new(3);

static BUILTIN_TABLE: LazyLock<Arc<SpeciesTable>> = LazyLock::new(|| {
    let n2 = Species::new("N2", 1.0, N2_SIGMA, 1.0, [0.0, 1.0, 1.0, 1.0]);
    let size_classes = SIZE_CLASSES.iter().enumerate().map(|(i, &(name, colour))| {
        let radius = SIZE_CLASS_MIN_RADIUS * 2.0_f32.powi(i as i32);
        let sigma = 2.0 * radius * INV_SIXTH_ROOT_OF_TWO + N2_SIGMA;
        Species::new(name, size_class_mass(radius), sigma, 1.0, colour)
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
            .collect(),
    ))
});

/// Mass of the former size classes: grows slowly with the radius.
fn size_class_mass(radius: f32) -> f32 {
    let density = 2. / 10.;
    1. + density * (radius / SIZE_CLASS_MIN_RADIUS - 1.).powf(1.2)
}

impl SpeciesTable {
    /// Panics if `species` is empty or longer than [`SpeciesSet::CAPACITY`].
    pub fn new(species: Vec<Species>) -> Self {
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

    pub fn iter(&self) -> impl Iterator<Item = &Species> {
        self.species.iter()
    }

    pub fn len(&self) -> usize {
        self.species.len()
    }

    pub fn is_empty(&self) -> bool {
        self.species.is_empty()
    }

    /// The species after `id`, wrapping around: what the UI steps through.
    pub fn next(&self, id: SpeciesId) -> SpeciesId {
        let len = self.species.len();
        SpeciesId::new(((id.index() % len + 1) % len) as u16)
    }

    /// The species before `id`, wrapping around.
    pub fn previous(&self, id: SpeciesId) -> SpeciesId {
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

    pub fn insert(&mut self, id: SpeciesId) {
        self.0 |= 1 << id.index();
    }

    pub fn contains(&self, id: SpeciesId) -> bool {
        self.0 & (1 << id.index()) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The ids in the set, ascending.
    pub fn iter(&self) -> impl Iterator<Item = SpeciesId> {
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
    fn n2_is_the_default_and_the_unit() {
        let n2 = SpeciesTable::builtin().species(DEFAULT_SPECIES);
        assert_eq!(n2.name, "N2");
        assert_eq!((n2.mass, n2.sigma, n2.epsilon), (1.0, 1.0, 1.0));
        assert_eq!(n2.display_radius, 0.5);
    }

    /// A former size class keeps its mass and colour, and every pair of them
    /// keeps its old equilibrium centre distance `R_i + R_j + 2^(1/6)`.
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
        for (i, name) in SIZE_CLASS_NAMES.iter().enumerate() {
            let s = table.species(table.by_name(name).unwrap());
            assert_eq!(s.mass.to_bits(), masses[i].to_bits(), "mass {name}");
            assert_eq!(s.colour, colours[i], "colour {name}");
            assert_eq!(s.display_radius, 0.5 * s.sigma);
            for (j, other) in SIZE_CLASS_NAMES.iter().enumerate() {
                let o = table.species(table.by_name(other).unwrap());
                let sigma_ij = 0.5 * (s.sigma + o.sigma);
                let old = SIZE_CLASS_RADII[i] + SIZE_CLASS_RADII[j] + sixth_root_of_two;
                let new = sixth_root_of_two * sigma_ij;
                assert!((new - old).abs() < 1e-5, "{name}-{other}: {new} vs {old}");
            }
        }
    }

    #[test]
    fn next_wraps_around() {
        let table = SpeciesTable::builtin();
        assert_eq!(table.next(SpeciesId::new(5)), SpeciesId::new(6));
        assert_eq!(table.next(SpeciesId::new(9)), SpeciesId::new(0));
        assert_eq!(table.previous(SpeciesId::new(0)), SpeciesId::new(9));
        assert_eq!(table.previous(SpeciesId::new(6)), SpeciesId::new(5));
        // An id outside the table wraps into it in both directions.
        assert_eq!(table.next(SpeciesId::new(13)), SpeciesId::new(4));
        assert_eq!(table.previous(SpeciesId::new(13)), SpeciesId::new(2));
    }

    #[test]
    fn id_rejects_indices_outside_the_table() {
        let table = SpeciesTable::builtin();
        assert_eq!(table.id(9), Some(SpeciesId::new(9)));
        assert_eq!(table.id(10), None);
        assert!(table.get(SpeciesId::new(10)).is_none());
        assert_eq!(table.by_name("M"), Some(SpeciesId::new(3)));
        assert_eq!(table.by_name("M"), Some(SPECIES_M));
        assert_eq!(table.by_name("Ar"), Some(SpeciesId::new(6)));
        assert_eq!(table.by_name("Kr"), None);
    }

    /// N₂ in literature units is exactly the unit species.
    #[test]
    fn n2_from_literature_is_the_unit() {
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

    /// The library gases are their literature values in N₂ units, drawn at
    /// `σ / 2`, after `N2` and the size classes.
    #[test]
    fn library_gases_follow_their_literature_values() {
        let table = SpeciesTable::builtin();
        for (i, lib) in LIBRARY.iter().enumerate() {
            let id = table.by_name(lib.name).unwrap();
            assert_eq!(id.index(), 1 + SIZE_CLASS_NAMES.len() + i);
            let s = table.species(id);
            let close = |a: f32, b: f64| ((a as f64) - b).abs() < 1e-6 * b;
            assert!(close(s.sigma, lib.sigma_angstrom / N2_SIGMA_ANGSTROM));
            assert!(close(s.epsilon, lib.epsilon_kelvin / N2_EPSILON_KELVIN));
            assert!(close(s.mass, lib.mass_u / N2_MASS_U));
            assert_eq!(s.display_radius, 0.5 * s.sigma);
            assert_eq!(s.colour, lib.colour);
            assert!(!lib.source.is_empty());
        }
    }

    /// The built-in table is part of the save format: a file names a species,
    /// and its mass, `σ` and `ε` come from this table when the file is loaded.
    /// If this test fails, read "Built-in species" in the `save-format` skill
    /// before updating it: renaming or changing a species needs a format
    /// version bump and a migration; otherwise add a new species instead.
    #[test]
    fn builtin_table_is_pinned() {
        // (name, f32 bits of mass, sigma, epsilon, display_radius), in table order.
        const PINNED: [(&str, u32, u32, u32, u32); 10] = [
            ("N2", 0x3f800000, 0x3f800000, 0x3f800000, 0x3f000000), // 1, 1, 1, 0.5
            ("XS", 0x3f800000, 0x3fa235e4, 0x3f800000, 0x3f2235e4), // 1, 1.2672696
            ("S", 0x3f99999a, 0x3fc46bc8, 0x3f800000, 0x3f446bc8),  // 1.2, 1.5345392
            ("M", 0x3fdfac12, 0x40046bc8, 0x3f800000, 0x3f846bc8),  // 1.7474387, 2.0690784
            ("L", 0x40443ab3, 0x4048d790, 0x3f800000, 0x3fc8d790),  // 3.0660827, 3.138157
            ("XL", 0x40c5008b, 0x40a8d790, 0x3f800000, 0x4028d790), // 6.1563163, 5.276314
            ("Ar", 0x3fb68836, 0x3f6bb773, 0x3fa15470, 0x3eebb773), // 1.426032, 0.920768, 1.260389
            ("Ne", 0x3f386a23, 0x3f3e4dd0, 0x3ebfc3ab, 0x3ebe4dd0), // 0.7203695, 0.7433748, 0.3745397
            ("CH4", 0x3f129bd3, 0x3f841e76, 0x3fc79331, 0x3f041e76), // 0.5726902, 1.03218, 1.559179
            ("O2", 0x3f92360f, 0x3f77d4cd, 0x3f9e3b86, 0x3ef7d4cd), // 1.142275, 0.9680908, 1.236192
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
