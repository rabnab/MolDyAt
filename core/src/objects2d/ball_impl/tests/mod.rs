// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod ball_physics;
mod simd_forces;
mod verlet;
mod wall_contact;

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ball_factory::BallFactory;
use crate::objects2d::ball_impl::{validate_mass, validate_radius};

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
    use crate::objects2d::ball_impl::Named;
    let factory = BallFactory::new();
    let ball = factory.create_massive_with_velocity(Vec2D::zero(), Vec2D::zero());
    assert!(ball.get_name().starts_with("Ball"));
}
