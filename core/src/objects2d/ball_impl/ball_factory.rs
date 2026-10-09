// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::ball_impl::BallId;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::vec_2d::Vec2D;
use std::cell::RefCell;

use rand::rngs::{StdRng, SysRng};
use rand::{RngExt, SeedableRng};
use rand_distr::{Distribution, Normal};

/// Factory for creating Ball2D instances with various configurations
pub struct BallFactory {
    next_id: RefCell<usize>,
    new_ball_size_idx: RefCell<usize>,
    rng: RefCell<StdRng>,
}

const FIRST_BALL_ID: usize = 1;

impl Default for BallFactory {
    fn default() -> Self {
        BallFactory {
            next_id: RefCell::new(FIRST_BALL_ID),
            new_ball_size_idx: RefCell::new(2),
            rng: RefCell::new(StdRng::try_from_rng(&mut SysRng).unwrap()),
        }
    }
}

/// Minimum ball radius (smallest size class)
pub const MIN_BALL_SIZE: f32 = 0.15;
/// Number of ball size classes
pub const NUM_BALL_SIZES: usize = 5;

impl BallFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a factory with deterministic RNG state from a fixed seed.
    pub fn with_seed(seed: u64) -> Self {
        BallFactory {
            next_id: RefCell::new(FIRST_BALL_ID),
            new_ball_size_idx: RefCell::new(2),
            rng: RefCell::new(StdRng::seed_from_u64(seed)),
        }
    }

    /// Calculate ball radius for a given size index.
    /// Radii increase exponentially: MIN_BALL_SIZE * 2^size_idx
    ///
    /// # Arguments
    /// * `size_idx` - Size class index (0-4)
    ///
    /// # Returns
    /// Ball radius in world units
    pub fn radius_for_size_idx(size_idx: usize) -> f32 {
        const BASE_EXPONENT: f32 = 2.0;
        MIN_BALL_SIZE * BASE_EXPONENT.powi(size_idx as i32)
    }

    fn mass_for_size_idx(size_idx: usize) -> f32 {
        let density = 2. / 10.;
        let radius = Self::radius_for_size_idx(size_idx);
        1. + density * (radius / MIN_BALL_SIZE - 1.).powf(1.2)
    }

    pub(crate) fn next_ball_size_idx(&self) {
        let current_idx = *self.new_ball_size_idx.borrow();
        *self.new_ball_size_idx.borrow_mut() = (current_idx + 1) % NUM_BALL_SIZES;
    }

    /// Create vector containing all possible ball radii.
    ///
    /// # Returns
    /// Vector of 5 radii values for size indices 0-4
    pub fn create_all_radii() -> Vec<f32> {
        (0..NUM_BALL_SIZES).map(Self::radius_for_size_idx).collect()
    }

    /// Returns the maximum possible ball radius (for the largest size index)
    pub fn get_max_radius() -> f32 {
        Self::radius_for_size_idx(NUM_BALL_SIZES - 1)
    }

    pub(crate) fn get_new_ball_size_idx(&self) -> usize {
        *self.new_ball_size_idx.borrow()
    }

    pub(crate) fn get_new_ball_radius(&self) -> f32 {
        Self::radius_for_size_idx(*self.new_ball_size_idx.borrow())
    }

    /// Creates a new ball with specified position and radius
    pub(crate) fn create_basic(&self, pos: impl Into<Vec2D>, size_idx: usize) -> Ball2D {
        let pos_vec = pos.into();
        let id = *self.next_id.borrow();
        *self.next_id.borrow_mut() += 1;

        Ball2D::new(id, pos_vec, (size_idx, Self::radius_for_size_idx(size_idx)))
    }

    /// Creates a ball with specified mass and initial velocity
    pub fn create_massive_with_velocity(
        &self,
        pos: impl Into<Vec2D>,
        initial_velocity: Vec2D,
    ) -> Ball2D {
        let size_idx = self.get_new_ball_size_idx();
        self.create_with_size_and_velocity(pos, size_idx, initial_velocity)
    }

    /// Creates a ball with a specific size class and initial velocity.
    pub fn create_with_size_and_velocity(
        &self,
        pos: impl Into<Vec2D>,
        size_idx: usize,
        velocity: Vec2D,
    ) -> Ball2D {
        let mut ball = self.create_basic(pos, size_idx);
        ball.mass = Self::mass_for_size_idx(size_idx);
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
    pub fn create_random(&self, pos_range: (&Vec2D, &Vec2D), velocity_range: (f32, f32)) -> Ball2D {
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

        self.create_massive_with_velocity(pos, velocity)
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
                self.create_massive_with_velocity(pos, Vec2D::zero())
            })
            .collect()
    }

    /// Creates a ball with specified properties and validation
    pub fn create_validated(
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

    /// Creates a static ball (zero initial velocity and high mass)
    pub fn create_static(&self, pos: impl Into<Vec2D>) -> Ball2D {
        self.create_massive_with_velocity(pos, Vec2D::zero())
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
    pub fn create_projectile(&self, pos: impl Into<Vec2D>, direction: Vec2D, speed: f32) -> Ball2D {
        let velocity = direction.normalize() * speed;
        self.create_massive_with_velocity(pos, velocity)
    }

    /// Resets the ID counter - useful for testing or when creating new simulation
    pub fn reset_id_counter(&self) {
        *self.next_id.borrow_mut() = FIRST_BALL_ID;
    }

    /// A ball with a given id (e.g. from a save): radius and mass follow from
    /// `size_idx`. Does not touch the id counter; see [`Self::continue_after`].
    pub(crate) fn restore(id: BallId, size_idx: usize, pos: Vec2D, velocity: Vec2D) -> Ball2D {
        let mut ball = Ball2D::new(id, pos, (size_idx, Self::radius_for_size_idx(size_idx)));
        ball.mass = Self::mass_for_size_idx(size_idx);
        ball.v = velocity;
        ball
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
        let ball = factory.create_basic(pos, 0);

        assert_eq!(ball.get_id(), 1);
        assert_eq!(*ball.get_position(), pos);
        assert_eq!(ball.get_radius(), MIN_BALL_SIZE);
        assert_eq!(ball.get_mass(), BallFactory::mass_for_size_idx(0));
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
        assert_eq!(ball.get_radius(), rad_expect);
    }

    #[test]
    fn test_random_ball_creation() {
        let factory = BallFactory::new();
        let pos_range = (&Vec2D::new(0.0, 0.0), &Vec2D::new(10.0, 10.0));
        let velocity_range = (0.0, 5.0);
        let ball = factory.create_random(pos_range, velocity_range);

        // Check if values are within ranges
        assert!(ball.get_position().x >= pos_range.0.x && ball.get_position().x <= pos_range.1.x);
        assert!(ball.get_position().y >= pos_range.0.y && ball.get_position().y <= pos_range.1.y);
    }
}
