use crate::objects2d::vec_2d::Vec2D;
use ggez::graphics::Rect;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

// const MAX_UPDATE_RECURSION: i32 = 10;
//const LJ_SIG: f32 = 5.; // equilibrium distance
//LJ_SIG in SI units is for Argon 3.4A=3.4E-10m
pub(crate) const LJ_SIG: f32 = 1.; // equilibrium distance
pub(crate) const LJ_CUTOFF: f32 = 8. * LJ_SIG; //distance where lenard jones force is < 1e-4 (as long as sig>0.1)
pub(crate) const LJ_EPS: f32 = 1.; // attractive strength
const SQRT6_OF_TWO: f32 = 1.122462048309372981; // == 2^(1/6)
static mut UPDATE_HOOD_CNT: usize = 0;

pub struct Ball2D<'a> {
    pub radius: f32,
    pub pos: Vec2D,
    pub(crate) id: usize,
    pub mass: f32,
    pub v: Vec2D,
    pub(crate) hood: Verlet<'a>,
    pub verbose: VerboseInfo,
    braking: bool,
    pub pre_update_pos: Vec2D,
    pub accel: Vec2D,
    last_update_hood_pos: Vec2D,
}

pub(crate) struct Verlet<'a> {
    pub neighbours: RefCell<Vec<Weak<RefCell<Ball2D<'a>>>>>,
    pub nearest: f32,
    pub farthest: f32,
}

pub enum VerboseInfoMode {
    CONSOLE,
    MAIN,
    OTHER,
    SPEED,
}
pub(crate) struct VerboseInfo {
    console: bool,
    main_neighbour: bool,
    other_neighbour: bool,
    speed: bool,
}

impl VerboseInfo {
    fn default() -> VerboseInfo {
        VerboseInfo {
            console: false,
            main_neighbour: false,
            other_neighbour: false,
            speed: false,
        }
    }

    pub(crate) fn lines(&self) -> bool {
        self.main_neighbour || self.other_neighbour || self.speed
    }

    pub(crate) fn toggle_console(&mut self) {
        self.console = !self.console;
    }

    pub(crate) fn toggle_main_neighbour(&mut self) {
        self.main_neighbour = !self.main_neighbour;
    }

    pub(crate) fn toggle_other_neighbour(&mut self) {
        self.other_neighbour = !self.other_neighbour;
    }

    pub(crate) fn toggle_speed(&mut self) {
        self.speed = !self.speed;
    }

    // pub(crate) fn any(&self) -> bool {
    //     self.console || self.speed || self.main_neighbour || self.other_neighbour
    // }

    pub(crate) fn is_main(&self) -> bool {
        self.main_neighbour
    }
    pub(crate) fn clear_all(&mut self) {
        self.console = false;
        self.main_neighbour = false;
        self.other_neighbour = false;
        self.speed = false;
    }
}

impl<'a> Default for Verlet<'a> {
    fn default() -> Verlet<'a> {
        Verlet {
            neighbours: RefCell::new(Vec::new()),
            nearest: f32::NAN,
            farthest: f32::NAN,
        }
    }
}

impl<'a> core::fmt::Debug for Verlet<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.write_fmt(format_args!(
            "nearest/farthest {:.3e}/{:.3e} num neighbours {}",
            self.nearest,
            self.farthest,
            self.neighbours.borrow().len()
        ))
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

impl<'a> Verlet<'a> {
    /// Retrieve the names of all valid neighbours.
    pub fn get_neighbour_names(&self) -> Vec<String> {
        self.neighbours
            .borrow()
            .iter()
            .filter_map(|weak| weak.upgrade()) // Filter out dropped references
            .map(|rcrefcell| rcrefcell.borrow().get_name())
            .collect()
    }

    fn add_neighbour_already_sure_was_no_neighbour(&self, ball_rc: &Rc<RefCell<Ball2D<'a>>>) {
        self.neighbours.borrow_mut().push(Rc::downgrade(ball_rc));
    }
    pub fn add_neighbour(
        &self,
        self_rc: &Rc<RefCell<Ball2D<'a>>>,
        ball_rc: &Rc<RefCell<Ball2D<'a>>>,
    ) -> bool {
        let was_neighbour = self.is_a_neighbour(&ball_rc.borrow().get_name());
        if !was_neighbour {
            self.neighbours.borrow_mut().push(Rc::downgrade(ball_rc));
            ball_rc
                .borrow()
                .hood
                .add_neighbour_already_sure_was_no_neighbour(self_rc);
        }
        was_neighbour
    }

    pub fn is_a_neighbour(&self, name: &str) -> bool {
        self.neighbours
            .borrow()
            .iter()
            .filter_map(|weak| weak.upgrade())
            .any(|rcrefcell| rcrefcell.borrow().get_name() == name)
    }

    pub fn remove(&mut self, name: &str) {
        self.neighbours.borrow_mut().retain(|weak| {
            weak.upgrade()
                .is_some_and(|ball_rc| ball_rc.borrow().get_name() != name)
        });
    }

    pub fn get_neighbour_force_and_pos(
        &self,
        ball_pos: &Vec2D,
        ball_radius: f32,
    ) -> Vec<(Vec2D, f32)> {
        self.neighbours
            .borrow()
            .iter()
            .filter_map(|weak| weak.upgrade()) // Filter out dropped references
            .map(|rcrefcell| {
                let other = rcrefcell.borrow();
                (
                    other.pos.clone(),
                    Ball2D::lenard_jones_force(
                        ball_pos.dist(&other.pos).norm(),
                        ball_radius + other.radius,
                        other.get_id(),
                    ),
                )
            })
            .collect()
    }
}

pub(crate) trait Named {
    fn get_name(&self) -> String;
}

// pub(crate) trait Verbose {
//     fn console(&self) -> bool;
//     fn main_neighbour(&self) -> bool;
//     fn other_neighbour(&self) -> bool;
//     fn speed(&self) -> bool;
// }

impl<'a> Named for Ball2D<'a> {
    fn get_name(&self) -> String {
        format!("Ball{}", self.id)
    }
}

#[derive(PartialEq, Debug)]
pub(crate) enum BallError {
    DistanceUnderflow,
    // LessThanTwoBalls,
}

type BallResult = Result<f32, BallError>;

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

impl<'a> PartialEq for Ball2D<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<'a> Ball2D<'a> {
    pub fn _new_sized(id: usize, x: f32, y: f32, radius: f32) -> Ball2D<'a> {
        Ball2D::_new_massive(id, x, y, radius, 1.)
    }

    pub fn _new_massive(_id: usize, _x: f32, _y: f32, _radius: f32, _m: f32) -> Ball2D<'a> {
        Ball2D::new_massive_with_v(_id, _x, _y, _radius, _m, None)
    }

    pub fn new_massive_with_v(
        _id: usize,
        _x: f32,
        _y: f32,
        _radius: f32,
        _m: f32,
        v: Option<Vec2D>,
    ) -> Ball2D<'a> {
        let pos = Vec2D::new(_x, _y);
        Ball2D {
            id: _id,
            pos: pos.clone(),
            radius: _radius,
            mass: _m,
            v: if v.is_none() {
                Vec2D::new(0., 0.)
            } else {
                v.unwrap()
            },
            pre_update_pos: pos,
            ..Default::default()
        }
    }

    pub fn get_update_hood_call_cnt() -> usize {
        unsafe { UPDATE_HOOD_CNT }
    }

    // pub fn has_larger_id(&self, other: &Self) -> bool {
    //     self.id > other.id
    // }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn moment(&self) -> Vec2D {
        self.v.scaled(self.mass)
    }

    pub fn needs_update_hood(&self, dt: f32) -> bool {
        let travel_since_last_upd_sq = self
            .last_update_hood_pos
            .add(self.v.scaled(dt))
            .dist(&self.pos)
            .norm_sq();
        return (self.radius + LJ_CUTOFF) * (self.radius + LJ_CUTOFF) < travel_since_last_upd_sq; //TODO this is not correct
    }

    pub fn update_hood(
        &mut self,
        dt: f32,
        self_rc: &Rc<RefCell<Self>>,
        others: Vec<Rc<RefCell<Ball2D<'a>>>>,
    ) {
        let cutoff = self.radius + LJ_CUTOFF * 2. + self.v.norm() * dt;

        let dist_limit_by_v_sq = 0.;
        //we only check hood every ten updates --> factors 100 and 10
        //100. * v_sq * dt * dt + eq * eq + 2. * 10. * dt * eq * (self.v.x + self.v.y);
        let dist_limit_by_closest_sq = cutoff * cutoff;
        let dist_limit_sq = f32::max(dist_limit_by_closest_sq, dist_limit_by_v_sq);
        if self.verbose.console {
            println!(
                "update_hood: {:?} dist_lim: {} equi_dist: {} #others: {}",
                self,
                f32::sqrt(dist_limit_sq),
                self.equilibrium_radius(),
                others.len()
            );
        }
        let this_ball_needs_update = self.needs_update_hood(dt);
        if !this_ball_needs_update {
            return;
        }
        unsafe {
            UPDATE_HOOD_CNT += 1;
        }
        self.last_update_hood_pos = self.pos.clone();
        for cur_rc in others.iter() {
            let cur_ref = cur_rc.borrow();
            // if cur_ref.needs_update_hood(){
            //     let other_for_cur = others.clone();
            //     other_for_cur.retain(|b_rc| b_rc.borrow().get_name() != cur_ref.get_name());
            //     other_for_cur.push(self_rc.clone());
            //     cur_ref.update_hood(dt, cur_rc, other_for_cur);
            // }
            // if self.hood.is_a_neighbour(&cur_ref.get_name()) {
            //     continue;
            // }

            let dist_sq = self.pos.dist(&cur_ref.pos).norm_sq();
            //println!("checking {}", cur_ref.get_name());
            if dist_sq < dist_limit_sq {
                //cur.hood.neighbours.push(self.into());
                //let new_neighbour = Rc::new( RefCell::new(cur));
                self.hood.add_neighbour(self_rc, cur_rc);

                if self.verbose.console {
                    println!(
                        "\tneighbours {} at ({},{}) dist {}",
                        cur_ref.get_name(),
                        cur_ref.pos.x,
                        cur_ref.pos.y,
                        f32::sqrt(dist_sq)
                    );
                } else if cur_ref.verbose.console {
                    println!(
                        "\t {} is neighbour of {} since its dist is {} (dist_limit {})",
                        cur_ref.get_name(),
                        self.get_name(),
                        f32::sqrt(dist_sq),
                        f32::sqrt(dist_limit_sq)
                    )
                }
                //println!("updated {}-neighbours {:?}", self.get_name(), self.hood.neighbours
                //.borrow()
                //.iter()
                //.filter_map(|weak| weak.upgrade())
                //.map(|val| val.borrow().get_name())
                //.collect::<Vec<String>>());
                //println!("updated {}-neighbours {:?}", cur.get_name(), cur.hood.neighbours);
            } else {
                self.hood.remove(&cur_ref.get_name());
            }
        }
    }

    pub fn update_closeness_stats(&mut self) {
        let actual_hood = self.hood.neighbours.borrow();

        let (nearest_sq, farthest_sq) = actual_hood
            .iter()
            .filter_map(|weak| weak.upgrade())
            .map(|val| {
                let d = self.pos.dist(&val.borrow().pos).norm_sq();
                d
            })
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), val| {
                (min.min(val), max.max(val))
            });

        self.hood.nearest = if nearest_sq.is_finite() {
            f32::sqrt(nearest_sq)
        } else {
            f32::NAN
        };
        self.hood.farthest = if nearest_sq.is_finite() {
            f32::sqrt(farthest_sq)
        } else {
            f32::NAN
        };

        if self.verbose.console {
            println!(
                "update_closeness: {} nearest/farthest: {}/{}",
                self.get_name(),
                self.hood.nearest,
                self.hood.farthest
            );
        }
    }

    pub fn is_colliding(&self, other: &Ball2D) -> bool {
        //use norm_sq to avoid sqrt in norm
        let radius_sum_sq =
            (self.radius + other.radius + LJ_SIG) * (self.radius + other.radius + LJ_SIG);
        let res = self.pos.dist(&other.pos).norm_sq() < radius_sum_sq;
        if res {
            println!(
                "ball {} at {} is too close to {}: dist = {} sum_rad = {}",
                self.id,
                self.pos,
                other.pos,
                self.pos.dist(&other.pos).norm(),
                self.radius + other.radius
            )
        }
        res
    }

    pub fn equilibrium_radius(&self) -> f32 {
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        Self::equilibrium_of_radius(self.radius)
    }

    pub(crate) fn equilibrium_of_radius(radius: f32) -> f32 {
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        (SQRT6_OF_TWO * LJ_SIG) * 0.5 + radius
    }

    pub(crate) fn lenard_jones_potential(dist_btw_balls: f32) -> f32 {
        //case dist_of_balls = equilibrium_radius*2 and radius_sum=2*radius
        //dist = SQRT6_OF_TWO LJ_SIG + 2*rad - 2*rad
        //dist = SQRT6_OF_TWO LJ_SIG
        //quot = LJ_SIG/dist = 1/SQRT6_OF_TWO
        //quot_six = 1/2
        //potential: 4 LJ_EPS 1/2 *(1/2-1)
        //    4 LJ_EPS 1/2 * (-1/2)
        //    - LJ_EPS

        if dist_btw_balls < Self::min_dist_for_const_force() {
            let quot = LJ_SIG / Self::min_dist_for_const_force();
            let quot_six = quot.powi(6);
            let pot_at_min_dist = 4. * LJ_EPS * quot_six * (quot_six - 1.);

            let force_at_min_dist = Self::lenard_jones_force_pure(Self::min_dist_for_const_force());
            return pot_at_min_dist
                + (dist_btw_balls - Self::min_dist_for_const_force()) * force_at_min_dist;
        }

        let quot = LJ_SIG / dist_btw_balls;
        let quot_six = quot.powi(6);
        4. * LJ_EPS * quot_six * (quot_six - 1.)
    }

    fn min_dist_for_const_force() -> f32 {
        LJ_SIG * 0.5
    }

    pub(crate) fn lenard_jones_force(dist_of_balls: f32, radius_sum: f32, _id: usize) -> f32 {
        let dist = dist_of_balls - radius_sum;
        if dist < Self::min_dist_for_const_force() {
            Self::lenard_jones_force_pure(Self::min_dist_for_const_force())
        } else {
            Self::lenard_jones_force_pure(dist)
        }
    }
    pub(crate) fn lenard_jones_force_pure(dist_btw_balls: f32) -> f32 {
        let quot = LJ_SIG / dist_btw_balls;
        let quot_six = quot.powi(6);
        let res = -24. * LJ_EPS * quot_six * (2. * quot_six - 1.) / dist_btw_balls;

        //0==6(2*LJ_SIG/dist6-1)*const/dist
        //0==2*LJ_SIG/dist6-1
        //LJ_SIG/sqrt6(1/2)==dist
        //LJ_SIG/sqrt6(1/2)==dist
        //1.12 *LJ_SIG == dist_equi - radius_sum
        //SQRT6_OF_TWO * LJ_SIG == dist_equi
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        //
        //println!("{} ------------ dist: {} ljForce: {} rad_sum: {} quot: {} r_m: {}",
        //   name, dist, res, radius_sum, quot, r_m);

        //equilibrium distance:
        //2.036 rad+1.14 SIG == dist_equi
        //equilibrium radius:
        //1.018* self.radius + 0.57*LJ_SIG
        res
    }

    pub(crate) fn check_min_dist(actual_dist: f32, min_dist: f32) -> BallResult {
        // let wiggle_room = LJ_SIG*0.1;
        // if actual_dist < 0. {
        //     return Err(BallError::LessThanTwoBalls);
        // }
        if actual_dist < min_dist || f32::is_nan(min_dist) {
            if actual_dist <= 0. {
                return Err(BallError::DistanceUnderflow);
            }
            return Ok(actual_dist);
        }
        Ok(min_dist)
    }

    //fn force_by(&self, others: &Vec<Ball2D>)  -> (f32, Vec2D, f32) {
    pub(crate) fn force(&self) -> (f32, Vec2D, f32) {
        let mut force = Vec2D::new(0., 0.);
        let mut min_dist = f32::NAN;
        let mut total_pot = 0.;
        //for b in others {
        let the_neighbours = self.hood.neighbours.borrow();

        for ref_b in the_neighbours.iter() {
            if let Some(b_rc) = ref_b.upgrade() {
                //from other_ball to this_ball
                let b = b_rc.borrow();
                let dist_vec = self.pos.dist(&b.pre_update_pos);
                let dist = dist_vec.norm();
                let radius_sum = b.radius + self.radius;
                /*min_dist = match Ball2D::check_min_dist(dist - radius_sum*0.9, min_dist) {
                    Err(BallError::DistanceUnderflow) | Err(BallError::LessThanTwoBalls) => return(0., Vec2D::new(0.,0.), 0.),
                    Ok(dist_result) => dist_result,
                };*/

                match Ball2D::check_min_dist(dist - radius_sum, min_dist) {
                    Ok(min_dist_cur) => {
                        min_dist = min_dist_cur;
                        let lj_force = Ball2D::lenard_jones_force(dist, radius_sum, self.id);
                        let force_to_dist_val = lj_force / dist;
                        let local_force = Vec2D::new(
                            dist_vec.x * force_to_dist_val,
                            dist_vec.y * force_to_dist_val,
                        );

                        if self.verbose.console {
                            println!(
                                "\t\t {}--force: {} ({},{}) at dist: {}",
                                b.get_name(),
                                local_force.norm(),
                                local_force.x,
                                local_force.y,
                                dist
                            );
                        }

                        force.add_mut(local_force);
                        total_pot += Ball2D::lenard_jones_potential(dist - radius_sum);
                    }
                    _ => {
                        min_dist = 0.;
                        //maybe use some max_force to repell
                        let lj_force = -100.;
                        let force_to_dist_val = lj_force / dist;
                        let local_force = Vec2D::new(
                            dist_vec.x * force_to_dist_val,
                            dist_vec.y * force_to_dist_val,
                        );

                        if self.verbose.console {
                            println!(
                                "\t\t ball {}--force(max): {:?} at dist: {}",
                                b.get_name(),
                                local_force,
                                dist
                            );
                        }

                        force.add_mut(local_force);
                        total_pot += Ball2D::lenard_jones_potential(dist - radius_sum);
                    }
                };
            }
        }
        (min_dist, force, total_pot)
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

    // pub fn clear_hood(&mut self) {
    //     self.hood.neighbours.borrow_mut().clear();
    // }

    pub fn update_old_pos(&mut self) {
        self.pre_update_pos = self.pos.clone();
    }
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
            accel.add_mut(accel_modification);

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

    // pub fn ball_color(&self) -> Color {
    //     let mut color_used = Color::RED;
    //     if self.braking {
    //         color_used = Color::YELLOW;
    //     }
    //     if self.verbose.any() {
    //         color_used = Color::BLUE;
    //     }
    //     color_used
    // }

    #[cfg(test)] //Only used in tests
    pub fn get_neighbour_names(&self) -> Vec<String> {
        self.hood.get_neighbour_names()
    }

    pub fn get_neighbour_force_and_pos(&self) -> Vec<(Vec2D, f32)> {
        self.hood
            .get_neighbour_force_and_pos(&self.pos, self.radius)
    }

    pub fn update_positions(&mut self, dt: f32, bound: &Rect) {
        if self.verbose.console {
            println!(
                "{} ------ [{:.2},{:.2}] neighbours: {:#?}",
                self.get_name(),
                self.hood.nearest,
                self.hood.farthest,
                self.hood.get_neighbour_names()
            );
        }

        // let (min_dist, total_force, total_pot) = self.force();
        // let speed_limit = f32::min(bound.w, bound.h) / 200.;
        //let accel = self.accel_with_speed_limit(&total_force, speed_limit, dt);

        // let a_norm = accel.norm();
        //let p = Vec2D::norm(&self.v)/a_norm;

        // let dx = self.v.x * dt + 0.5 * self.pre_update_accel.x * dt * dt;
        // let dy = self.v.y * dt + 0.5 * self.pre_update_accel.y * dt * dt;
        // let mut dt_max = dt;
        // if (min_dist >= 0. && (dx * dx + dy * dy) > (min_dist * min_dist * 0.5))
        //     || dx.abs() > bound.w * 0.5 || dy.abs() > bound.h * 0.5 {
        //     //dt_max = -p + f32::sqrt(p*p+min_dist/ a_norm);
        //     dt_max = 0.00001;
        // }
        //
        // if (MAX_UPDATE_RECURSION as f32) < dt / dt_max {
        //     dt_max = 2. * dt / (MAX_UPDATE_RECURSION as f32);
        // }
        //
        // let steps = f32::ceil(dt / dt_max);
        // let int_steps = steps as i32;
        // let dt_sub = dt / steps;

        // if int_steps > MAX_UPDATE_RECURSION {
        //     panic!("Too many substeps for update: {}", int_steps);
        // }
        let dt_sub = dt;
        let int_steps: usize = 1;
        self.execute_update_pos(dt_sub, bound);

        // for _ in 1..int_steps {
        //     let (_, total_force, _) = self.force();
        //     let accel = self.accel_with_speed_limit(&total_force, speed_limit, dt_sub);
        //     self.execute_update_pos(&accel, dt_sub, bound);
        // }

        if self.verbose.console {
            println!(
                "{} --- dt: {} substeps: {} new pos: {} new v: {}",
                self.get_name(),
                dt,
                int_steps,
                self.pos,
                self.v
            );
        }
        self.update_old_pos();
    }

    pub fn update_accel_and_speed(&mut self, dt: f32, bound: &Rect) -> (f32, f32) {
        let (min_dist, total_force, total_pot) = self.force();
        let speed_limit = f32::min(bound.w, bound.h) / 0.002;
        let accel = self.accel_with_speed_limit(&total_force, speed_limit, dt);
        self.v.x = self.v.x + 0.5 * (accel.x + self.accel.x) * dt;
        self.v.y = self.v.y + 0.5 * (accel.y + self.accel.y) * dt;

        self.accel = accel;

        if self.verbose.console {
            println!("updated to  v {:?}  a {:?}", self.v, self.accel);
        }
        (min_dist, total_pot)
    }
}

/*impl<'a> Clone for Verlet<'a> {
    fn clone(&self) -> Verlet<'a> {
        Verlet {
            nearest: self.nearest,
            farthest: self.farthest,
            neighbours: self.neighbours.clone(),
        }
    }
}*/

/*impl<'a> Clone for Ball2D<'a> {
    fn clone(&self) -> Ball2D<'a> {
       Ball2D {
           id: self.id,
           pos: self.pos.clone(),
           radius: self.radius,
           mass: self.mass,
           v: self.v.clone(),
           hood: self.hood.clone(),
           verbose: self.verbose.clone(),
       }
    }
}*/
