pub use self::ball_factory::*;
pub use self::ball_physics::*;
// Re-export core components for external use
pub use crate::objects2d::ball_impl::ball_core::Ball2D;

// Module declarations
pub mod ball_core;
mod ball_factory;
pub mod ball_physics;
pub mod simd_forces;
pub mod verlet;

static mut UPDATE_HOOD_CNT: usize = 0;

/// get the count of how often update_hood was actually collecting the neighbors
pub fn get_update_hood_call_cnt() -> usize {
    unsafe { UPDATE_HOOD_CNT }
}

// Type definitions and constants that are common across ball modules
pub type BallId = usize;

/// Common trait for ball identification
pub trait Named {
    fn get_name(&self) -> String;
}

/// Result type for ball-related operations
pub type BallResult<T> = Result<T, BallError>;

/// Error types for ball-related operations
#[derive(Debug, PartialEq)]
pub enum BallError {
    DistanceUnderflow,
    PhysicsError,
    InvalidParameter,
    // Add other error types as needed
}

// Common validation functions if needed
#[inline]
#[cfg(test)]
pub(crate) fn validate_radius(radius: f32) -> BallResult<()> {
    if radius <= 0.0 {
        Err(BallError::InvalidParameter)
    } else {
        Ok(())
    }
}

#[inline]
#[cfg(test)]
pub(crate) fn validate_mass(mass: f32) -> BallResult<()> {
    if mass <= 0.0 {
        Err(BallError::InvalidParameter)
    } else {
        Ok(())
    }
}

// Tests for common functionality
#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::Vec2D;
    use crate::objects2d::ball_impl::ball_factory::BallFactory;

    #[test]
    fn test_validation_functions() {
        assert!(validate_radius(1.0).is_ok());
        assert!(validate_radius(0.0).is_err());
        assert!(validate_radius(-1.0).is_err());

        assert!(validate_mass(1.0).is_ok());
        assert!(validate_mass(0.0).is_err());
        assert!(validate_mass(-1.0).is_err());
    }

    #[test]
    fn test_ball_creation() {
        let factory = BallFactory::new();
        let ball = factory.create_massive_with_velocity(Vec2D::zero(), Vec2D::zero());
        assert_eq!(*ball.get_position(), Vec2D::new(0.0, 0.0));
    }

    #[test]
    fn test_named_trait() {
        let factory = BallFactory::new();
        let ball = factory.create_massive_with_velocity(Vec2D::zero(), Vec2D::zero());
        assert!(ball.get_name().starts_with("Ball"));
    }
}
