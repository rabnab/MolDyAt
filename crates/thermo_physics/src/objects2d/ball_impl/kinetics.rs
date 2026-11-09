// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Kinetic energy, degrees of freedom and thermal sampling of one ball.
//!
//! The only place that combines translation with rotation (#348): code that
//! measures, scales, samples or stops a ball's motion calls these instead of
//! reading `omega` or `inertia`, so a further degree of freedom is added here.

use super::ball_core::Ball2D;
use crate::objects2d::Vec2D;
use rand::Rng;
use rand_distr::{Distribution, Normal};

impl Ball2D {
    /// Degrees of freedom: 2 translational, plus 1 for a ball with an
    /// orientation.
    pub(crate) fn dof(&self) -> usize {
        2 + usize::from(self.inertia > 0.0)
    }

    /// Rotational kinetic energy `½·I·ω²` (0 without orientation).
    #[inline]
    pub(crate) fn rotational_energy(&self) -> f32 {
        0.5 * self.inertia * self.omega * self.omega
    }

    /// Translational plus rotational kinetic energy.
    #[inline]
    pub(crate) fn kinetic_energy(&self) -> f64 {
        f64::from(self.v.norm_sq() * self.mass * 0.5) + f64::from(self.rotational_energy())
    }

    /// Scale the velocity about `centre` (the frame the temperature is
    /// measured in) and the angular velocity by `factor`.
    pub(crate) fn scale_about(&mut self, centre: Vec2D, factor: f32) {
        self.v = centre.add_scaled(&(self.v - &centre), factor);
        self.omega *= factor;
    }

    /// No translation and no rotation (else the rotation feeds its energy
    /// back).
    pub(crate) fn stop(&mut self) {
        self.v = Vec2D::zero();
        self.omega = 0.0;
    }

    /// Draw `v` and `ω` at `temperature` (2D, `k_B = 1`): each component
    /// `~ N(0, √(T / m))`, `ω ~ N(0, √(T / I))`.
    pub(crate) fn sample(&mut self, temperature: f32, rng: &mut impl Rng) {
        let normal = Normal::new(0.0, (temperature / self.mass).sqrt()).expect("finite spread");
        self.v = Vec2D::new(normal.sample(rng), normal.sample(rng));
        self.sample_spin(temperature, rng);
    }

    /// Draw `ω` at `temperature`; nothing from the RNG for a ball without
    /// orientation or at `T ≤ 0`, so runs of those stay bit-identical.
    pub(crate) fn sample_spin(&mut self, temperature: f32, rng: &mut impl Rng) {
        if self.inertia > 0.0
            && temperature > 0.0
            && let Ok(normal) = Normal::new(0.0, (temperature / self.inertia).sqrt())
        {
            self.omega = normal.sample(rng);
        }
    }
}
