use crate::objects2d::ball_impl::verbose_info::VerboseInfo;
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{BallId, Named};
use crate::objects2d::vec_2d::Vec2D;
use ggez::graphics::{Color, Rect};

pub struct Ball2D<'a> {
    pub radius: f32,
    pub pos: Vec2D,
    pub(crate) id: BallId,
    pub mass: f32,
    pub v: Vec2D,
    pub(crate) hood: Verlet<'a>,
    pub verbose: VerboseInfo,
    pub braking: bool,
    pub pre_update_pos: Vec2D,
    pub accel: Vec2D,
    pub last_update_hood_pos: Vec2D,
}

impl<'a> Default for Ball2D<'a> {
    fn default() -> Ball2D<'a> {
        Ball2D {
            radius: 1.,
            pos: Vec2D::new(0., 0.),
            id: 0,
            mass: 1.,
            v: Vec2D::new(0., 0.),
            hood: Verlet::default(),
            verbose: VerboseInfo::default(),
            braking: false,
            pre_update_pos: Vec2D::new(0., 0.),
            accel: Vec2D::new(0., 0.),
            last_update_hood_pos: Vec2D::new(f32::INFINITY, f32::INFINITY),
        }
    }
}

impl<'a> Ball2D<'a> {
    pub fn get_id(&self) -> usize {
        self.id
    }

    // Basic position and property access methods
    pub fn get_position(&self) -> &Vec2D {
        &self.pos
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

    pub fn ball_color(&self) -> Option<Color> {
        if self.verbose.console || self.verbose.lines() {
            return Some(Color::BLUE);
        }
        if self.braking {
            return Some(Color::YELLOW);
        }

        None
    }

    #[cfg(test)] //Only used in tests
    pub fn get_neighbour_names(&self) -> Vec<String> {
        self.hood.get_neighbour_names()
    }

    // pub fn get_neighbour_force_and_pos(&self) -> Vec<(Vec2D, f32)> {
    //   self.hood
    //       .get_neighbour_force_and_pos(&self.pos, self.radius)
    // }

    pub(crate) fn accel_with_speed_limit(
        &mut self,
        total_force: &Vec2D,
        speed_limit: f32,
        dt: f32,
    ) -> Vec2D {
        let mut accel: Vec2D = total_force.scaled(1. / self.mass);

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
    ) {
        let newy = self.pos.y + dy;

        if bound_max_y > newy && newy > bound_min_y {
            self.pos.y += dy;
        } else if newy <= bound_min_y {
            let overy = bound_min_y - newy;
            self.pos.y = bound_min_y + overy;
            self.v.y = -self.v.y;
        } else {
            // y is outside right boundary
            let overy = newy - bound_max_y;
            self.pos.y = bound_max_y - overy;
            self.v.y = -self.v.y;
        }
    }

    fn update_x_and_handle_left_right_boundary(
        &mut self,
        dx: f32,
        bound_min_x: f32,
        bound_max_x: f32,
    ) {
        let newx = self.pos.x + dx;

        if bound_max_x > newx && newx > bound_min_x {
            self.pos.x += dx;
        } else if newx <= bound_min_x {
            let overx = bound_min_x - newx;
            self.pos.x = bound_min_x + overx;
            self.v.x = -self.v.x;
        } else {
            // x is outside right boundary
            let overx = newx - bound_max_x;
            self.pos.x = bound_max_x - overx;
            self.v.x = -self.v.x;
        }
    }

    pub(crate) fn execute_update_pos(&mut self, dt: f32, bound: &Rect) {
        let old_pos = self.pos.clone();
        let dx = self.v.x * dt + 0.5 * self.accel.x * dt * dt;
        let dy = self.v.y * dt + 0.5 * self.accel.y * dt * dt;

        let bound_min_x = bound.x + self.radius;
        let bound_max_x = bound.x + bound.w - self.radius;
        let bound_min_y = bound.y + self.radius;
        let bound_max_y = bound.y + bound.h - self.radius;
        self.update_x_and_handle_left_right_boundary(dx, bound_min_x, bound_max_x);
        self.update_y_and_handle_top_bottom_boundary(dy, bound_min_y, bound_max_y);

        if self.verbose.console {
            println!(
                "moved pos {:?} -->  {:?} at dt {:.3} v {:?} a {:?}",
                old_pos, self.pos, dt, self.v, self.accel
            );
        }
    }

    #[cfg(test)]
    pub fn set_accel(&mut self, accel: Vec2D) {
        self.accel = accel;
    }
}

impl<'a> Named for Ball2D<'a> {
    fn get_name(&self) -> String {
        format!("Ball{}", self.id)
    }
}

impl<'a> PartialEq for Ball2D<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<'a> core::fmt::Debug for Ball2D<'a> {
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
