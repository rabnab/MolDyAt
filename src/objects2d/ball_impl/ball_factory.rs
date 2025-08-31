use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::vec_2d::Vec2D;

use rand::Rng;
use rand_distr::{Distribution, Normal};

/// Factory for creating Ball2D instances with various configurations
pub struct BallFactory {
    next_id: usize,
}

const FIRST_BALL_ID: usize = 1;

impl Default for BallFactory {
    fn default() -> Self {
        BallFactory {
            next_id: FIRST_BALL_ID,
        }
    }
}

impl BallFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new ball with specified position and radius
    pub fn create_basic<'a>(&mut self, pos: impl Into<Vec2D>, radius: f32) -> Ball2D<'a> {
        let pos_vec = pos.into();

        let ball = Ball2D::new(self.next_id, pos_vec.clone(), radius);

        self.next_id += 1;
        ball
    }

    /// Creates a ball with specified mass and initial velocity
    pub fn create_massive_with_velocity<'a>(
        &mut self,
        pos: impl Into<Vec2D>,
        radius: f32,
        mass: f32,
        initial_velocity: Vec2D,
    ) -> Ball2D<'a> {
        let mut ball = self.create_basic(pos, radius);
        ball.mass = mass;
        ball.v = initial_velocity;
        ball
    }

    /// Creates a ball with random properties within specified ranges
    pub fn create_random<'a>(
        &mut self,
        pos_range: (Vec2D, Vec2D),
        radius_range: (f32, f32),
        mass_range: (f32, f32),
        velocity_range: (f32, f32),
    ) -> Ball2D<'a> {
        let mut rng = rand::rng();

        // Random position within bounds
        let pos = Vec2D::new(
            rng.random_range(pos_range.0.x..pos_range.1.x),
            rng.random_range(pos_range.0.y..pos_range.1.y),
        );

        // Random radius
        let radius = rng.random_range(radius_range.0..radius_range.1);

        // Random mass
        let mass = rng.random_range(mass_range.0..mass_range.1);

        // Random velocity with random direction
        let speed = rng.random_range(velocity_range.0..velocity_range.1);
        let angle = rng.random_range(0.0..std::f32::consts::TAU);
        let velocity = Vec2D::new(speed * angle.cos(), speed * angle.sin());

        self.create_massive_with_velocity(pos, radius, mass, velocity)
    }

    /// Creates a collection of balls with normally distributed properties
    pub fn create_normal_distribution<'a>(
        &mut self,
        count: usize,
        center_pos: Vec2D,
        pos_std_dev: f32,
        mean_radius: f32,
        radius_std_dev: f32,
        mean_mass: f32,
        mass_std_dev: f32,
    ) -> Vec<Ball2D<'a>> {
        let mut rng = rand::rng();
        let pos_dist = Normal::new(0.0, pos_std_dev).unwrap();
        let radius_dist = Normal::new(mean_radius, radius_std_dev).unwrap();
        let mass_dist = Normal::new(mean_mass, mass_std_dev).unwrap();

        (0..count)
            .map(|_| {
                let pos = Vec2D::new(
                    center_pos.x + pos_dist.sample(&mut rng),
                    center_pos.y + pos_dist.sample(&mut rng),
                );
                let radius = radius_dist.sample(&mut rng).max(0.1); // Ensure positive radius
                let mass = mass_dist.sample(&mut rng).max(0.1); // Ensure positive mass

                self.create_massive_with_velocity(pos, radius, mass, Vec2D::zero())
            })
            .collect()
    }

    /// Creates a ball with specified properties and validation
    pub fn create_validated<'a>(
        &mut self,
        pos: impl Into<Vec2D>,
        radius: f32,
        mass: f32,
        velocity: Vec2D,
    ) -> Result<Ball2D<'a>, String> {
        // Validate input parameters
        if radius <= 0.0 {
            return Err("Radius must be positive".to_string());
        }
        if mass <= 0.0 {
            return Err("Mass must be positive".to_string());
        }
        let pos_vec = pos.into();
        if !pos_vec.is_finite() || !velocity.is_finite() {
            return Err("Position and velocity components must be finite".to_string());
        }

        Ok(self.create_massive_with_velocity(pos_vec, radius, mass, velocity))
    }

    /// Creates a static ball (zero initial velocity and high mass)
    pub fn create_static<'a>(&mut self, pos: impl Into<Vec2D>, radius: f32) -> Ball2D<'a> {
        self.create_massive_with_velocity(pos, radius, f32::MAX / 2.0, Vec2D::zero())
    }

    /// Creates a lightweight ball with high velocity
    pub fn create_projectile<'a>(
        &mut self,
        pos: impl Into<Vec2D>,
        radius: f32,
        direction: Vec2D,
        speed: f32,
    ) -> Ball2D<'a> {
        let velocity = direction.normalize() * speed;
        self.create_massive_with_velocity(pos, radius, radius * 0.1, velocity)
    }

    /// Resets the ID counter - useful for testing or when creating new simulation
    pub fn reset_id_counter(&mut self) {
        self.next_id = FIRST_BALL_ID;
    }

    /// Get the next available ID without incrementing the counter
    pub fn peek_next_id(&self) -> usize {
        self.next_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_ball_creation() {
        let mut factory = BallFactory::new();
        let pos = Vec2D::new(1.0, 2.0);
        let ball = factory.create_basic(pos.clone(), 1.0);

        assert_eq!(ball.get_id(), 1);
        assert_eq!(*ball.get_position(), pos);
        assert_eq!(ball.get_radius(), 1.0);
        assert_eq!(ball.get_mass(), 1.0);
        assert_eq!(*ball.get_velocity(), Vec2D::zero());
    }

    #[test]
    fn test_massive_ball_creation() {
        let mut factory = BallFactory::new();
        let pos = Vec2D::new(1.0, 2.0);
        let velocity = Vec2D::new(3.0, 4.0);
        let ball = factory.create_massive_with_velocity(pos.clone(), 2.0, 5.0, velocity.clone());

        assert_eq!(ball.get_id(), 1);
        assert_eq!(*ball.get_position(), pos);
        assert_eq!(ball.get_radius(), 2.0);
        assert_eq!(ball.get_mass(), 5.0);
        assert_eq!(*ball.get_velocity(), velocity);
    }

    #[test]
    fn test_validated_ball_creation() {
        let mut factory = BallFactory::new();

        // Test invalid radius
        assert!(
            factory
                .create_validated(Vec2D::zero(), -1.0, 1.0, Vec2D::zero())
                .is_err()
        );

        // Test invalid mass
        assert!(
            factory
                .create_validated(Vec2D::zero(), 1.0, -1.0, Vec2D::zero())
                .is_err()
        );

        // Test valid parameters
        let ball = factory
            .create_validated(Vec2D::zero(), 1.0, 1.0, Vec2D::zero())
            .unwrap();
        assert_eq!(ball.get_radius(), 1.0);
    }

    #[test]
    fn test_random_ball_creation() {
        let mut factory = BallFactory::new();
        let pos_range = (Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 10.0));
        let radius_range = (1.0, 2.0);
        let mass_range = (1.0, 5.0);
        let velocity_range = (0.0, 5.0);

        let ball =
            factory.create_random(pos_range.clone(), radius_range, mass_range, velocity_range);

        // Check if values are within ranges
        assert!(ball.get_position().x >= pos_range.0.x && ball.get_position().x <= pos_range.1.x);
        assert!(ball.get_position().y >= pos_range.0.y && ball.get_position().y <= pos_range.1.y);
        assert!(ball.get_radius() >= radius_range.0 && ball.get_radius() <= radius_range.1);
        assert!(ball.get_mass() >= mass_range.0 && ball.get_mass() <= mass_range.1);
    }
}
