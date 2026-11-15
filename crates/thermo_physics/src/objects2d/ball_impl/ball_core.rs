// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Rect;
use crate::objects2d::ball_impl::external_field::{ExternalField, UniformField};
#[cfg(test)]
use crate::objects2d::ball_impl::species::DEFAULT_SPECIES;
#[cfg(any(test, feature = "test-support"))]
use crate::objects2d::ball_impl::species::SpeciesTable;
use crate::objects2d::ball_impl::species::{Species, SpeciesId};
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::wall_contact::{
    NormalPosition, WallContact, resolve_normal_position,
};
use crate::objects2d::ball_impl::{BallId, Named};
use crate::objects2d::vec_2d::Vec2D;
use crate::objects2d::verbose_info::VerboseInfo;
use std::collections::VecDeque;

/// Maximum number of tail points kept per verbose ball.
const TAIL_LENGTH: usize = 1000;

const TAIL_RESOLUTION_DELTA_SQ: f32 = 1. * 1.; // minimum squared distance to add new tail point

/// Wall contacts found during the position update, for the velocity update.
/// The velocity update applies them one after the other along their normals,
/// so contacts in the same step must have orthogonal normals: a box corner
/// gives one `box_x` and one `box_y` contact, and a line wall is only swept
/// in a step without a box contact (`line_wall` is `None` whenever a box
/// field is set).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BounceInfo {
    /// Contact with the left or right box wall.
    pub box_x: Option<WallContact>,
    /// Contact with the top or bottom box wall.
    pub box_y: Option<WallContact>,
    /// Contact with a line-segment wall (see `SimulationWorld::sweep_wall_contact`).
    pub line_wall: Option<WallContact>,
    /// Momentum the walls took from a molecule's site impulses
    /// ([`Ball2D::wall_hit`]) in the position update.
    pub wall_momentum: Vec2D,
    /// Site impulses applied.
    pub impulses: u32,
}

impl Default for BounceInfo {
    fn default() -> Self {
        Self {
            box_x: None,
            box_y: None,
            line_wall: None,
            wall_momentum: Vec2D::zero(),
            impulses: 0,
        }
    }
}

impl BounceInfo {
    /// Whether the ball touched a box wall this step.
    pub(crate) fn touched_box(&self) -> bool {
        self.box_x.is_some() || self.box_y.is_some()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &WallContact> {
        [&self.box_x, &self.box_y, &self.line_wall]
            .into_iter()
            .flatten()
    }
}

/// A ball: the data the step reads or writes for every ball (hot), plus a
/// pointer to the rest ([`BallCold`]), so the passes over all balls touch
/// less memory per ball (P6, #346; about 96 bytes instead of ~200, 80
/// before the f64 position of #213, which still spans two 64-byte lines
/// for most balls).
pub struct Ball2D {
    /// The position (#213): f64, so the small step increments are not
    /// rounded away far from the origin. Every move goes through it.
    exact: [f64; 2],
    /// `exact` rounded to f32, for everything that reads positions.
    pos: Vec2D,
    pub v: Vec2D,
    pub accel: Vec2D,
    pub mass: f32,
    /// Moment of inertia of the species (#348); 0 for a species without
    /// orientation, whose `theta`, `omega` and `alpha` stay 0.
    pub inertia: f32,
    /// Orientation in radians, in `[-π, π)`.
    pub theta: f32,
    /// Angular velocity.
    pub omega: f32,
    /// Angular acceleration (torque / inertia) of the last force pass.
    pub alpha: f32,
    pub species: SpeciesId,
    /// `σ / 2` of the species: the ball's size for overlap and placement
    /// checks. Wall contacts take their distance from the `WallSettings`.
    pub contact_radius: f32,
    pub pre_update_pos: Vec2D,
    pub last_update_hood_pos: Vec2D,
    pub(crate) id: BallId,
    /// Four flags, checked in every pass, so they stay hot.
    pub verbose: VerboseInfo,
    pub braking: bool,
    /// A wall appeared or changed on top of this ball (a valve closed, a wall
    /// was drawn, moved or thickened over it). With soft walls the line walls
    /// ramp in for it (`BallCold::wall_ramp`); with hard walls it passes
    /// through the walls it overlaps until it touches none.
    pub leaving_wall: bool,
    pub(crate) cold: Box<BallCold>,
}

/// The part of a ball kept out of line: the neighbour list (read once per
/// ball in the force pass and the rebuild, through its own heap buffer
/// anyway) and the tail of verbose balls.
#[derive(Default)]
pub(crate) struct BallCold {
    pub(crate) hood: Verlet,
    /// Position history, kept only while the ball is verbose (allocated
    /// lazily; `None` otherwise).
    tail: Option<VecDeque<Vec2D>>,
    /// While `Ball2D::leaving_wall` with soft walls: how strongly the line
    /// walls act on the ball, growing from 0 to 1 over `WALL_RAMP_TIME`.
    pub(crate) wall_ramp: f32,
}

/// A ball of the built-in default species (`N2`) at the origin. Test builds
/// only: a world's balls come from its species table (see
/// [`Ball2D::of_species`]).
#[cfg(test)]
impl Default for Ball2D {
    fn default() -> Ball2D {
        let mut ball = Ball2D::new(0, Vec2D::zero(), DEFAULT_SPECIES);
        ball.pre_update_pos = Vec2D::zero();
        ball
    }
}

/// `angle` wrapped into `[-π, π)`.
pub(crate) fn wrap_angle(angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    if (-PI..PI).contains(&angle) {
        angle
    } else {
        angle - TAU * ((angle + PI) / TAU).floor()
    }
}

/// Gravity of strength `gravity` as the [`UniformField`] it is (#347): the
/// acceleration `-g ŷ`. Shorthand for the call sites that only shift
/// accelerations when gravity changes.
pub(crate) fn gravity_acceleration(gravity: f32) -> Vec2D {
    UniformField::gravity(gravity).acceleration(Vec2D::zero(), 1.0)
}

/// The potential energy of mass `mass` at height `height` above the floor
/// in gravity `gravity` (see [`UniformField`]).
#[cfg(test)]
pub(crate) fn gravity_potential(mass: f32, gravity: f32, height: f32) -> f32 {
    UniformField::gravity(gravity).potential(Vec2D::new(0.0, height), mass)
}

impl Ball2D {
    /// A ball of a built-in `species` at rest, with its mass and contact
    /// radius. Test builds only: production code builds balls from the
    /// world's table with [`Self::of_species`].
    #[cfg(any(test, feature = "test-support"))]
    pub fn new(id: BallId, pos: Vec2D, species: SpeciesId) -> Ball2D {
        Self::of_species(id, pos, species, SpeciesTable::builtin().species(species))
    }

    /// A ball of species `species` (with the data `data` of that species, from
    /// the world's table) at rest, without a neighbour list:
    /// `last_update_hood_pos` at infinity makes the next rebuild pass build
    /// one.
    pub(crate) fn of_species(id: BallId, pos: Vec2D, species: SpeciesId, data: &Species) -> Ball2D {
        Ball2D {
            contact_radius: data.contact_radius(),
            species,
            exact: [f64::from(pos.x), f64::from(pos.y)],
            pos,
            id,
            mass: data.mass,
            inertia: data.model.inertia().unwrap_or(0.0),
            theta: 0.0,
            omega: 0.0,
            alpha: 0.0,
            v: Vec2D::zero(),
            verbose: VerboseInfo::default(),
            braking: false,
            pre_update_pos: pos,
            accel: Vec2D::zero(),
            last_update_hood_pos: Vec2D::new(f32::INFINITY, f32::INFINITY),
            leaving_wall: false,
            cold: Box::default(),
        }
    }

    pub fn get_id(&self) -> BallId {
        self.id
    }

    pub fn species(&self) -> SpeciesId {
        self.species
    }

    // Basic position and property access methods
    pub fn get_position(&self) -> &Vec2D {
        &self.pos
    }

    /// The position in f64 (see `exact`): for differences of positions far
    /// from the origin (the force kernel).
    pub(crate) fn exact_position(&self) -> [f64; 2] {
        self.exact
    }

    pub fn get_tail_positions(&self) -> Option<&VecDeque<Vec2D>> {
        self.cold.tail.as_ref().filter(|tail| !tail.is_empty())
    }

    pub fn get_velocity(&self) -> &Vec2D {
        &self.v
    }

    /// `σ / 2` of the ball's species.
    pub fn get_contact_radius(&self) -> f32 {
        self.contact_radius
    }

    pub fn get_mass(&self) -> f32 {
        self.mass
    }

    /// Gap between the contact discs of this ball and another: the centre
    /// distance minus both contact radii (`σ_ij` under arithmetic mixing),
    /// zero where their pair potential crosses zero.
    ///
    /// # Arguments
    /// * `other` - Other ball to measure distance to
    ///
    /// # Returns
    /// Gap between the contact discs (negative if overlapping)
    #[cfg(test)]
    pub(crate) fn get_edge_distance(&self, other: &Ball2D) -> f32 {
        let center_dist = self.get_position().dist(other.get_position()).norm();
        center_dist - (self.contact_radius + other.contact_radius)
    }

    // Core state methods
    pub fn is_braking(&self) -> bool {
        self.braking
    }

    /// Clear the tail at a neighbour-list rebuild, so the rendered trajectory
    /// shows the movement since that rebuild only.
    pub(crate) fn clear_tail(&mut self) {
        if let Some(tail) = self.cold.tail.as_mut() {
            tail.clear();
        }
    }

    /// Rotational half of the velocity-Verlet drift (#348):
    /// `θ += ω·dt + ½·α·dt²`, wrapped into `[-π, π)`.
    pub(crate) fn advance_orientation(&mut self, dt: f32) {
        let theta = self.theta + self.omega * dt + 0.5 * self.alpha * dt * dt;
        self.theta = wrap_angle(theta);
    }

    /// Rotational kick (#348): `ω += ½·(α_old + α_new)·dt` with
    /// `α_new = torque / inertia`. Only for balls with an orientation.
    pub(crate) fn kick_rotation(&mut self, torque: f32, dt: f32) {
        let alpha = torque / self.inertia;
        self.omega += 0.5 * (self.alpha + alpha) * dt;
        self.alpha = alpha;
    }

    /// Calculate linear momentum (mass × velocity).
    ///
    /// # Returns
    /// Momentum vector
    pub(crate) fn moment(&self) -> Vec2D {
        self.v.scaled(self.mass)
    }

    pub(crate) fn accel_with_speed_limit(
        &mut self,
        total_force: &Vec2D,
        gravity: f32,
        speed_limit: f32,
        dt: f32,
    ) -> Vec2D {
        let gravi_vec = gravity_acceleration(gravity);
        let mut accel: Vec2D = (total_force).scaled(1. / self.mass) + gravi_vec;

        debug_assert!(speed_limit >= 0., "negative speed limit {speed_limit}");
        let speed_sq = self.v.norm_sq();

        if speed_sq > speed_limit.powi(2) {
            let speed = f32::sqrt(speed_sq);
            let over_speed = speed - speed_limit;
            let speed_reduction = 0.25 * f32::sqrt(2.) * over_speed;

            // Apply braking acceleration opposite to velocity
            let accel_modification = self.v.scaled(-speed_reduction / dt / speed);
            accel += accel_modification;
            self.braking = true;

            if self.verbose.console {
                log::debug!(
                    target: crate::logging::VERBOSE_TARGET,
                    "braking: over_speed {} at speed {:?}",
                    over_speed,
                    self.v
                );
            }
        } else {
            self.braking = false;
        }
        accel
    }

    /// Save current position before physics update.
    /// Used to restore position or calculate displacement.
    /// Note: Despite the name, this stores the "current" position, not the "old" one.
    pub(crate) fn update_old_pos(&mut self) {
        self.pre_update_pos = self.pos;
    }

    /// Free velocity-Verlet drift of a molecule's position and angle, with
    /// the arithmetic of the free path of [`Self::resolve_box_axis`].
    pub(crate) fn drift(&mut self, dt: f32) {
        let displacement = |v: f32, a: f32| f64::from(v * dt + 0.5 * a * dt * dt);
        let [x, y] = self.exact;
        self.set_exact_position([
            x + displacement(self.v.x, self.accel.x),
            y + displacement(self.v.y, self.accel.y),
        ]);
        self.advance_orientation(dt);
    }

    /// Move the ball by `delta`, added to its f64 position.
    pub(crate) fn shift(&mut self, delta: Vec2D) {
        let [x, y] = self.exact;
        self.set_exact_position([x + f64::from(delta.x), y + f64::from(delta.y)]);
    }

    /// Resolve one box axis: the ball's centre must stay in `[min, max]`.
    /// Returns the new coordinate and the contact, if the ball reached a wall.
    fn resolve_box_axis(
        pos: f64,
        velocity: f32,
        accel: f32,
        (min, max): (f32, f32),
        dt: f32,
        restitution: f32,
    ) -> (f64, Option<(f32, NormalPosition)>) {
        // The displacement is small and computed in f32; added to the f64
        // position it is not rounded away far from the origin (#213).
        let displacement = velocity * dt + 0.5 * accel * dt * dt;
        let free = pos + f64::from(displacement);
        let (min, max) = (f64::from(min), f64::from(max));
        if free <= min {
            // Normal +axis: distance from the wall is `pos - min`.
            let distance = (pos - min) as f32;
            if let Some(n) = resolve_normal_position(distance, velocity, accel, dt, restitution) {
                return (min + f64::from(n.distance), Some((1.0, n)));
            }
        } else if free >= max {
            // Normal -axis: distance from the wall is `max - pos`.
            let distance = (max - pos) as f32;
            if let Some(n) = resolve_normal_position(distance, -velocity, -accel, dt, restitution) {
                return (max - f64::from(n.distance), Some((-1.0, n)));
            }
        }
        (free, None)
    }

    /// Update position using velocity-Verlet integration and resolve contacts
    /// with the box walls (see `wall_contact`). Velocities are updated later,
    /// in the velocity half of the step, from the returned contacts.
    ///
    /// # Arguments
    /// * `dt` - Time step
    /// * `bound` - Boundary rectangle (reflective walls)
    /// * `contact` - Centre distance from the left/right and the
    ///   bottom/top walls at which the ball is reflected
    ///   (`BallPhysics::wall_contact_distance`)
    /// * `restitution` - Normal restitution of the walls (1.0 = elastic)
    pub(crate) fn execute_update_pos(
        &mut self,
        dt: f32,
        bound: &Rect,
        contact: [f32; 2],
        restitution: f32,
    ) -> BounceInfo {
        let old_pos = self.pos;
        let (x0, y0) = bound.get_tl();
        let (x1, y1) = bound.get_br();
        let [contact_x, contact_y] = contact;
        let x_range = (f32::min(x0, x1) + contact_x, f32::max(x0, x1) - contact_x);
        let y_range = (f32::min(y0, y1) + contact_y, f32::max(y0, y1) - contact_y);

        let [x, y] = self.exact;
        let (x, x_contact) =
            Self::resolve_box_axis(x, self.v.x, self.accel.x, x_range, dt, restitution);
        let (y, y_contact) =
            Self::resolve_box_axis(y, self.v.y, self.accel.y, y_range, dt, restitution);

        self.set_exact_position([x, y]);

        if self.verbose.console {
            log::debug!(
                target: crate::logging::VERBOSE_TARGET,
                "moved pos {:?} -->  {:?} at dt {:.3} v {:?} a {:?}",
                old_pos,
                self.pos,
                dt,
                self.v,
                self.accel
            );
        }

        let contact = |axis: Vec2D, (sign, n): (f32, NormalPosition)| WallContact {
            normal: axis.scaled(sign),
            contact_fraction: n.contact_fraction,
            free_fraction: n.free_fraction,
            counts_for_momentum: true,
        };
        BounceInfo {
            box_x: x_contact.map(|c| contact(Vec2D::new(1.0, 0.0), c)),
            box_y: y_contact.map(|c| contact(Vec2D::new(0.0, 1.0), c)),
            line_wall: None,
            wall_momentum: Vec2D::zero(),
            impulses: 0,
        }
    }

    /// The rigid-body impulse `J = -(1 + e)·u / (1/m + (r×n)²/I)` of a wall
    /// with normal `normal` on the site at `lever` from the centre, `u` its
    /// normal velocity, if the site moves into the wall: it reflects the
    /// site, turns the molecule and with `e = 1` keeps the kinetic energy.
    /// The site, `overlap` inside its contact distance, is moved out by
    /// `(1 + e)·overlap`, mirroring the overshoot. A site moving out is left
    /// alone: moving it would change the energy with no force paying for it.
    pub(crate) fn wall_hit(
        &mut self,
        normal: Vec2D,
        overlap: f32,
        lever: Vec2D,
        restitution: f32,
    ) -> Option<f32> {
        let point_velocity = self.v + Vec2D::new(-self.omega * lever.y, self.omega * lever.x);
        let approach = point_velocity.dot(&normal);
        if approach >= 0.0 {
            return None;
        }
        let arm = lever.x * normal.y - lever.y * normal.x;
        let inverse_mass = 1.0 / self.mass + arm * arm / self.inertia;
        let impulse = -(1.0 + restitution) * approach / inverse_mass;
        self.v = self.v.add_scaled(&normal, impulse / self.mass);
        self.omega += arm * impulse / self.inertia;
        self.shift(normal.scaled((1.0 + restitution) * overlap));
        Some(impulse)
    }

    /// Set ball position and, for verbose balls only, add to the tail trail
    /// if movement exceeds the threshold.
    /// Only adds to tail if distance from last tail point exceeds TAIL_RESOLUTION_DELTA_SQ.
    /// Maintains tail length by removing oldest points when limit is exceeded.
    /// A ball that is no longer verbose drops its tail.
    ///
    /// # Arguments
    /// * `new_pos` - New position to set
    pub(crate) fn set_position_and_update_tail(&mut self, new_pos: Vec2D) {
        self.set_exact_position([f64::from(new_pos.x), f64::from(new_pos.y)]);
    }

    /// Set the f64 position and its f32 copy, and the tail (see
    /// [`Self::set_position_and_update_tail`]).
    pub(crate) fn set_exact_position(&mut self, exact: [f64; 2]) {
        self.exact = exact;
        let new_pos = Vec2D::new(exact[0] as f32, exact[1] as f32);
        if self.verbose.any() {
            let tail = self.cold.tail.get_or_insert_with(Default::default);
            let far_enough = tail
                .back()
                .is_none_or(|last| last.dist(&new_pos).norm_sq() > TAIL_RESOLUTION_DELTA_SQ);
            if far_enough {
                tail.push_back(new_pos);
                if tail.len() > TAIL_LENGTH {
                    tail.pop_front();
                }
            }
        } else if self.cold.tail.is_some() {
            self.cold.tail = None;
        }
        self.pos = new_pos;
    }

    #[cfg(test)]
    pub(crate) fn get_neighbour_names(&self, balls: &[Ball2D]) -> Vec<String> {
        self.cold.hood.get_neighbour_names(balls)
    }

    #[cfg(test)]
    pub(crate) fn set_accel(&mut self, accel: Vec2D) {
        self.accel = accel;
    }
}

impl Named for Ball2D {
    fn get_name(&self) -> String {
        format!("Ball{}", self.id)
    }
}

impl PartialEq for Ball2D {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl core::fmt::Debug for Ball2D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.write_fmt(format_args!("{}: contact radius/mass {:.3}/{:.3}, pos: ({:.3e},{:.3e}), vel: ({:.3e},{:.3e}), acc: ({:.3e},{:.3e}), brake: {} ",
                             self.get_name(),
                             self.contact_radius,
                             self.mass,
                             self.pos.x,
                             self.pos.y,
                             self.v.x,
                             self.v.y,
                             self.accel.x,
                             self.accel.y,
                             self.braking,
    ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accel_with_speed_limit() {
        let mut b = Ball2D {
            mass: 2.0,
            v: Vec2D::new(3.0, 4.0),
            ..Default::default()
        };
        let dt = 0.1;
        // Case 1: limit above current speed -> no braking
        let acc = b.accel_with_speed_limit(&Vec2D::new(2.0, 0.0), 0., 10.0, dt);
        assert!(!b.braking);
        assert!((acc.x - 1.0).abs() < 1e-6 && (acc.y - 0.0).abs() < 1e-6);

        // Case 2: limit below speed -> braking engaged and accel opposes v
        let acc2 = b.accel_with_speed_limit(&Vec2D::new(0.0, 0.0), 0., 1.0, dt);
        assert!(b.braking);
        // acceleration should have negative dot with velocity to reduce speed
        let dot = acc2.x * b.v.x + acc2.y * b.v.y;
        assert!(dot < 0.0, "accel should oppose velocity, dot={}", dot);

        // Case 3: braking scales with the excess over the limit, not the whole speed
        let acc3 = b.accel_with_speed_limit(&Vec2D::new(0.0, 0.0), 0., 4.0, dt);
        let expected = -0.25 * f32::sqrt(2.) * (5.0 - 4.0) / dt;
        assert!((acc3.norm() + expected).abs() < 1e-4, "braking {acc3:?}");
    }

    #[test]
    fn test_execute_update_pos_and_bounce() {
        let mut b = Ball2D {
            v: Vec2D::new(10.0, 0.0),
            ..Default::default()
        };
        b.set_accel(Vec2D::zero());
        let bound = Rect::new(-2.0, -2.0, 4.0, 4.0); // square from -2..2
        // Move with dt so that we cross right boundary (max x = 2 - contact = 1)
        let bounce_info = b.execute_update_pos(0.2, &bound, [1.0, 1.0], 1.0); // dx = 2.0
        // Expect bounce: position reflected inside, velocity unchanged (bounce handled later)
        assert!(b.pos.x <= 1.0 + 1e-6, "Position should be reflected");
        assert!(bounce_info.box_x.is_some(), "Should detect x bounce");
        assert_eq!(
            b.v.x, 10.0,
            "Velocity should be unchanged during position update"
        );

        // Now move towards bottom boundary in y
        b.v = Vec2D::new(0.0, -10.0);
        b.set_position_and_update_tail(Vec2D::new(0.0, 0.0));
        let bounce_info = b.execute_update_pos(0.2, &bound, [1.0, 1.0], 1.0); // dy = -2.0 -> bounce at y = -1
        assert!(b.pos.y >= -1.0 - 1e-6, "Position should be reflected");
        assert!(bounce_info.box_y.is_some(), "Should detect y bounce");
        assert_eq!(
            b.v.y, -10.0,
            "Velocity should be unchanged during position update"
        );
    }

    #[test]
    fn test_ball_name_and_id_and_moment_and_eq() {
        let mut b1 = Ball2D {
            id: 17,
            mass: 5.0,
            contact_radius: 4.0,
            ..Default::default()
        };
        let b2 = Ball2D {
            id: 17,
            ..Default::default()
        };
        let b3 = Ball2D {
            id: 16,
            ..Default::default()
        };

        assert_eq!(b1.get_name(), "Ball17");
        assert_eq!(b1.get_id(), 17);

        assert_eq!(b1.moment(), Vec2D::new(0.0, 0.0));
        b1.v.x = 3.0; // 3 * mass(5) = 15
        assert_eq!(b1.moment(), Vec2D::new(15.0, 0.0));

        assert_eq!(b1, b2);
        assert_ne!(b1, b3);
    }

    /// The hot part stays small (P6, #346), so the passes over all balls
    /// touch less memory per ball; the bound is the layout, not a cache-line
    /// bound. It rose from 80 to 96 bytes with the f64 position (#213),
    /// which duplicates `pos` (its f32 copy) and overlaps `pre_update_pos`,
    /// and to 112 with the orientation (#348: inertia, θ, ω, α; owner
    /// decision to keep them hot). Do not treat 112 as room to grow: a new
    /// field that not every pass needs belongs in `BallCold`.
    #[test]
    fn test_hot_ball_data_fits_in_112_bytes() {
        let size = std::mem::size_of::<Ball2D>();
        assert!(size <= 112, "Ball2D is {size} bytes");
    }

    /// Far from the origin, steps smaller than the f32 spacing still move the
    /// ball (#213): 1000 steps of 1e-5 at x = 2000 (f32 spacing 1.2e-4).
    #[test]
    fn test_small_steps_accumulate_far_from_the_origin() {
        let mut b = Ball2D::new(1, Vec2D::new(2000.0, 0.0), DEFAULT_SPECIES);
        b.v = Vec2D::new(1e-3, 0.0);
        b.set_accel(Vec2D::zero());
        let bound = Rect::new(-4000.0, 4000.0, 8000.0, -8000.0);
        for _ in 0..1000 {
            b.execute_update_pos(0.01, &bound, [1.0, 1.0], 1.0);
        }
        let moved = b.exact_position()[0] - 2000.0;
        assert!((moved - 1e-2).abs() < 1e-9, "moved {moved}");
        assert!((b.get_position().x - 2000.01).abs() < 2e-4);
    }

    #[test]
    fn test_tail_only_kept_for_verbose_balls() {
        let mut b = Ball2D::new(1, Vec2D::zero(), DEFAULT_SPECIES);
        b.set_position_and_update_tail(Vec2D::new(5.0, 0.0));
        assert!(b.get_tail_positions().is_none());

        b.verbose.toggle_console();
        b.set_position_and_update_tail(Vec2D::new(10.0, 0.0));
        b.set_position_and_update_tail(Vec2D::new(15.0, 0.0));
        assert_eq!(b.get_tail_positions().map(|t| t.len()), Some(2));

        b.verbose.clear_all();
        b.set_position_and_update_tail(Vec2D::new(20.0, 0.0));
        assert!(b.get_tail_positions().is_none());
    }
}
