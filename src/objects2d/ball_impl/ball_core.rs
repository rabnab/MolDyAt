use crate::objects2d::Rect;
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{BallId, Named};
use crate::objects2d::vec_2d::Vec2D;
use crate::objects2d::verbose_info::VerboseInfo;
use std::collections::VecDeque;

const DEFAULT_TAIL_LENGTH: usize = 0;

const TAIL_RESOLUTION_DELTA_SQ: f32 = 3. * 3.; // minimum squared distance to add new tail point

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
        ball.set_position(pos);
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
            let overspeed = f32::min(speed, speed - speed_limit);
            let speed_reduction = 0.25 * f32::sqrt(2.) * overspeed;
            let accel_modification = self.v.scaled(-speed_reduction / dt / speed);

            self.braking = true;
            accel += accel_modification;

            if self.verbose.console {
                println!("braking: overspeed {} at speed {:?}", overspeed, self.v);
            }
            // println!("... F_t: {} and limit: {} for speed {}", total_force, speed_limit, self.v);
            // println!("...... friction_braking:{} and mass: {} accel_res: {:?}", friction_braking, self.mass, accel);
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
    ) -> (f32, f32, f32) {
        let newy = self.pos.y + dy;

        let mut res_y = newy;
        let mut res_vy = self.v.y;
        let mut wall_momentum_y = 0.0;

        if bound_max_y > newy && newy > bound_min_y {
            //nothing to do, inside boundary
        } else if newy <= bound_min_y {
            let overy = bound_min_y - newy;
            res_y = bound_min_y + overy;
            res_vy = -self.v.y;
            // Momentum change: ball loses 2*m*v_y, wall gains 2*m*v_y
            wall_momentum_y = 2.0 * self.mass * self.v.y;
        } else {
            // y is outside top boundary
            let overy = newy - bound_max_y;
            res_y = bound_max_y - overy;
            res_vy = -self.v.y;
            // Momentum change: ball loses 2*m*v_y, wall gains 2*m*v_y
            wall_momentum_y = 2.0 * self.mass * self.v.y;
        }
        (res_y, res_vy, wall_momentum_y)
    }

    fn update_x_and_handle_left_right_boundary(
        &mut self,
        dx: f32,
        bound_min_x: f32,
        bound_max_x: f32,
    ) -> (f32, f32, f32) {
        let newx = self.pos.x + dx;

        let mut res_x = newx;
        let mut res_vx = self.v.x;
        let mut wall_momentum_x = 0.0;

        if bound_max_x > newx && newx > bound_min_x {
            //nothing to do, inside boundary
        } else if newx <= bound_min_x {
            let overx = bound_min_x - newx;
            res_x = bound_min_x + overx;
            res_vx = -self.v.x;
            // Momentum change: ball loses 2*m*v_x, wall gains 2*m*v_x
            wall_momentum_x = 2.0 * self.mass * self.v.x;
        } else {
            // x is outside right boundary
            let overx = newx - bound_max_x;
            res_x = bound_max_x - overx;
            res_vx = -self.v.x;
            // Momentum change: ball loses 2*m*v_x, wall gains 2*m*v_x
            wall_momentum_x = 2.0 * self.mass * self.v.x;
        }

        (res_x, res_vx, wall_momentum_x)
    }

    pub(crate) fn execute_update_pos(&mut self, dt: f32, bound: &Rect) -> (f32, f32) {
        let old_pos = self.pos.clone();
        let dx = self.v.x * dt + 0.5 * self.accel.x * dt * dt;
        let dy = self.v.y * dt + 0.5 * self.accel.y * dt * dt;

        let (x0, y0) = bound.get_tl();
        let (x1, y1) = bound.get_br();

        let bound_min_x = f32::min(x0, x1) + self.radius;
        let bound_max_x = f32::max(x0, x1) - self.radius;
        let bound_min_y = f32::min(y0, y1) + self.radius;
        let bound_max_y = f32::max(y0, y1) - self.radius;
        let (x, vx, wall_px) =
            self.update_x_and_handle_left_right_boundary(dx, bound_min_x, bound_max_x);
        let (y, vy, wall_py) =
            self.update_y_and_handle_top_bottom_boundary(dy, bound_min_y, bound_max_y);

        self.set_position(Vec2D::new(x, y));
        self.v = Vec2D::new(vx, vy);

        if self.verbose.console {
            println!(
                "moved pos {:?} -->  {:?} at dt {:.3} v {:?} a {:?}",
                old_pos, self.pos, dt, self.v, self.accel
            );
        }

        (wall_px, wall_py)
    }

    pub fn set_position(&mut self, new_pos: Vec2D) {
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
        b.execute_update_pos(0.2, &bound); // dx = 2.0
        // Expect bounce: position mirrored inside and v.x flipped
        assert!(b.pos.x <= 1.0 + 1e-6);
        assert!(b.v.x <= 0.0);

        // Now move towards bottom boundary in y
        b.v = Vec2D::new(0.0, -10.0);
        b.pos = Vec2D::new(0.0, 0.0);
        b.execute_update_pos(0.2, &bound); // dy = -2.0 -> bounce at y = -1
        assert!(b.pos.y >= -1.0 - 1e-6);
        assert!(b.v.y >= 0.0);
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
