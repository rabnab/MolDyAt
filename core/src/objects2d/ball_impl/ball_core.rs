// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Rect;
use crate::objects2d::ball_impl::species::{DEFAULT_SPECIES, Species, SpeciesId, SpeciesTable};
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
#[derive(Debug, Clone, Copy, Default)]
pub struct BounceInfo {
    /// Contact with the left or right box wall.
    pub box_x: Option<WallContact>,
    /// Contact with the top or bottom box wall.
    pub box_y: Option<WallContact>,
    /// Contact with a line-segment wall (see `SimulationWorld::sweep_wall_contact`).
    pub line_wall: Option<WallContact>,
}

impl BounceInfo {
    /// Whether the ball touched a box wall this step.
    pub fn touched_box(&self) -> bool {
        self.box_x.is_some() || self.box_y.is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = &WallContact> {
        [&self.box_x, &self.box_y, &self.line_wall]
            .into_iter()
            .flatten()
    }
}

pub struct Ball2D {
    /// `σ / 2` of the species: the ball's size for overlap and placement
    /// checks. Wall contacts take their distance from the `WallLaw`.
    pub contact_radius: f32,
    pub species: SpeciesId,
    pos: Vec2D,
    pub(crate) id: BallId,
    pub mass: f32,
    pub v: Vec2D,
    pub(crate) hood: Verlet,
    pub verbose: VerboseInfo,
    pub braking: bool,
    pub pre_update_pos: Vec2D,
    pub accel: Vec2D,
    pub last_update_hood_pos: Vec2D,
    /// Position history, kept only while the ball is verbose (allocated lazily;
    /// `None` otherwise).
    tail: Option<VecDeque<Vec2D>>,
    pub show_update_count: usize,
    /// The cell this ball currently occupies (row, col). None if not assigned to any cell.
    pub current_cell: Option<(usize, usize)>,
    /// A wall appeared or changed on top of this ball (a valve closed, a wall
    /// was drawn, moved or thickened over it). Until it no longer touches any
    /// wall, it passes through the walls it overlaps instead of being pushed.
    pub leaving_wall: bool,
}

impl Default for Ball2D {
    fn default() -> Ball2D {
        let species = SpeciesTable::builtin().species(DEFAULT_SPECIES);
        Ball2D {
            contact_radius: species.contact_radius(),
            species: DEFAULT_SPECIES,
            pos: Vec2D::zero(),
            id: 0,
            mass: species.mass,
            v: Vec2D::zero(),
            hood: Verlet::default(),
            verbose: VerboseInfo::default(),
            braking: false,
            pre_update_pos: Vec2D::zero(),
            accel: Vec2D::zero(),
            last_update_hood_pos: Vec2D::new(f32::INFINITY, f32::INFINITY),
            tail: None,
            show_update_count: 0,
            current_cell: None,
            leaving_wall: false,
        }
    }
}

impl Ball2D {
    /// A ball of a built-in `species` at rest, with its mass and contact
    /// radius. Sets pre_update_pos and last_update_hood_pos to the initial
    /// position.
    pub fn new(id: BallId, pos: Vec2D, species: SpeciesId) -> Ball2D {
        Self::of_species(id, pos, species, SpeciesTable::builtin().species(species))
    }

    /// A ball of species `species` (with the data `data` of that species, from
    /// the world's table) at rest. Sets pre_update_pos and
    /// last_update_hood_pos to the initial position.
    pub fn of_species(id: BallId, pos: Vec2D, species: SpeciesId, data: &Species) -> Ball2D {
        Ball2D {
            id,
            pos,
            species,
            contact_radius: data.contact_radius(),
            mass: data.mass,
            pre_update_pos: pos,
            last_update_hood_pos: pos,
            ..Ball2D::default()
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

    pub fn get_tail_positions(&self) -> Option<&VecDeque<Vec2D>> {
        self.tail.as_ref().filter(|tail| !tail.is_empty())
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
    pub fn get_edge_distance(&self, other: &Ball2D) -> f32 {
        let center_dist = self.get_position().dist(other.get_position()).norm();
        center_dist - (self.contact_radius + other.contact_radius)
    }

    // Core state methods
    pub fn is_braking(&self) -> bool {
        self.braking
    }

    /// Reset the "show update" counter after a neighborhood (Verlet list) update.
    ///
    /// This also clears the tail so that the rendered trajectory only represents
    /// movement **since the last neighborhood update**, not the full history.
    /// Callers relying on long-lived trajectory visuals should be aware that
    /// invoking this method will drop the existing tail points.
    pub fn reset_update_counter(&mut self) {
        self.show_update_count = 200;
        if let Some(tail) = self.tail.as_mut() {
            tail.clear();
        }
    }

    pub fn decrement_update_counter(&mut self) {
        if self.show_update_count > 0 {
            self.show_update_count -= 1;
        }
    }

    pub fn get_update_counter(&self) -> usize {
        self.show_update_count
    }

    /// Calculate linear momentum (mass × velocity).
    ///
    /// # Returns
    /// Momentum vector
    pub fn moment(&self) -> Vec2D {
        self.v.scaled(self.mass)
    }

    pub(crate) fn accel_with_speed_limit(
        &mut self,
        total_force: &Vec2D,
        gravity: f32,
        speed_limit: f32,
        dt: f32,
    ) -> Vec2D {
        let gravi_vec = Vec2D::new(0., -gravity);
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
    pub fn update_old_pos(&mut self) {
        self.pre_update_pos = self.pos;
    }

    /// Resolve one box axis: the ball's centre must stay in `[min, max]`.
    /// Returns the new coordinate and the contact, if the ball reached a wall.
    fn resolve_box_axis(
        pos: f32,
        velocity: f32,
        accel: f32,
        (min, max): (f32, f32),
        dt: f32,
        restitution: f32,
    ) -> (f32, Option<(f32, NormalPosition)>) {
        // Sum the displacement first: adding its terms to a large coordinate
        // one by one would round away most of the small ½·a·dt² term.
        let displacement = velocity * dt + 0.5 * accel * dt * dt;
        let free = pos + displacement;
        if free <= min {
            // Normal +axis: distance from the wall is `pos - min`.
            if let Some(n) = resolve_normal_position(pos - min, velocity, accel, dt, restitution) {
                return (min + n.distance, Some((1.0, n)));
            }
        } else if free >= max {
            // Normal -axis: distance from the wall is `max - pos`.
            if let Some(n) = resolve_normal_position(max - pos, -velocity, -accel, dt, restitution)
            {
                return (max - n.distance, Some((-1.0, n)));
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
    /// * `contact` - Centre distance from a wall at which the ball is
    ///   reflected (`BallPhysics::wall_contact_distance`)
    /// * `restitution` - Normal restitution of the walls (1.0 = elastic)
    pub(crate) fn execute_update_pos(
        &mut self,
        dt: f32,
        bound: &Rect,
        contact: f32,
        restitution: f32,
    ) -> BounceInfo {
        let old_pos = self.pos;
        let (x0, y0) = bound.get_tl();
        let (x1, y1) = bound.get_br();
        let x_range = (f32::min(x0, x1) + contact, f32::max(x0, x1) - contact);
        let y_range = (f32::min(y0, y1) + contact, f32::max(y0, y1) - contact);

        let (x, x_contact) =
            Self::resolve_box_axis(self.pos.x, self.v.x, self.accel.x, x_range, dt, restitution);
        let (y, y_contact) =
            Self::resolve_box_axis(self.pos.y, self.v.y, self.accel.y, y_range, dt, restitution);

        self.set_position_and_update_tail(Vec2D::new(x, y));

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
        }
    }

    /// Set ball position and, for verbose balls only, add to the tail trail
    /// if movement exceeds the threshold.
    /// Only adds to tail if distance from last tail point exceeds TAIL_RESOLUTION_DELTA_SQ.
    /// Maintains tail length by removing oldest points when limit is exceeded.
    /// A ball that is no longer verbose drops its tail.
    ///
    /// # Arguments
    /// * `new_pos` - New position to set
    pub fn set_position_and_update_tail(&mut self, new_pos: Vec2D) {
        if self.verbose.any() {
            let tail = self.tail.get_or_insert_with(Default::default);
            let far_enough = tail
                .back()
                .is_none_or(|last| last.dist(&new_pos).norm_sq() > TAIL_RESOLUTION_DELTA_SQ);
            if far_enough {
                tail.push_back(new_pos);
                if tail.len() > TAIL_LENGTH {
                    tail.pop_front();
                }
            }
        } else {
            self.tail = None;
        }
        self.pos = new_pos;
    }

    #[cfg(test)]
    pub fn get_neighbour_names(&self, balls: &[Ball2D]) -> Vec<String> {
        self.hood.get_neighbour_names(balls)
    }

    #[cfg(test)]
    pub fn set_accel(&mut self, accel: Vec2D) {
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
            pos: Vec2D::new(0.0, 0.0),
            v: Vec2D::new(10.0, 0.0),
            ..Default::default()
        };
        b.set_accel(Vec2D::zero());
        let bound = Rect::new(-2.0, -2.0, 4.0, 4.0); // square from -2..2
        // Move with dt so that we cross right boundary (max x = 2 - contact = 1)
        let bounce_info = b.execute_update_pos(0.2, &bound, 1.0, 1.0); // dx = 2.0
        // Expect bounce: position reflected inside, velocity unchanged (bounce handled later)
        assert!(b.pos.x <= 1.0 + 1e-6, "Position should be reflected");
        assert!(bounce_info.box_x.is_some(), "Should detect x bounce");
        assert_eq!(
            b.v.x, 10.0,
            "Velocity should be unchanged during position update"
        );

        // Now move towards bottom boundary in y
        b.v = Vec2D::new(0.0, -10.0);
        b.pos = Vec2D::new(0.0, 0.0);
        let bounce_info = b.execute_update_pos(0.2, &bound, 1.0, 1.0); // dy = -2.0 -> bounce at y = -1
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
