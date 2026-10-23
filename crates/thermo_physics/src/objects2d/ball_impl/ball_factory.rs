// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::ball_impl::BallId;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::species::{DEFAULT_SPECIES, SpeciesId, SpeciesTable};
use crate::objects2d::vec_2d::Vec2D;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

use rand::rngs::{StdRng, SysRng};
use rand::{RngExt, SeedableRng};

/// Factory for creating Ball2D instances with various configurations.
///
/// It owns the species table of the world it makes balls for (injected at
/// construction, swapped only through [`Self::use_species`]), so
/// `new_ball_species` always indexes that table.
pub struct BallFactory {
    /// The world's species table: mass and contact radius of new balls.
    species: RefCell<Arc<SpeciesTable>>,
    next_id: RefCell<usize>,
    new_ball_species: Cell<SpeciesId>,
    rng: RefCell<StdRng>,
}

const FIRST_BALL_ID: usize = 1;

/// A factory for the built-in species table, seeded from entropy.
impl Default for BallFactory {
    fn default() -> Self {
        Self::with_species(SpeciesTable::builtin_shared(), None)
    }
}

impl BallFactory {
    /// [`Self::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// A factory for the built-in species table with deterministic RNG state
    /// from a fixed seed.
    #[cfg(test)]
    pub(crate) fn with_seed(seed: u64) -> Self {
        Self::with_species(SpeciesTable::builtin_shared(), Some(seed))
    }

    /// A factory for balls of `species` (the world's table); its RNG is
    /// seeded from `seed` if given, else from entropy.
    pub(crate) fn with_species(species: Arc<SpeciesTable>, seed: Option<u64>) -> Self {
        let rng = match seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::try_from_rng(&mut SysRng).unwrap(),
        };
        BallFactory {
            species: RefCell::new(species),
            next_id: RefCell::new(FIRST_BALL_ID),
            new_ball_species: Cell::new(DEFAULT_SPECIES),
            rng: RefCell::new(rng),
        }
    }

    /// Make balls of `species` from now on (the world was loaded or
    /// replaced). New balls go back to the default species if the table
    /// differs, so `new_ball_species` always indexes the factory's table.
    pub(crate) fn use_species(&self, species: Arc<SpeciesTable>) {
        if **self.species.borrow() != *species {
            self.new_ball_species.set(DEFAULT_SPECIES);
        }
        *self.species.borrow_mut() = species;
    }

    /// Switch new balls to the next species of the table.
    pub(crate) fn next_species(&self) {
        let next = self.species.borrow().next(self.new_ball_species.get());
        self.new_ball_species.set(next);
    }

    /// Switch new balls to the previous species of the table.
    pub(crate) fn previous_species(&self) {
        let previous = self.species.borrow().previous(self.new_ball_species.get());
        self.new_ball_species.set(previous);
    }

    /// The species of new balls.
    pub fn new_ball_species(&self) -> SpeciesId {
        self.new_ball_species.get()
    }

    /// Contact radius (`σ / 2`) of the species of new balls.
    pub(crate) fn get_new_ball_radius(&self) -> f32 {
        self.species
            .borrow()
            .species(self.new_ball_species.get())
            .contact_radius()
    }

    /// A ball of `species` at rest, with the next free id.
    pub(crate) fn create_basic(&self, pos: impl Into<Vec2D>, species: SpeciesId) -> Ball2D {
        let id = *self.next_id.borrow();
        *self.next_id.borrow_mut() += 1;
        Ball2D::of_species(
            id,
            pos.into(),
            species,
            self.species.borrow().species(species),
        )
    }

    /// Creates a ball with specified mass and initial velocity
    pub fn create_massive_with_velocity(
        &self,
        pos: impl Into<Vec2D>,
        initial_velocity: Vec2D,
    ) -> Ball2D {
        self.create_with_species_and_velocity(pos, self.new_ball_species(), initial_velocity)
    }

    /// Creates a ball of a given species with an initial velocity.
    pub(crate) fn create_with_species_and_velocity(
        &self,
        pos: impl Into<Vec2D>,
        species: SpeciesId,
        velocity: Vec2D,
    ) -> Ball2D {
        let mut ball = self.create_basic(pos, species);
        ball.v = velocity;
        ball
    }

    /// Creates a ball with random properties within specified ranges.
    /// Position is uniformly distributed, velocity has random direction.
    ///
    /// # Arguments
    /// * `species` - Species of the ball, of the factory's table (callers
    ///   pass it explicitly instead of changing the species of new balls)
    /// * `pos_range` - Min and max position bounds (x and y)
    /// * `velocity_range` - Speed range (min, max)
    ///
    /// # Returns
    /// New ball with random position and velocity
    pub fn create_random(
        &self,
        species: SpeciesId,
        pos_range: (&Vec2D, &Vec2D),
        velocity_range: (f32, f32),
    ) -> Ball2D {
        let mut rng = self.rng.borrow_mut();

        let min_x = f32::min(pos_range.0.x, pos_range.1.x);
        let max_x = f32::max(pos_range.0.x, pos_range.1.x);
        let min_y = f32::min(pos_range.0.y, pos_range.1.y);
        let max_y = f32::max(pos_range.0.y, pos_range.1.y);

        let pos = Vec2D::new(
            rng.random_range(min_x..=max_x),
            rng.random_range(min_y..=max_y),
        );

        let velocity = if velocity_range.0 < velocity_range.1 {
            let speed = rng.random_range(velocity_range.0..=velocity_range.1);
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            Vec2D::new(speed * angle.cos(), speed * angle.sin())
        } else {
            Vec2D::zero()
        };

        self.create_with_species_and_velocity(pos, species, velocity)
    }

    /// A direction angle in `0..TAU` from the factory's RNG, so a seeded
    /// factory gives a reproducible sequence.
    pub(crate) fn random_angle(&self) -> f32 {
        self.rng
            .borrow_mut()
            .random_range(0.0..std::f32::consts::TAU)
    }

    /// Creates a ball with specified properties and validation
    #[cfg(test)]
    pub(crate) fn create_validated(
        &self,
        pos: impl Into<Vec2D>,
        velocity: Vec2D,
    ) -> Result<Ball2D, String> {
        let pos_vec = pos.into();
        if !pos_vec.is_finite() || !velocity.is_finite() {
            return Err("Position and velocity components must be finite".to_string());
        }
        Ok(self.create_massive_with_velocity(pos_vec, velocity))
    }

    /// Resets the ID counter - useful for testing or when creating new simulation
    pub(crate) fn reset_id_counter(&self) {
        *self.next_id.borrow_mut() = FIRST_BALL_ID;
    }

    /// Continue numbering after every id in `ids` (the balls of a loaded world).
    pub(crate) fn continue_after(&self, ids: impl IntoIterator<Item = BallId>) {
        let next = ids
            .into_iter()
            .map(|id| id + 1)
            .max()
            .unwrap_or(FIRST_BALL_ID);
        *self.next_id.borrow_mut() = next.max(FIRST_BALL_ID);
    }

    /// Make new balls of `species`.
    #[cfg(test)]
    pub(crate) fn set_new_ball_species(&self, species: SpeciesId) {
        self.new_ball_species.set(species);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_ball_creation() {
        let factory = BallFactory::new();
        let pos = Vec2D::new(1.0, 2.0);
        let species = SpeciesId::new(0);
        let ball = factory.create_basic(pos, species);

        let expected = SpeciesTable::builtin().species(species);
        assert_eq!(ball.get_id(), 1);
        assert_eq!(ball.species(), species);
        assert_eq!(*ball.get_position(), pos);
        assert_eq!(ball.get_contact_radius(), expected.contact_radius());
        assert_eq!(ball.get_mass(), expected.mass);
        assert_eq!(*ball.get_velocity(), Vec2D::zero());
    }

    #[test]
    fn test_massive_ball_creation() {
        let factory = BallFactory::new();
        let pos = Vec2D::new(1.0, 2.0);
        let velocity = Vec2D::new(3.0, 4.0);
        let ball = factory.create_massive_with_velocity(pos, velocity);

        assert_eq!(ball.get_id(), 1);
        assert_eq!(*ball.get_position(), pos);
        assert_eq!(*ball.get_velocity(), velocity);
    }

    #[test]
    fn test_validated_ball_creation() {
        let factory = BallFactory::new();

        assert!(
            factory
                .create_validated(Vec2D::new(f32::NAN, 0.), Vec2D::zero())
                .is_err()
        );

        assert!(
            factory
                .create_validated(Vec2D::zero(), Vec2D::new(f32::NAN, 0.))
                .is_err()
        );

        let rad_expect = factory.get_new_ball_radius();
        // Test valid parameters
        let ball = factory
            .create_validated(Vec2D::zero(), Vec2D::zero())
            .unwrap();
        assert_eq!(ball.get_contact_radius(), rad_expect);
    }

    #[test]
    fn test_random_ball_creation() {
        let factory = BallFactory::new();
        let pos_range = (&Vec2D::new(0.0, 0.0), &Vec2D::new(10.0, 10.0));
        let velocity_range = (0.0, 5.0);
        let ball = factory.create_random(DEFAULT_SPECIES, pos_range, velocity_range);

        // Check if values are within ranges
        assert!(ball.get_position().x >= pos_range.0.x && ball.get_position().x <= pos_range.1.x);
        assert!(ball.get_position().y >= pos_range.0.y && ball.get_position().y <= pos_range.1.y);
    }
}

#[cfg(test)]
mod species_tests {
    use super::*;
    use crate::objects2d::ball_impl::Species;

    fn two_species() -> Arc<SpeciesTable> {
        Arc::new(SpeciesTable::new(vec![
            Species::new("A", 2.0, 1.0, 1.0, [1.0; 4]),
            Species::new("B", 3.0, 2.0, 1.0, [1.0; 4]),
        ]))
    }

    /// Balls take mass and radius from the injected table.
    #[test]
    fn balls_follow_the_injected_table() {
        let factory = BallFactory::with_species(two_species(), Some(1));
        factory.next_species();
        let ball = factory.create_massive_with_velocity(Vec2D::zero(), Vec2D::zero());
        assert_eq!((ball.get_mass(), ball.get_contact_radius()), (3.0, 1.0));
        factory.next_species();
        assert_eq!(factory.new_ball_species(), DEFAULT_SPECIES, "wraps at 2");
    }

    /// `use_species` keeps the species of new balls for the same table and
    /// resets it for another one.
    #[test]
    fn use_species_resets_the_species_of_new_balls_only_for_another_table() {
        let factory = BallFactory::new();
        factory.next_species();
        factory.use_species(SpeciesTable::builtin_shared());
        assert_eq!(factory.new_ball_species(), SpeciesId::new(1));
        factory.use_species(two_species());
        assert_eq!(factory.new_ball_species(), DEFAULT_SPECIES);
        assert_eq!(factory.get_new_ball_radius(), 0.5);
    }
}
