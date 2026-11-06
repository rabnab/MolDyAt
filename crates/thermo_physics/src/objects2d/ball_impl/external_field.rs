// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! External fields (#347): forces that act on every ball by its position
//! alone, applied in the kick, with their potential energy in the stats.
//!
//! An [`ExternalField`] gives the acceleration of a ball and its potential
//! energy, so `-∇U / m` is the acceleration. Gravity is a [`UniformField`]
//! whose strength the window toggles at run time (`PhysicsConstants`); the
//! other fields come from the config (`[[fields]]`) and are fixed for a
//! world: [`HarmonicTrap`] pulls every ball towards a centre.

use crate::objects2d::Vec2D;

/// A field acting on every ball.
pub(crate) trait ExternalField {
    /// Acceleration of a ball of `mass` at `pos`.
    fn acceleration(&self, pos: Vec2D, mass: f32) -> Vec2D;
    /// Potential energy of a ball of `mass` at `pos`, zero at the field's
    /// reference.
    fn potential(&self, pos: Vec2D, mass: f32) -> f32;
}

/// Uniform downward field `-g ŷ` (gravity), with its potential `m·g·h`
/// measured from `floor`: a world far from the origin would otherwise add
/// `m·g·y` terms so large that f32 loses the other energies next to them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UniformField {
    /// Strength `g` (acceleration towards -y).
    pub g: f32,
    /// Height of the zero of the potential (the bottom wall).
    pub floor: f32,
}

impl UniformField {
    /// Gravity `g` with its potential measured from height 0.
    pub(crate) fn gravity(g: f32) -> Self {
        Self { g, floor: 0.0 }
    }
}

impl ExternalField for UniformField {
    #[inline]
    fn acceleration(&self, _pos: Vec2D, _mass: f32) -> Vec2D {
        Vec2D::new(0., -self.g)
    }

    #[inline]
    fn potential(&self, pos: Vec2D, mass: f32) -> f32 {
        mass * self.g * (pos.y - self.floor)
    }
}

/// Harmonic trap `U = ½·k·|r - c|²`: the force `-k·(r - c)` pulls every
/// ball towards the centre `c`, independent of its mass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HarmonicTrap {
    pub center: Vec2D,
    /// Spring constant `k` (ε/σ²).
    pub stiffness: f32,
}

impl ExternalField for HarmonicTrap {
    #[inline]
    fn acceleration(&self, pos: Vec2D, mass: f32) -> Vec2D {
        self.center.dist(&pos).scaled(-self.stiffness / mass)
    }

    #[inline]
    fn potential(&self, pos: Vec2D, _mass: f32) -> f32 {
        0.5 * self.stiffness * self.center.dist(&pos).norm_sq()
    }
}

/// A configured field (`[[fields]]`), the static dispatch over the kinds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Field {
    Harmonic(HarmonicTrap),
}

impl ExternalField for Field {
    #[inline]
    fn acceleration(&self, pos: Vec2D, mass: f32) -> Vec2D {
        match self {
            Field::Harmonic(f) => f.acceleration(pos, mass),
        }
    }

    #[inline]
    fn potential(&self, pos: Vec2D, mass: f32) -> f32 {
        match self {
            Field::Harmonic(f) => f.potential(pos, mass),
        }
    }
}

/// Sum of the accelerations of `fields` on a ball of `mass` at `pos`, added
/// in list order.
#[inline]
pub(crate) fn fields_acceleration(fields: &[Field], pos: Vec2D, mass: f32) -> Vec2D {
    fields
        .iter()
        .fold(Vec2D::zero(), |acc, f| acc + f.acceleration(pos, mass))
}
