use crate::objects2d::Rect;
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{BallId, Named};
use crate::objects2d::vec_2d::Vec2D;
use crate::objects2d::verbose_info::VerboseInfo;
use std::collections::VecDeque;

const DEFAULT_TAIL_LENGTH: usize = 1000;

const TAIL_RESOLUTION_DELTA_SQ: f32 = 1. * 1.; // minimum squared distance to add new tail point

/// Information about wall bounces detected during position update.
/// Bounces are applied as impulses during the velocity update step to preserve Verlet integration.
#[derive(Debug, Clone, Copy, Default)]
pub struct BounceInfo {
    /// Momentum that will be transferred to walls (calculated during velocity update)
    pub wall_px: f32,
    pub wall_py: f32,
    /// Whether the ball bounced off left/right wall during position update
    pub bounced_x: bool,
    /// Whether the ball bounced off top/bottom wall during position update
    pub bounced_y: bool,
    /// Velocity before bounce (needed for wall momentum calculation)
    pub v_x_before_bounce: f32,
    pub v_y_before_bounce: f32,
    /// Fraction of timestep before collision (0.0 to 1.0)
    pub collision_time_fraction: f32,
}

pub struct Ball2D {
    pub radius: f32,
    pub size_idx: usize,
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
    pub tail: VecDeque<Vec2D>,
    pub tail_length: usize,
    pub show_update_count: usize,
    /// The cell this ball currently occupies (row, col). None if not assigned to any cell.
    pub current_cell: Option<(usize, usize)>,
}

impl Default for Ball2D {
    fn default() -> Ball2D {
        Ball2D {
            radius: 1.,
            size_idx: usize::MAX,
            pos: Vec2D::zero(),
            id: 0,
            mass: 1.,
            v: Vec2D::zero(),
            hood: Verlet::default(),
            verbose: VerboseInfo::default(),
            braking: false,
            pre_update_pos: Vec2D::zero(),
            accel: Vec2D::zero(),
            last_update_hood_pos: Vec2D::new(f32::INFINITY, f32::INFINITY),
            tail_length: DEFAULT_TAIL_LENGTH,
            tail: VecDeque::with_capacity(DEFAULT_TAIL_LENGTH),
            show_update_count: 0,
            current_cell: None,
        }
    }
}

impl Ball2D {
    pub fn new(id: BallId, pos: Vec2D, radius: (usize, f32)) -> Ball2D {
        let mut ball = Ball2D {
            id,
            pos: pos.clone(),
            radius: radius.1,
            size_idx: radius.0,
            pre_update_pos: pos.clone(),
            last_update_hood_pos: pos.clone(),
            ..Ball2D::default()
        };
        ball.set_position_and_update_tail(pos);
        ball
    }

    pub fn get_id(&self) -> BallId {
        self.id
    }

    pub fn get_size_idx(&self) -> usize {
        self.size_idx
    }

    // Basic position and property access methods
    pub fn get_position(&self) -> &Vec2D {
        &self.pos
    }

    pub fn get_tail_positions(&self) -> Option<&VecDeque<Vec2D>> {
        if self.tail.is_empty() {
            return None;
        }
        Some(&self.tail)
    }

    pub fn get_velocity(&self) -> &Vec2D {
        &self.v
    }

    pub fn get_radius(&self) -> f32 {
        self.radius
    }

    pub fn get_mass(&self) -> f32 {
        self.mass
    }

    // Core state methods
    pub fn is_braking(&self) -> bool {
        self.braking
    }

    pub fn reset_update_counter(&mut self) {
        self.show_update_count = 200;
        self.tail.clear();
    }

    pub fn decrement_update_counter(&mut self) {
        if self.show_update_count > 0 {
            self.show_update_count -= 1;
        }
    }

    pub fn get_update_counter(&self) -> usize {
        self.show_update_count
    }

    pub fn moment(&self) -> Vec2D {
        self.v.scaled(self.mass)
    }

    #[cfg(test)] //Only used in tests
    pub fn get_neighbour_names(&self, balls: &[Ball2D]) -> Vec<String> {
        self.hood.get_neighbour_names(balls)
    }

    // pub fn get_neighbour_force_and_pos(&self) -> Vec<(Vec2D, f32)> {
    //   self.hood
    //       .get_neighbour_force_and_pos(&self.pos, self.radius)
    // }

    pub(crate) fn accel_with_speed_limit(
        &mut self,
        total_force: &Vec2D,
        gravity: f32,
        speed_limit: f32,
        dt: f32,
    ) -> Vec2D {
        let gravi_vec = Vec2D::new(0., -gravity);
        let mut accel: Vec2D = (total_force).scaled(1. / self.mass) + gravi_vec;

        let speed_sq = self.v.norm_sq();

        if speed_sq > speed_limit.powi(2) {
            let speed = f32::sqrt(speed_sq);
            let over_speed = f32::min(speed, speed - speed_limit);
            let speed_reduction = 0.25 * f32::sqrt(2.) * over_speed;

            // Apply braking acceleration opposite to velocity
            let accel_modification = self.v.scaled(-speed_reduction / dt / speed);
            accel += accel_modification;
            self.braking = true;

            if self.verbose.console {
                println!("braking: over_speed {} at speed {:?}", over_speed, self.v);
            }
        } else {
            self.braking = false;
        }
        accel
    }

    pub fn update_old_pos(&mut self) {
        self.pre_update_pos = self.pos.clone();
    }

    fn update_y_and_handle_top_bottom_boundary(
        &mut self,
        dy: f32,
        bound_min_y: f32,
        bound_max_y: f32,
    ) -> (f32, bool, f32) {
        let newy = self.pos.y + dy;

        let mut res_y = newy;
        let bounced;
        let mut collision_fraction = 1.0; // No collision by default (full timestep)

        if bound_max_y > newy && newy > bound_min_y {
            // Inside boundary - no collision
            bounced = false;
        } else if newy <= bound_min_y {
            // Hit bottom boundary - calculate collision time
            let distance_to_boundary = bound_min_y - self.pos.y;
            if dy.abs() > 1e-10 {
                collision_fraction = distance_to_boundary / dy;
                collision_fraction = collision_fraction.clamp(0.0, 1.0);
            }
            // Reflect position
            let over_y = bound_min_y - newy;
            res_y = bound_min_y + over_y;
            bounced = true;
        } else {
            // Hit top boundary - calculate collision time
            let distance_to_boundary = bound_max_y - self.pos.y;
            if dy.abs() > 1e-10 {
                collision_fraction = distance_to_boundary / dy;
                collision_fraction = collision_fraction.clamp(0.0, 1.0);
            }
            // Reflect position
            let over_y = newy - bound_max_y;
            res_y = bound_max_y - over_y;
            bounced = true;
        }
        (res_y, bounced, collision_fraction)
    }

    fn update_x_and_handle_left_right_boundary(
        &mut self,
        dx: f32,
        bound_min_x: f32,
        bound_max_x: f32,
    ) -> (f32, bool, f32) {
        let new_x = self.pos.x + dx;

        let mut res_x = new_x;
        let bounced;
        let mut collision_fraction = 1.0; // No collision by default (full timestep)

        if bound_max_x > new_x && new_x > bound_min_x {
            // Inside boundary - no collision
            bounced = false;
        } else if new_x <= bound_min_x {
            // Hit left boundary - calculate collision time
            let distance_to_boundary = bound_min_x - self.pos.x;
            if dx.abs() > 1e-10 {
                collision_fraction = distance_to_boundary / dx;
                collision_fraction = collision_fraction.clamp(0.0, 1.0);
            }
            // Reflect position
            let over_x = bound_min_x - new_x;
            res_x = bound_min_x + over_x;
            bounced = true;
        } else {
            // Hit right boundary - calculate collision time
            let distance_to_boundary = bound_max_x - self.pos.x;
            if dx.abs() > 1e-10 {
                collision_fraction = distance_to_boundary / dx;
                collision_fraction = collision_fraction.clamp(0.0, 1.0);
            }
            // Reflect position
            let over_x = new_x - bound_max_x;
            res_x = bound_max_x - over_x;
            bounced = true;
        }

        (res_x, bounced, collision_fraction)
    }

    pub(crate) fn execute_update_pos(&mut self, dt: f32, bound: &Rect) -> BounceInfo {
        let old_pos = self.pos.clone();
        let dx = self.v.x * dt + 0.5 * self.accel.x * dt * dt;
        let dy = self.v.y * dt + 0.5 * self.accel.y * dt * dt;

        let (x0, y0) = bound.get_tl();
        let (x1, y1) = bound.get_br();

        let bound_min_x = f32::min(x0, x1) + self.radius;
        let bound_max_x = f32::max(x0, x1) - self.radius;
        let bound_min_y = f32::min(y0, y1) + self.radius;
        let bound_max_y = f32::max(y0, y1) - self.radius;

        // Store velocity before any bounces for momentum calculation
        let v_before = self.v.clone();

        let (x, bounced_x, collision_frac_x) =
            self.update_x_and_handle_left_right_boundary(dx, bound_min_x, bound_max_x);
        let (y, bounced_y, collision_frac_y) =
            self.update_y_and_handle_top_bottom_boundary(dy, bound_min_y, bound_max_y);

        // Use the earliest collision time (if both x and y collide, use the earlier one)
        let collision_time_fraction = if bounced_x && bounced_y {
            collision_frac_x.min(collision_frac_y)
        } else if bounced_x {
            collision_frac_x
        } else if bounced_y {
            collision_frac_y
        } else {
            1.0
        };

        // Update position only - velocity will be modified during velocity update step
        self.set_position_and_update_tail(Vec2D::new(x, y));

        if self.verbose.console {
            println!(
                "moved pos {:?} -->  {:?} at dt {:.3} v {:?} a {:?}",
                old_pos, self.pos, dt, self.v, self.accel
            );
        }

        BounceInfo {
            wall_px: 0.0, // Will be calculated during velocity update
            wall_py: 0.0,
            bounced_x,
            bounced_y,
            v_x_before_bounce: v_before.x,
            v_y_before_bounce: v_before.y,
            collision_time_fraction,
        }
    }

    pub fn set_position_and_update_tail(&mut self, new_pos: Vec2D) {
        let end_pos = self.tail.back();
        if let Some(last_pos) = end_pos {
            let dist = last_pos.dist(&new_pos);
            if dist.norm_sq() > TAIL_RESOLUTION_DELTA_SQ {
                self.tail.push_back(new_pos.clone());
            }
        } else {
            self.tail.push_back(new_pos.clone());
        }
        if self.tail.len() > self.tail_length {
            self.tail.pop_front();
        }
        self.pos = new_pos;
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
        f.write_fmt(format_args!("{}: radius/mass {:.3}/{:.3}, pos: ({:.3e},{:.3e}), vel: ({:.3e},{:.3e}), acc: ({:.3e},{:.3e}), brake: {} ",
                             self.get_name(),
                             self.radius,
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
        let mut b = Ball2D::default();
        b.mass = 2.0;
        b.v = Vec2D::new(3.0, 4.0); // speed 5
        let dt = 0.1;
        // Case 1: limit above current speed -> no braking
        let acc = b.accel_with_speed_limit(&Vec2D::new(2.0, 0.0), 0., 10.0, dt);
        assert_eq!(b.braking, false);
        assert!((acc.x - 1.0).abs() < 1e-6 && (acc.y - 0.0).abs() < 1e-6);

        // Case 2: limit below speed -> braking engaged and accel opposes v
        let acc2 = b.accel_with_speed_limit(&Vec2D::new(0.0, 0.0), 0., 1.0, dt);
        assert!(b.braking);
        // acceleration should have negative dot with velocity to reduce speed
        let dot = acc2.x * b.v.x + acc2.y * b.v.y;
        assert!(dot < 0.0, "accel should oppose velocity, dot={}", dot);
    }

    #[test]
    fn test_execute_update_pos_and_bounce() {
        let mut b = Ball2D::default();
        b.radius = 1.0;
        b.pos = Vec2D::new(0.0, 0.0);
        b.v = Vec2D::new(10.0, 0.0);
        b.set_accel(Vec2D::zero());
        let bound = Rect::new(-2.0, -2.0, 4.0, 4.0); // square from -2..2
        // Move with dt so that we cross right boundary (max x = 2 - radius = 1)
        let bounce_info = b.execute_update_pos(0.2, &bound); // dx = 2.0
        // Expect bounce: position reflected inside, velocity unchanged (bounce handled later)
        assert!(b.pos.x <= 1.0 + 1e-6, "Position should be reflected");
        assert!(bounce_info.bounced_x, "Should detect x bounce");
        assert_eq!(
            b.v.x, 10.0,
            "Velocity should be unchanged during position update"
        );

        // Now move towards bottom boundary in y
        b.v = Vec2D::new(0.0, -10.0);
        b.pos = Vec2D::new(0.0, 0.0);
        let bounce_info = b.execute_update_pos(0.2, &bound); // dy = -2.0 -> bounce at y = -1
        assert!(b.pos.y >= -1.0 - 1e-6, "Position should be reflected");
        assert!(bounce_info.bounced_y, "Should detect y bounce");
        assert_eq!(
            b.v.y, -10.0,
            "Velocity should be unchanged during position update"
        );
    }

    #[test]
    fn test_ball_name_and_id_and_moment_and_eq() {
        let mut b1 = Ball2D::default();
        b1.id = 17;
        b1.mass = 5.0;
        b1.radius = 4.0;
        let mut b2 = Ball2D::default();
        b2.id = 17; // same id -> equal
        let mut b3 = Ball2D::default();
        b3.id = 16; // different id -> not equal

        assert_eq!(b1.get_name(), "Ball17");
        assert_eq!(b1.get_id(), 17);

        assert_eq!(b1.moment(), Vec2D::new(0.0, 0.0));
        b1.v.x = 3.0; // 3 * mass(5) = 15
        assert_eq!(b1.moment(), Vec2D::new(15.0, 0.0));

        assert_eq!(b1, b2);
        assert_ne!(b1, b3);
    }
}
