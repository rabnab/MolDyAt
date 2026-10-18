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
//! The built-in table holds `N2`, the default and the reference of the reduced
//! units (`σ = ε = m = 1`), and the five former size classes `XS` to `XL`. A
//! size class of radius `R` became the species with
//! `σ = 2·R·2^(-1/6) + σ_N2`: with arithmetic mixing every pair then keeps its
//! old equilibrium centre distance `R_i + R_j + 2^(1/6)·σ_N2` (see
//! `docs/simulation.md`).

use std::sync::LazyLock;

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

static BUILTIN_TABLE: LazyLock<SpeciesTable> = LazyLock::new(|| {
    let n2 = Species::new("N2", 1.0, N2_SIGMA, 1.0, [0.0, 1.0, 1.0, 1.0]);
    let size_classes = SIZE_CLASSES.iter().enumerate().map(|(i, &(name, colour))| {
        let radius = SIZE_CLASS_MIN_RADIUS * 2.0_f32.powi(i as i32);
        let sigma = 2.0 * radius * INV_SIXTH_ROOT_OF_TWO + N2_SIGMA;
        Species::new(name, size_class_mass(radius), sigma, 1.0, colour)
    });
    SpeciesTable::new(std::iter::once(n2).chain(size_classes).collect())
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

    /// The built-in species: `N2` and the former size classes `XS` to `XL`.
    pub fn builtin() -> &'static SpeciesTable {
        &BUILTIN_TABLE
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
        SpeciesId::new(((id.index() + 1) % self.species.len()) as u16)
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
        assert_eq!(table.next(SpeciesId::new(4)), SpeciesId::new(5));
        assert_eq!(table.next(SpeciesId::new(5)), SpeciesId::new(0));
    }

    #[test]
    fn id_rejects_indices_outside_the_table() {
        let table = SpeciesTable::builtin();
        assert_eq!(table.id(5), Some(SpeciesId::new(5)));
        assert_eq!(table.id(6), None);
        assert!(table.get(SpeciesId::new(6)).is_none());
        assert_eq!(table.by_name("M"), Some(SpeciesId::new(3)));
        assert_eq!(table.by_name("M"), Some(SPECIES_M));
        assert_eq!(table.by_name("Ar"), None);
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
