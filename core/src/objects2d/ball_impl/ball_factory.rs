// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::ball_impl::BallId;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::species::{DEFAULT_SPECIES, SpeciesId, SpeciesTable};
use crate::objects2d::vec_2d::Vec2D;
use std::cell::{Cell, RefCell};

use rand::rngs::{StdRng, SysRng};
use rand::{RngExt, SeedableRng};
use rand_distr::{Distribution, Normal};

/// Factory for creating Ball2D instances with various configurations.
///
/// It holds the id counter, the RNG and the species of new balls, but no
/// species table: every method that needs species data (mass, contact radius,
/// the species after the current one) takes the world's table, which must be
/// the table `new_ball_species` indexes.
pub struct BallFactory {
    next_id: RefCell<usize>,
    new_ball_species: Cell<SpeciesId>,
    rng: RefCell<StdRng>,
}

const FIRST_BALL_ID: usize = 1;

impl Default for BallFactory {
    fn default() -> Self {
        BallFactory {
            next_id: RefCell::new(FIRST_BALL_ID),
            new_ball_species: Cell::new(DEFAULT_SPECIES),
            rng: RefCell::new(StdRng::try_from_rng(&mut SysRng).unwrap()),
        }
    }
}

impl BallFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a factory with deterministic RNG state from a fixed seed.
    pub fn with_seed(seed: u64) -> Self {
        BallFactory {
            next_id: RefCell::new(FIRST_BALL_ID),
            new_ball_species: Cell::new(DEFAULT_SPECIES),
            rng: RefCell::new(StdRng::seed_from_u64(seed)),
        }
    }

    /// Switch new balls to the species after the current one in `table`.
    pub(crate) fn next_species(&self, table: &SpeciesTable) {
        self.new_ball_species
            .set(table.next(self.new_ball_species.get()));
    }

    /// Switch new balls to `species` (of the world's table).
    pub(crate) fn set_new_ball_species(&self, species: SpeciesId) {
        self.new_ball_species.set(species);
    }

    /// The species of new balls.
    pub(crate) fn new_ball_species(&self) -> SpeciesId {
        self.new_ball_species.get()
    }

    /// Contact radius (`σ / 2`) of the species of new balls, from `table`.
    pub(crate) fn get_new_ball_radius(&self, table: &SpeciesTable) -> f32 {
        table.species(self.new_ball_species.get()).contact_radius()
    }

    /// A ball of `species` (of `table`) at rest, with the next free id.
    pub(crate) fn create_basic(
        &self,
        table: &SpeciesTable,
        pos: impl Into<Vec2D>,
        species: SpeciesId,
    ) -> Ball2D {
        let id = *self.next_id.borrow();
        *self.next_id.borrow_mut() += 1;
        Ball2D::of_species(id, pos.into(), species, table.species(species))
    }

    /// A ball of the species of new balls with an initial velocity.
    pub fn create_massive_with_velocity(
        &self,
        table: &SpeciesTable,
        pos: impl Into<Vec2D>,
        initial_velocity: Vec2D,
    ) -> Ball2D {
        self.create_with_species_and_velocity(table, pos, self.new_ball_species(), initial_velocity)
    }

    /// Creates a ball of a given species with an initial velocity.
    pub fn create_with_species_and_velocity(
        &self,
        table: &SpeciesTable,
        pos: impl Into<Vec2D>,
        species: SpeciesId,
        velocity: Vec2D,
    ) -> Ball2D {
        let mut ball = self.create_basic(table, pos, species);
        ball.v = velocity;
        ball
    }

    /// Creates a ball with random properties within specified ranges.
    /// Position is uniformly distributed, velocity has random direction.
    ///
    /// # Arguments
    /// * `pos_range` - Min and max position bounds (x and y)
    /// * `velocity_range` - Speed range (min, max)
    ///
    /// # Returns
    /// New ball with random position and velocity
    pub fn create_random(
        &self,
        table: &SpeciesTable,
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

        self.create_massive_with_velocity(table, pos, velocity)
    }

    /// A direction angle in `0..TAU` from the factory's RNG, so a seeded
    /// factory gives a reproducible sequence.
    pub(crate) fn random_angle(&self) -> f32 {
        self.rng
            .borrow_mut()
            .random_range(0.0..std::f32::consts::TAU)
    }

    /// Creates a collection of balls with normally distributed positions (Gaussian cluster).
    ///
    /// # Arguments
    /// * `count` - Number of balls to create
    /// * `center_pos` - Mean position (cluster center)
    /// * `pos_std_dev` - Standard deviation for position distribution
    ///
    /// # Returns
    /// Vector of balls with zero initial velocity
    pub fn create_normal_distribution(
        &self,
        table: &SpeciesTable,
        count: usize,
        center_pos: Vec2D,
        pos_std_dev: f32,
    ) -> Vec<Ball2D> {
        let mut rng = self.rng.borrow_mut();
        let pos_dist = Normal::new(center_pos.x, pos_std_dev).unwrap();
        let pos_dist_y = Normal::new(center_pos.y, pos_std_dev).unwrap();

        (0..count)
            .map(|_| {
                let pos = Vec2D::new(pos_dist.sample(&mut *rng), pos_dist_y.sample(&mut *rng));
                self.create_massive_with_velocity(table, pos, Vec2D::zero())
            })
            .collect()
    }

    /// Creates a ball with specified properties and validation
    pub fn create_validated(
        &self,
        table: &SpeciesTable,
        pos: impl Into<Vec2D>,
        velocity: Vec2D,
    ) -> Result<Ball2D, String> {
        let pos_vec = pos.into();
        if !pos_vec.is_finite() || !velocity.is_finite() {
            return Err("Position and velocity components must be finite".to_string());
        }
        Ok(self.create_massive_with_velocity(table, pos_vec, velocity))
    }

    /// Creates a static ball (zero initial velocity and high mass)
    pub fn create_static(&self, table: &SpeciesTable, pos: impl Into<Vec2D>) -> Ball2D {
        self.create_massive_with_velocity(table, pos, Vec2D::zero())
    }

    /// Creates a lightweight ball with high velocity (projectile).
    ///
    /// # Arguments
    /// * `pos` - Initial position
    /// * `direction` - Direction vector (will be normalized)
    /// * `speed` - Magnitude of velocity
    ///
    /// # Returns
    /// New ball moving in specified direction at given speed
    pub fn create_projectile(
        &self,
        table: &SpeciesTable,
        pos: impl Into<Vec2D>,
        direction: Vec2D,
        speed: f32,
    ) -> Ball2D {
        let velocity = direction.normalize() * speed;
        self.create_massive_with_velocity(table, pos, velocity)
    }

    /// Resets the ID counter - useful for testing or when creating new simulation
    pub fn reset_id_counter(&self) {
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

    /// Get the next available ID without incrementing the counter
    pub fn peek_next_id(&self) -> usize {
        *self.next_id.borrow()
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
        let ball = factory.create_basic(SpeciesTable::builtin(), pos, species);

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
        let ball = factory.create_massive_with_velocity(SpeciesTable::builtin(), pos, velocity);

        assert_eq!(ball.get_id(), 1);
        assert_eq!(*ball.get_position(), pos);
        assert_eq!(*ball.get_velocity(), velocity);
    }

    #[test]
    fn test_validated_ball_creation() {
        let factory = BallFactory::new();

        assert!(
            factory
                .create_validated(
                    SpeciesTable::builtin(),
                    Vec2D::new(f32::NAN, 0.),
                    Vec2D::zero()
                )
                .is_err()
        );

        assert!(
            factory
                .create_validated(
                    SpeciesTable::builtin(),
                    Vec2D::zero(),
                    Vec2D::new(f32::NAN, 0.)
                )
                .is_err()
        );

        let rad_expect = factory.get_new_ball_radius(SpeciesTable::builtin());
        // Test valid parameters
        let ball = factory
            .create_validated(SpeciesTable::builtin(), Vec2D::zero(), Vec2D::zero())
            .unwrap();
        assert_eq!(ball.get_contact_radius(), rad_expect);
    }

    #[test]
    fn test_random_ball_creation() {
        let factory = BallFactory::new();
        let pos_range = (&Vec2D::new(0.0, 0.0), &Vec2D::new(10.0, 10.0));
        let velocity_range = (0.0, 5.0);
        let ball = factory.create_random(SpeciesTable::builtin(), pos_range, velocity_range);

        // Check if values are within ranges
        assert!(ball.get_position().x >= pos_range.0.x && ball.get_position().x <= pos_range.1.x);
        assert!(ball.get_position().y >= pos_range.0.y && ball.get_position().y <= pos_range.1.y);
    }
}
