// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Particle types (species): the per-type data every ball of that type shares.
//!
//! A ball stores only its [`SpeciesId`]; mass, radius and colour come from the
//! [`SpeciesTable`]. The built-in table holds the five former size classes, so
//! species `i` is size class `i` with the same radius, mass and colour.
//!
//! Interaction still uses the edge distance with the global `σ` of the pair
//! potential, so `radius` is a physics value here. #234 moves Lennard-Jones to
//! the centre distance with a per-species `σ`; from then on only
//! `display_radius` is about size, and it is for drawing only.

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
    /// Physics radius: interaction, wall and placement checks work on the
    /// edge distance, the centre distance minus both radii.
    pub radius: f32,
    /// Radius the renderer draws. Has no effect on physics.
    pub display_radius: f32,
    /// RGBA in `[0, 1]`.
    pub colour: [f32; 4],
}

/// The species a world uses, indexed by [`SpeciesId`].
#[derive(Clone, Debug, PartialEq)]
pub struct SpeciesTable {
    species: Vec<Species>,
}

/// Radius of the smallest built-in species. Each further one doubles it.
const BUILTIN_MIN_RADIUS: f32 = 0.15;
const BUILTIN: [(&str, [f32; 4]); 5] = [
    ("XS", [1.0, 0.0, 0.0, 1.0]), // red
    ("S", [0.0, 1.0, 0.0, 1.0]),  // green
    ("M", [0.0, 0.0, 1.0, 1.0]),  // blue
    ("L", [1.0, 1.0, 0.0, 1.0]),  // yellow
    ("XL", [1.0, 0.0, 1.0, 1.0]), // magenta
];
/// Species new balls and new source spheres start with (`M`).
pub const DEFAULT_SPECIES: SpeciesId = SpeciesId::new(2);

static BUILTIN_TABLE: LazyLock<SpeciesTable> = LazyLock::new(|| {
    let species = BUILTIN
        .iter()
        .enumerate()
        .map(|(i, &(name, colour))| {
            let radius = BUILTIN_MIN_RADIUS * 2.0_f32.powi(i as i32);
            Species {
                name: name.to_string(),
                mass: builtin_mass(radius),
                radius,
                display_radius: radius,
                colour,
            }
        })
        .collect();
    SpeciesTable { species }
});

/// Mass of the former size classes: grows slowly with the radius.
fn builtin_mass(radius: f32) -> f32 {
    let density = 2. / 10.;
    1. + density * (radius / BUILTIN_MIN_RADIUS - 1.).powf(1.2)
}

impl SpeciesTable {
    /// The built-in species (the former size classes `XS` to `XL`).
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

    /// The largest physics radius, which sizes the cell grid and the
    /// neighbour search.
    pub fn max_radius(&self) -> f32 {
        self.species.iter().map(|s| s.radius).fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in species reproduce the former size classes bit for bit, so
    /// replacing `size_idx` by a species changes no simulation result.
    #[test]
    fn builtin_species_match_the_former_size_classes() {
        let table = SpeciesTable::builtin();
        assert_eq!(table.len(), 5);
        let radii = [0.15_f32, 0.3, 0.6, 1.2, 2.4];
        // `1 + 0.2 * (r / 0.15 - 1)^1.2`, evaluated by the former
        // `BallFactory::mass_for_size_idx`.
        let masses = [1.0_f32, 1.2, 1.7474387, 3.0660827, 6.1563163];
        for (i, s) in table.species.iter().enumerate() {
            assert_eq!(s.radius.to_bits(), radii[i].to_bits(), "radius {i}");
            assert_eq!(s.mass.to_bits(), masses[i].to_bits(), "mass {i}");
            assert_eq!(s.display_radius, s.radius);
        }
        assert_eq!(
            table.species(SpeciesId::new(0)).colour,
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            table.species(SpeciesId::new(4)).colour,
            [1.0, 0.0, 1.0, 1.0]
        );
        assert_eq!(table.max_radius(), 2.4);
    }

    #[test]
    fn next_wraps_around() {
        let table = SpeciesTable::builtin();
        assert_eq!(table.next(SpeciesId::new(3)), SpeciesId::new(4));
        assert_eq!(table.next(SpeciesId::new(4)), SpeciesId::new(0));
    }

    #[test]
    fn id_rejects_indices_outside_the_table() {
        let table = SpeciesTable::builtin();
        assert_eq!(table.id(4), Some(SpeciesId::new(4)));
        assert_eq!(table.id(5), None);
        assert!(table.get(SpeciesId::new(5)).is_none());
    }
}
