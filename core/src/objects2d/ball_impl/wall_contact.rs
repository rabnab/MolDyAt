// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Contact of a ball with a flat wall, resolved along the wall normal.
//!
//! Within one step the ball's acceleration is taken as constant, so its
//! distance from the wall follows a parabola. Contact is resolved on that
//! parabola: the ball is reflected at the exact contact time and then
//! continues under the same acceleration, bouncing again (or coming to rest)
//! if the acceleration pushes it back within the step. Mirroring the whole
//! overshoot instead flips the sign of the ½·a·dt² term, which puts the ball
//! a·(dt - t_c)² too far from the wall against the force: an energy gain of
//! m·a²·(dt - t_c)² on every contact, and on every step for a ball resting on
//! a wall. Pushing the ball back onto the surface has the same kind of error.

use crate::objects2d::Vec2D;

/// One wall contact during a step, as needed by the velocity update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallContact {
    /// Unit wall normal, pointing away from the wall into the free region.
    pub normal: Vec2D,
    /// Fraction of the step before the first contact, in `[0, 1]`.
    pub contact_fraction: f32,
    /// Fraction of the step since the last bounce, or `None` if the ball ends
    /// the step resting on the wall.
    pub free_fraction: Option<f32>,
    /// Whether the momentum given to the wall counts towards the wall
    /// momentum (pressure) totals; hits on the ghost wall do not.
    pub counts_for_momentum: bool,
}

/// Normal-direction outcome of the position update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalPosition {
    /// Distance from the wall at the end of the step (`>= 0`).
    pub distance: f32,
    pub contact_fraction: f32,
    pub free_fraction: Option<f32>,
}

/// Resolve the normal motion of a ball `distance` away from a wall, moving at
/// normal velocity `velocity` with normal acceleration `accel` (both positive
/// away from the wall). Returns `None` if the ball does not reach the wall
/// during the step. A ball already inside the wall is treated as touching it.
pub fn resolve_normal_position(
    distance: f32,
    velocity: f32,
    accel: f32,
    dt: f32,
    restitution: f32,
) -> Option<NormalPosition> {
    let s0 = distance.max(0.0);
    let s_free = s0 + velocity * dt + 0.5 * accel * dt * dt;
    if s_free >= 0.0 {
        return None;
    }

    let t_contact = first_contact_time(s0, velocity, accel, dt);
    let v_contact = (velocity + accel * t_contact).min(0.0);
    let v_out = -restitution * v_contact;
    let remaining = dt - t_contact;

    // Time between bounces when the acceleration pushes the ball back.
    let bounce_period = if accel < 0.0 {
        2.0 * v_out / -accel
    } else {
        f32::INFINITY
    };
    if bounce_period <= RESTING_PERIOD_FRACTION * dt {
        return Some(NormalPosition {
            distance: 0.0,
            contact_fraction: t_contact / dt,
            free_fraction: None,
        });
    }
    let free_time = if remaining >= bounce_period {
        remaining - bounce_period * (remaining / bounce_period).floor()
    } else {
        remaining
    };

    Some(NormalPosition {
        distance: (v_out * free_time + 0.5 * accel * free_time * free_time).max(0.0),
        contact_fraction: t_contact / dt,
        free_fraction: Some(free_time / dt),
    })
}

/// Normal velocity at the end of a step with a wall contact.
///
/// `velocity` is the normal velocity at the start of the step and `kick` the
/// velocity-Verlet kick along the normal, `½·(a_old + a_new)·dt`. The kick is
/// spread evenly over the step, so without bounces in between this is the
/// usual split update: `-(v + kick·t_c) + kick·(1 - t_c)`.
pub fn normal_velocity_after_contact(
    velocity: f32,
    kick: f32,
    contact: &WallContact,
    restitution: f32,
) -> f32 {
    let Some(free_fraction) = contact.free_fraction else {
        return 0.0;
    };
    let v_contact = (velocity + kick * contact.contact_fraction).min(0.0);
    -restitution * v_contact + kick * free_fraction
}

/// A bounce period shorter than this fraction of the step counts as resting.
const RESTING_PERIOD_FRACTION: f32 = 1e-4;

/// Earliest `t` in `[0, dt]` with `s0 + v·t + ½·a·t² = 0`, given `s0 >= 0`
/// and a negative value at `dt`.
fn first_contact_time(s0: f32, v: f32, a: f32, dt: f32) -> f32 {
    let half_a = 0.5 * a;
    let t = if half_a.abs() <= f32::EPSILON * (v.abs() + s0 / dt) {
        s0 / -v
    } else {
        let disc = (v * v - 4.0 * half_a * s0).max(0.0);
        // Numerically stable roots of half_a·t² + v·t + s0.
        let q = -0.5 * (v + v.signum() * disc.sqrt());
        let (r1, r2) = (q / half_a, if q != 0.0 { s0 / q } else { f32::INFINITY });
        [r1, r2]
            .into_iter()
            .filter(|r| *r >= 0.0)
            .fold(f32::INFINITY, f32::min)
    };
    if t.is_finite() { t.clamp(0.0, dt) } else { 0.0 }
}
