use crate::objects2d::point_2d::Vec2D;
use ggez::graphics::Rect;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

const MAX_UPDATE_RECURSION: i32 = 10;
//const LJ_SIG: f32 = 5.; // equilibrium distance 
//LJ_SIG in SI units is for Argon 3.4A=3.4E-10m
const LJ_SIG: f32 = 0.5; // equilibrium distance 
const LJ_EPS: f32 = 1.;// attractive strength
const SQRT6_OF_TWO: f32 = 1.122462048309372981; // == 2^(1/6)

#[derive(Debug)]
pub struct Ball2D<'a> {
    pub radius: f32,
    pub pos: Vec2D,
    id: usize,
    pub mass: f32,
    pub v: Vec2D,
    hood: Verlet<'a>,
    pub verbose: bool,
}

#[derive(Debug)]
struct Verlet<'a> {
    pub neighbours: RefCell<Vec<Weak<RefCell<Ball2D<'a>>>>>,
    pub nearest: f32,
    pub farthest: f32,
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

}

trait Named {
    fn get_name(&self) -> String;
}

impl<'a> Named for Ball2D<'a> {
    fn get_name(&self) -> String {
        format!("Ball{}",self.id)
    }
}

#[derive(PartialEq)]
#[derive(Debug)]
enum BallError {
    DistanceUnderflow,
    // LessThanTwoBalls,
}

type BallResult = Result<f32, BallError>;

impl<'a> Default for Ball2D<'a> {
    fn default() -> Ball2D<'a> {
        Ball2D {
            radius: 1.,
            pos: Vec2D::new(0.,0.),
            id: 0,
            mass: 1.,
            v: Vec2D::new(0.,0.),
            hood: Verlet { ..Default::default() },
            verbose: false,
        }
    }
}

impl<'a> PartialEq for Ball2D<'a> {
    fn eq(&self, other: &Self) -> bool{
        self.id == other.id
    }
}

impl<'a> Ball2D<'a> {

    pub fn _new_sized(id: usize, x: f32, y: f32, radius: f32) -> Ball2D<'a> {
        Ball2D::new_massive(id, x, y, radius, 1.)
    }

    pub fn new_massive(_id: usize, _x: f32, _y: f32, _radius: f32, _m: f32) -> Ball2D<'a>{
        Ball2D {
            id: _id, pos: Vec2D::new(_x,_y), radius: _radius, mass: _m, v: Vec2D::new(0.,0.),
            ..Default::default()
        }
    }



    pub fn has_larger_id(&self, other: &Self) -> bool{
        self.id > other.id
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn moment(&self) -> Vec2D{
        self.v.scaled_copy(self.mass)
    }


    pub fn update_hood(&mut self, dt: f32, self_rc: &Rc<RefCell<Self>>, others: Vec<Rc<RefCell<Ball2D<'a>>>>) {
       //use approximation for (v*dt+LJ_SIG)^2
       let v_sq = self.v.norm_sq();
       let eq= LJ_SIG + self.radius;
       //we only check hood every ten updates --> factors 100 and 10
       let dist_limit_by_v = 100.*v_sq * dt * dt + eq * eq + 2. * 10. * dt * eq * (self.v.x + self.v.y);
       let dist_limit_by_closest = if self.hood.nearest.is_finite() {
           let near_used = f32::max(self.hood.nearest, self.radius+LJ_SIG*2.);
           near_used * near_used *4. 
       } else { 
           //f32::MAX 
           (4.*LJ_SIG)*(4.*LJ_SIG)*4.
       };
       let dist_limit = f32::max(dist_limit_by_closest, dist_limit_by_v); 
       if self.verbose {
           println!("update_hood: {} dist_lim: {} #others: {}", self.get_name(), f32::sqrt(dist_limit), others.len());
       }
     

       for cur_rc in others.iter() {

         let cur_ref = cur_rc.borrow();
         let dist_sq= self.pos.dist(&cur_ref.pos).norm_sq();
         //println!("checking {}", cur_ref.get_name());
         if dist_sq < dist_limit {
             //cur.hood.neighbours.push(self.into());
             //let new_neighbour = Rc::new( RefCell::new(cur));
             self.hood.neighbours.borrow_mut().push(Rc::downgrade(cur_rc));
             cur_ref.hood.neighbours.borrow_mut().push(Rc::downgrade(self_rc));
             if self.verbose {
                 println!("\tneighbours {} at dist {}", cur_ref.get_name(), f32::sqrt(dist_sq));
             }
             //println!("updated {}-neighbours {:?}", self.get_name(), self.hood.neighbours
                //.borrow() 
                 //.iter()
                 //.filter_map(|weak| weak.upgrade())
                 //.map(|val| val.borrow().get_name())
                 //.collect::<Vec<String>>());
             //println!("updated {}-neighbours {:?}", cur.get_name(), cur.hood.neighbours);
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
           .fold((f32::INFINITY, f32::NEG_INFINITY),
            |(min, max), val| {
                ( min.min(val), max.max(val) )
            });
           


       self.hood.nearest = if nearest_sq.is_finite() { f32::sqrt(nearest_sq) } else  { f32::NAN };
       self.hood.farthest = if nearest_sq.is_finite() { f32::sqrt(farthest_sq) } else  { f32::NAN };

       if self.verbose {
           println!("update_closeness: {} nearest/farthest: {}/{}", self.get_name(), self.hood.nearest, self.hood.farthest);
       }
    }

    pub fn is_colliding (&self, other: &Ball2D) -> bool {
        //use norm_sq to avoid sqrt in norm
        let radius_sum_sq = (self.radius +  other.radius)*(self.radius + other.radius);
        let res = self.pos.dist(&other.pos).norm_sq()<radius_sum_sq;
        if res {
            println!("ball {} at {} is too close to {}: dist = {} sum_rad = {}", self.id, self.pos, other.pos,
                     self.pos.dist(&other.pos).norm(),
                     self.radius + other.radius)
        }
        res
    }

    pub fn equilibrium_radius(&self) -> f32 {
        //case: r1==r2 --- SQRT6_OF_TWO * LJ_SIG + 2* rad == dist_equi
        (SQRT6_OF_TWO * LJ_SIG)*0.5 + self.radius
    }
    
    fn lenard_jones_potential(dist_of_balls: f32, radius_sum:f32) -> f32 {

        //case dist_of_balls = equilibrium_radius*2 and radius_sum=2*radius
        //dist = SQRT6_OF_TWO LJ_SIG + 2*rad - 2*rad
        //dist = SQRT6_OF_TWO LJ_SIG
        //quot = LJ_SIG/dist = 1/SQRT6_OF_TWO
        //quot_six = 1/2
        //potential: 4 LJ_EPS 1/2 *(1/2-1)
        //    4 LJ_EPS 1/2 * (-1/2)
        //    - LJ_EPS


        let dist = dist_of_balls-radius_sum;
        let quot = LJ_SIG/dist;
        let quot_six = quot.powi(6);
        4.* LJ_EPS* quot_six * (quot_six - 1.)
    }

    fn lenard_jones_force(dist_of_balls: f32, radius_sum:f32, _id: usize)-> f32 {

        let dist = dist_of_balls - radius_sum;
        let quot = LJ_SIG/dist;
        let quot_six = quot.powi(6);
        let res = 24. * LJ_EPS*  quot_six * ( 2. * quot_six - 1.)/dist;
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

    fn check_min_dist(actual_dist: f32, min_dist: f32) -> BallResult {
        // let wiggle_room = LJ_SIG*0.1;
        // if actual_dist < 0. {
        //     return Err(BallError::LessThanTwoBalls);
        // }
        if actual_dist < min_dist || f32::is_nan(min_dist){
            if actual_dist <= 0. {
                return Err(BallError::DistanceUnderflow);
            }
            return Ok(actual_dist);
        }
        Ok(min_dist)
    }

    //fn force_by(&self, others: &Vec<Ball2D>)  -> (f32, Vec2D, f32) {
    fn force(&self) -> (f32, Vec2D, f32) {
        let mut force = Vec2D::new(0.,0.);
        let mut min_dist = f32::NAN;
        let mut total_pot = 0.;
        //for b in others {
        let the_neighbours = self.hood.neighbours.borrow();
        for ref_b in the_neighbours.iter(){
            if let Some(b_rc) = ref_b.upgrade() {

                //from other_ball to this_ball
                let b = b_rc.borrow();
                let dist_vec = self.pos.dist(&b.pos);
                let dist = dist_vec.norm();
                let radius_sum = b.radius + self.radius;
                /*min_dist = match Ball2D::check_min_dist(dist - radius_sum*0.9, min_dist) {
                    Err(BallError::DistanceUnderflow) | Err(BallError::LessThanTwoBalls) => return(0., Vec2D::new(0.,0.), 0.),
                    Ok(dist_result) => dist_result,
                };*/

                match  Ball2D::check_min_dist(dist - radius_sum, min_dist) {
                    Ok(min_dist_cur) => {
                        min_dist = min_dist_cur;
                        let lj_force = Ball2D::lenard_jones_force(dist, radius_sum, self.id);
                        let force_to_dist_val = lj_force/dist;
                        let local_force = Vec2D::new(dist_vec.x*force_to_dist_val, dist_vec.y*force_to_dist_val);

                        if self.verbose {
                            println!("\t\t ball {}--force: {:?} at dist: {}", b.get_name(), local_force, dist);
                        }

                        force.add(local_force);
                        total_pot += Ball2D::lenard_jones_potential(dist, radius_sum);
                    },
                    _ => {
                        min_dist = 0.;
                        //maybe use some max_force to repell 
                        let lj_force = 100.;
                        let force_to_dist_val = lj_force/dist;
                        let local_force = Vec2D::new(dist_vec.x*force_to_dist_val, dist_vec.y*force_to_dist_val);

                        if self.verbose {
                            println!("\t\t ball {}--force(max): {:?} at dist: {}", b.get_name(), local_force, dist);
                        }

                        force.add(local_force);
                        total_pot += Ball2D::lenard_jones_potential(dist, radius_sum);
                    },
                };

            }
        }
        (min_dist, force, total_pot)
    }

    fn execute_update(&mut self, accel: &Vec2D, dt: f32, bound: &Rect) {
        let dx = self.v.x * dt + 0.5*accel.x * dt * dt;
        let dy = self.v.y * dt + 0.5*accel.y * dt * dt;
        self.v.x += accel.x * dt;
        self.v.y += accel.y * dt;

        let newx = self.pos.x + dx;
        let bound_min_x = bound.x + self.radius;
        let bound_max_x = bound.x + bound.w - self.radius;


        if bound_max_x> newx && newx > bound_min_x {
            self.pos.x += dx;
        } else if newx<=bound_min_x {
            let overx = bound_min_x - newx;
            self.pos.x = bound_min_x + overx;
            self.v.x = -self.v.x;
        } else  {
            // x is outside right boundary
            let overx = newx-bound_max_x;
            self.pos.x = bound_max_x - overx;
            self.v.x = -self.v.x;
        }

        let newy = self.pos.y + dy;
        let bound_min_y = bound.y + self.radius;
        let bound_max_y = bound.y + bound.h - self.radius;
        if bound_max_y> newy && newy > bound_min_y {
            self.pos.y += dy;
        } else if newy<=bound_min_y {
            let overy = bound_min_y - newy;
            self.pos.y = bound_min_y + overy;
            self.v.y = -self.v.y;
        } else  {
            // y is outside right boundary
            let overy = newy-bound_max_y;
            self.pos.y = bound_max_y - overy;
            self.v.y = -self.v.y;
        }
    }

    pub fn clear_hood(&mut self){
        self.hood.neighbours.borrow_mut().clear();
    }

    fn accel_with_speed_limit(&self, total_force: &Vec2D, speed_limit: f32 ) -> (f32, Vec2D) {
        let speed_sq = self.v.norm_sq();
        let _friction_number = speed_sq/speed_limit/speed_limit;
        let friction_braking = 2. * _friction_number * _friction_number;
        let mut accel: Vec2D = total_force.scaled_copy(1./self.mass);
        accel.add(Vec2D::new(-friction_braking*self.v.x, -friction_braking*self.v.y));
        // println!("... F_t: {} and limit: {} for speed {}", total_force, speed_limit, self.v);
        // println!("...... friction_braking:{} and mass: {} accel_res: {:?}", friction_braking, self.mass, accel);
        (friction_braking, accel)
    }

    #[cfg(test)] //Only used in tests
    pub fn get_neighbour_names(&self) -> Vec<String>{
        self.hood.get_neighbour_names()
    }

    pub fn update(&mut self, dt: f32, bound: &Rect) -> (f32, f32) {
        if self.verbose {
            println!("{} ------ [{:.2},{:.2}] neighbours: {:#?}", self.get_name(), self.hood.nearest, self.hood.farthest, self.hood.get_neighbour_names());
        }

        let (min_dist, total_force, total_pot) = self.force();
        let speed_limit = f32::min(bound.w, bound.h)/1E-1;
        let (fb, accel) = self.accel_with_speed_limit(&total_force, speed_limit);

        let a_norm = accel.norm();
        //let p = Vec2D::norm(&self.v)/a_norm;
        
        if fb>a_norm {
            println!("{} --- has active friction brake: {} at accel {} and force {}", self.get_name(),  fb, a_norm, total_force);
        }

        let dx = self.v.x * dt + 0.5*accel.x * dt * dt ;
        let dy = self.v.y * dt + 0.5*accel.y * dt * dt;
        let mut dt_max = dt;
        if min_dist >= 0. && (dx*dx+dy*dy) > (min_dist*min_dist*0.5) {
            //dt_max = -p + f32::sqrt(p*p+min_dist/ a_norm);
            dt_max = 0.00001;
        }

        if (MAX_UPDATE_RECURSION as f32) < dt/dt_max {
            dt_max = 2. * dt/(MAX_UPDATE_RECURSION as f32);
        }

        let steps = f32::ceil(dt/dt_max);
        let int_steps = steps as i32;
        let dt_sub = dt/steps;


        if int_steps > MAX_UPDATE_RECURSION {
            panic!("Too many substeps for update: {}", int_steps);
        }
        self.execute_update(&accel, dt_sub, bound);

        for _ in 1..int_steps {
            let (_, total_force, _) = self.force();
            let (_, accel) = self.accel_with_speed_limit(&total_force, speed_limit);
            self.execute_update(&accel, dt_sub, bound);
        }

        if self.verbose {
            println!("{} --- dt: {} substeps: {} new pos: {} new v: {}", self.get_name(), dt, int_steps, self.pos, self.v);
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


#[cfg(test)]
mod tests {
    use float_cmp::{approx_eq, assert_approx_eq};

    use super::*;
    
    #[test]
    fn test_name() {
        let b = Ball2D::new_massive(0, 0., 0., 3., 1.);
        assert_eq!(b.get_name(), "Ball0");

    }

    
    #[test]
    fn test_verlet_default(){
        let vlist: Verlet = Verlet::default();
        assert!(vlist.nearest.is_nan());
        assert!(vlist.farthest.is_nan());

        let actual_list = vlist.neighbours.borrow();
        assert_eq!(actual_list.len(), 0);
    }

    #[test]
    fn test_verlet_names(){
        let vlist: Verlet = Verlet::default();

        let nu_ball1= Rc::new( RefCell::new( Ball2D::new_massive(1, 2., 3., 4., 5.)));
        let nu_ball2= Rc::new( RefCell::new( Ball2D::new_massive(2, 3., 4., 5., 6.)));
        { //mutable section of lst
            let mut lst_to_add = vlist.neighbours.borrow_mut();
            lst_to_add.push(Rc::downgrade(&nu_ball1));
            lst_to_add.push(Rc::downgrade(&nu_ball2));
        }



        let names = vlist.get_neighbour_names();

        assert_eq!(names[0], nu_ball1.borrow().get_name());
        assert_eq!(names[1], nu_ball2.borrow().get_name());

    }

    #[test]
    fn test_clear_hood(){
        let vlist: Verlet = Verlet::default();
        let mut ball1=  Ball2D::new_massive(3, 20., 30., 4., 5.);


        let nu_ball1= Rc::new( RefCell::new( Ball2D::new_massive(1, 2., 3., 4., 5.)));
        let nu_ball2= Rc::new( RefCell::new( Ball2D::new_massive(2, 3., 4., 5., 6.)));
        { //mutable section of lst
            let mut lst_to_add = vlist.neighbours.borrow_mut();
            lst_to_add.push(Rc::downgrade(&nu_ball1));
            lst_to_add.push(Rc::downgrade(&nu_ball2));
        }
        ball1.hood = vlist;




        let names = ball1.get_neighbour_names();

        assert_eq!(names[0], nu_ball1.borrow().get_name());
        assert_eq!(names[1], nu_ball2.borrow().get_name());

        ball1.clear_hood();

        let names = ball1.get_neighbour_names();

        assert!(names.is_empty());
    }

    #[test]
    fn test_ball_name(){
        let ball17 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let nam = ball17.get_name();

        assert_eq!(nam, "Ball17");
    }

    #[test]
    fn test_partial_eq_ball2d(){

        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let ball2 = Ball2D::new_massive(17, 4., 5., 6., 7.,);
        let ball3 = Ball2D::new_massive(16, 2., 3., 4., 5.,);


        assert_eq!(ball1, ball2);
        assert_ne!(ball1, ball3);


    }

    #[test]
    fn test_get_id(){
        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let ball2 = Ball2D::new_massive(1, 4., 5., 6., 7.,);

        assert_eq!(ball1.get_id(), 17);
        assert_eq!(ball2.get_id(), 1);
    }

    #[test]
    fn test_has_larger_id(){
        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let ball2 = Ball2D::new_massive(15, 4., 5., 6., 7.,);

        assert!(!ball1.has_larger_id(&ball1));
        assert!(ball1.has_larger_id(&ball2));
        assert!(!ball2.has_larger_id(&ball1));
    }

    #[test]
    fn test_moment() {
        let mut ball1 = Ball2D::new_massive(17, 2., 3., 4., 5., );

        assert_eq!(ball1.moment(), Vec2D::new(0., 0.));
        ball1.v.x = 3.;
        assert_eq!(ball1.moment(), Vec2D::new(15., 0.));
    }

    #[test]
    fn test_is_colliding(){
        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let ball2 = Ball2D::new_massive(15, 11.99999, 3., 6., 7.,);

        assert!(ball1.is_colliding(&ball2), "ball1 {:?} ball2 {:?} dist: {:?}", 
            ball1.pos, 
            ball2.pos, 
            Vec2D::dist(&ball1.pos, &ball2.pos),
            );

        let ball3 =  Ball2D::new_massive(13, 12., 5., 6., 7.,);

        assert!(!ball1.is_colliding(&ball3), "ball1 {:?} ball2 {:?} dist: {:?}", 
            ball1.pos, 
            ball3.pos, 
            Vec2D::dist(&ball1.pos, &ball3.pos),
            );
    }

    #[test]
    fn test_lenard_jones_potential(){
        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let distance = ball1.equilibrium_radius()*2.;
        println!("ball: {:?}", ball1);
        println!("eq_distance: {}, effective distance {}", distance, distance - 2.*ball1.radius);
        let result_pot: f32 = Ball2D::lenard_jones_potential(distance, 2.*ball1.radius);

        assert!(approx_eq!(f32, 
                result_pot,
                -LJ_EPS, 
                ulps = 2), 
            "potential {} is not close to LJ_EPS ({})", result_pot, -LJ_EPS);
    }

    #[test]
    fn test_lenard_jones_no_force_at_equilibrium(){

        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let distance = ball1.equilibrium_radius()*2.;
        let result_force: f32 = Ball2D::lenard_jones_force(
            distance,
            ball1.radius*2., 
            0);

        assert!(result_force.abs()<1e-4,
            "force {} is not close to 0", result_force);
    }

    #[test]
    fn test_lenard_jones_positive_force_if_near(){

        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let distance = ball1.equilibrium_radius()*2.*0.9;
        let result_force: f32 = Ball2D::lenard_jones_force(
            distance,
            ball1.radius*2.,
            0);

        assert!(result_force<0.,
                "force {} must be < 0 if closer than equilibrium", result_force);
    }

    #[test]
    fn test_lenard_jones_negative_force_if_far(){

        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let distance = ball1.equilibrium_radius()*3.;
        let result_force: f32 = Ball2D::lenard_jones_force(
            distance,
            ball1.radius*2.,
            0);

        assert!(result_force<0.,
                "force {} must be < 0 if closer than equilibrium", result_force);
    }

    #[test]
    fn test_check_min_dist() {
        // Case 1: actual_dist is negative → Expect Err(BallError::LessThanTwoBalls)
        // assert_eq!(
        //     Ball2D::check_min_dist(-1.0, 2.0),
        //     Err(BallError::LessThanTwoBalls),
        //     "Expected LessThanTwoBalls error for negative distance"
        // );

        // Case 2: actual_dist is exactly 0 → Expect Err(BallError::DistanceUnderflow)
        assert_eq!(
            Ball2D::check_min_dist(0.0, 2.0),
            Err(BallError::DistanceUnderflow),
            "Expected DistanceUnderflow error for distance == 0"
        );

        // Case 3: actual_dist is greater than 0 but less than min_dist → Expect Ok(actual_dist)
        assert_eq!(
            Ball2D::check_min_dist(1.5, 2.0),
            Ok(1.5),
            "Expected Ok(actual_dist) when actual_dist < min_dist"
        );

        // Case 4: actual_dist is greater than or equal to min_dist → Expect Ok(min_dist)
        assert_eq!(
            Ball2D::check_min_dist(2.5, 2.0),
            Ok(2.0),
            "Expected Ok(min_dist) when actual_dist > min_dist"
        );

        assert_eq!(
            Ball2D::check_min_dist(2.0, 2.0),
            Ok(2.0),
            "Expected Ok(min_dist) when actual_dist == min_dist"
        );
    }

    #[test]
    fn test_accel_with_no_velocity() {
        let mass = 1.0;
        let mut ball = Ball2D::new_massive(17, 2., 3., 4., mass);
        ball.v = Vec2D::new(0., 0.);
        let force = Vec2D::new(10.0, 0.0);

        let (friction, accel) = ball.accel_with_speed_limit(&force, 5.0);

        assert_eq!(friction, 0.0, "Expected zero friction for zero velocity, got {}", friction);
        assert_eq!(accel, Vec2D::new(10.0, 0.0), "Expected acceleration to be force/mass, got {:?}", accel);
    }

    #[test]
    fn test_accel_with_velocity_below_speed_limit() {
        let mass = 1.0;
        let mut ball = Ball2D::new_massive(17, 2., 3., 4., mass);
        ball.v = Vec2D::new(2., 0.);
        let force = Vec2D::new(10.0, 0.0);

        let (friction, accel) = ball.accel_with_speed_limit(&force, 5.0);

        assert!(friction > 0.0, "Expected positive friction for moving object, got {}", friction);
        assert!(accel.x < 10.0, "Expected acceleration to be slightly reduced by friction, got {:?}", accel);
    }

    #[test]
    fn test_accel_with_velocity_at_speed_limit() {
        let mass = 1.0;
        let mut ball = Ball2D::new_massive(17, 2., 3., 4., mass);
        ball.v = Vec2D::new(5., 0.);
        let force = Vec2D::new(10.0, 0.0);

        let (friction, accel) = ball.accel_with_speed_limit(&force, 5.0);

        assert!(friction > 0.1, "Expected noticeable friction at speed limit, got {}", friction);
        assert!(accel.x < 10.0, "Expected acceleration to be reduced by friction, got {:?}", accel);
    }

    #[test]
    fn test_accel_with_velocity_above_speed_limit() {
        let mass = 1.0;
        let mut ball = Ball2D::new_massive(17, 2., 3., 4., mass);
        ball.v = Vec2D::new(10., 0.);
        let force = Vec2D::new(10.0, 0.0);

        let (friction, accel) = ball.accel_with_speed_limit(&force, 5.0);

        assert!(friction > 0.5, "Expected strong friction above speed limit, got {}", friction);
        assert!(accel.x < 10.0, "Expected acceleration to be significantly reduced, got {:?}", accel);
    }

    #[test]
    fn test_accel_with_no_force_applied() {
        let mass = 1.0;
        let mut ball = Ball2D::new_massive(17, 2., 3., 4., mass);
        ball.v = Vec2D::new(3., 0.);
        let force = Vec2D::new(0.0, 0.0);

        let (_friction, accel) = ball.accel_with_speed_limit(&force, 5.0);

        assert!(accel.x < 0.0, "Expected negative acceleration causing deceleration, got {:?}", accel);
    }

    #[test]
    fn test_accel_with_different_mass() {
        let mass = 1.0;
        let mut ball = Ball2D::new_massive(17, 2., 3., 4., 2. * mass);
        ball.v = Vec2D::new(4., 0.);
        let force = Vec2D::new(10.0, 0.0);

        let (friction, accel) = ball.accel_with_speed_limit(&force, 5.0);

        let expected_accel_x = (10.0 / 2.0) + (-friction * 4.0);
        assert_eq!(accel.x, expected_accel_x, "Acceleration should factor in mass correctly. Expected: {}, got: {}", expected_accel_x, accel.x);
    }

    #[test]
    fn test_force_no_neighbors() {
        let ball = Ball2D::new_massive(17, 2., 3., 4., 1.0,);

        let (min_dist, force, total_pot) = ball.force();

        assert!(min_dist.is_nan(), "Expected min_dist to be NaN for no neighbors");
        assert_eq!(force, Vec2D::new(0.0, 0.0), "Expected zero force for no neighbors");
        assert_eq!(total_pot, 0.0, "Expected zero potential energy for no neighbors");
    }

    #[test]
    fn test_force_one_neighbor_at_equilibrium() {

        let ball = Ball2D::new_massive(17, 2., 3., 4., 1.,);
        let neighbour = Rc::new(RefCell::new(Ball2D::new_massive(18, 2. + ball.equilibrium_radius()*2., 3., 4., 1.,)));


        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(&neighbour));

        let (min_dist, force, total_pot) = ball.force();

        assert!(min_dist > 0.0, "min_dist should be positive");
        assert!(force.norm() < 1e-4, "Force should be close to zero at equilibrium but was {:?}", force);
        assert!(total_pot < 0.0, "Potential energy should be negative (binding energy) but was {}", total_pot);
    }

    #[test]
    fn test_force_one_neighbor_too_close() {

        let ball = Ball2D::new_massive(17, 2., 3., 4., 1.,);
        let dist_for_repulsion = ball.radius+ ball.equilibrium_radius();
        let neigh_x = ball.pos.x + dist_for_repulsion;
        let neigh_y = ball.pos.y;
        let neighbour = Rc::new(RefCell::new(Ball2D::new_massive(18, neigh_x, neigh_y, 4., 1.,)));


        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(&neighbour));

        let (min_dist, force, _total_pot) = ball.force();

        assert!((min_dist- dist_for_repulsion +ball.radius*2.).abs()< 1e-7, "min_dist {} should be dist_for_repulsion {} (diff: {})",
                min_dist,
                dist_for_repulsion -ball.radius*2.,
                min_dist- dist_for_repulsion +ball.radius*2.,
        );
        println!("the force vector is {:?} and the dist vector is {:?}", force, neighbour.borrow().pos.dist(&Vec2D::new(neigh_x, neigh_y)));
        assert!(force.norm() > 50.0, "Force should be strongly repulsive");
    }

    #[test]
    fn test_force_one_neighbor_far_away() {

        let ball = Ball2D::new_massive(17, 2., 3., 4., 1.,);
        let dist_for_attraction = 3.*ball.equilibrium_radius();
        let neigh_x = ball.pos.x + dist_for_attraction;
        let neigh_y = ball.pos.y;
        let neighbour = Rc::new(RefCell::new(Ball2D::new_massive(18, neigh_x, neigh_y, ball.radius, 1.,)));


        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(&neighbour));

        let (min_dist, force, _total_pot) = ball.force();

        assert!((min_dist- dist_for_attraction +ball.radius*2.).abs()< 1e-7, "min_dist {} should be dist_for_attraction {} (diff: {})",
                min_dist,
                dist_for_attraction -ball.radius*2.,
                min_dist- dist_for_attraction +ball.radius*2.,
        );

        println!("the force vector is {:?} and the dist vector is {:?}", force, neighbour.borrow().pos.dist(&Vec2D::new(neigh_x, neigh_y)));
        assert_eq!(force.y, 0.);
        assert!(force.x < 0.0, "Force should be slightly attractive");
    }

    #[test]
    fn test_force_multiple_neighbors() {
        let ball = Ball2D::new_massive(17, 2., 3., 4., 1.,);
        let dist_for_repulsion = 3.*ball.equilibrium_radius();
        let neighbour1 = Rc::new(RefCell::new(Ball2D::new_massive(18, 2. + dist_for_repulsion, 3., 4., 1.,)));
        let neighbour2 = Rc::new(RefCell::new(Ball2D::new_massive(19, 2. , 3.+ dist_for_repulsion, 4., 1.,)));

        let lj_force = Ball2D::lenard_jones_force(dist_for_repulsion, ball.radius*2., 17);


        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(&neighbour1));
        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(&neighbour2));

        let (min_dist, force, total_pot) = ball.force();

        assert!(min_dist > 0.0, "min_dist should be positive");
        assert!(force.norm() > 0.0, "Total force should be non-zero");
        assert!(total_pot < 0.0, "Total potential should be negative (binding energy)");

        assert_approx_eq!(f32, force.x, lj_force, epsilon=1e-6);
        assert_approx_eq!(f32, force.y, lj_force, epsilon=1e-6);
    }

    #[test]
    fn test_execute_update_no_collision() {
        // let mut ball = Ball2D {
        //     pos: Vec2D::new(5.0, 5.0),
        //     v: Vec2D::new(1.0, 1.0),
        //     radius: 1.0,
        // };
        let mut ball = Ball2D::new_massive(17, 5., 5., 1., 1.,);
        ball.v = Vec2D::new(1.0, 1.0);
        let accel = Vec2D::new(0.0, 0.0);
        let dt = 1.0;
        let bound = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };

        ball.execute_update(&accel, dt, &bound);

        assert_approx_eq!(f32, ball.pos.x, 6.0, ulps = 2);
        assert_approx_eq!(f32, ball.pos.y, 6.0, ulps = 2);
        assert_eq!(ball.v, Vec2D::new(1.0, 1.0)); // No change in velocity
    }

    #[test]
    fn test_execute_update_left_wall_collision() {
        // let mut ball = 2Ball2D {
        //     pos: Vec2D::new(1.5, 5.0),
        //     v: Vec2D::new(-2.0, 0.0),
        //     radius: 1.0,
        // };
        let mut ball = Ball2D::new_massive(17, 1.5, 5., 1., 1.,);
        ball.v = Vec2D::new(-2.0, 0.0);
        let accel = Vec2D::new(0.0, 0.0);
        let dt = 1.0;
        let bound = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };

        ball.execute_update(&accel, dt, &bound);

        assert!(ball.pos.x > 1.0, "Ball should bounce off left wall.");
        assert_eq!(ball.v.x, 2.0, "Velocity should be reversed after hitting left wall.");
    }

    #[test]
    fn test_execute_update_right_wall_collision() {
        // let mut ball = Ball2D {
        //     pos: Vec2D::new(8.5, 5.0),
        //     v: Vec2D::new(2.0, 0.0),
        //     radius: 1.0,
        // };
        let mut ball = Ball2D::new_massive(17, 8.5, 5., 1., 1.,);
        ball.v = Vec2D::new(2.0, 0.0);
        let accel = Vec2D::new(0.0, 0.0);
        let dt = 1.0;
        let bound = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };

        ball.execute_update(&accel, dt, &bound);

        assert!(ball.pos.x < 9.0, "Ball should bounce off right wall.");
        assert_eq!(ball.v.x, -2.0, "Velocity should be reversed after hitting right wall.");
    }

    #[test]
    fn test_execute_update_top_wall_collision() {
        // let mut ball = Ball2D {
        //     pos: Vec2D::new(5.0, 8.5),
        //     v: Vec2D::new(0.0, 2.0),
        //     radius: 1.0,
        // };
        let mut ball = Ball2D::new_massive(17, 5., 8.5, 1., 1.,);
        ball.v = Vec2D::new(0., 2.0);
        let accel = Vec2D::new(0.0, 0.0);
        let dt = 1.0;
        let bound = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };

        ball.execute_update(&accel, dt, &bound);

        assert!(ball.pos.y < 9.0, "Ball should bounce off top wall.");
        assert_eq!(ball.v.y, -2.0, "Velocity should be reversed after hitting top wall.");
    }

    #[test]
    fn test_execute_update_bottom_wall_collision() {
        // let mut ball = Ball2D {
        //     pos: Vec2D::new(5.0, 1.5),
        //     v: Vec2D::new(0.0, -2.0),
        //     radius: 1.0,
        // };
        let mut ball = Ball2D::new_massive(17, 5., 1.5, 1., 1.,);
        ball.v = Vec2D::new(0., -2.0);
        let accel = Vec2D::new(0.0, 0.0);
        let dt = 1.0;
        let bound = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };

        ball.execute_update(&accel, dt, &bound);

        assert!(ball.pos.y > 1.0, "Ball should bounce off bottom wall.");
        assert_eq!(ball.v.y, 2.0, "Velocity should be reversed after hitting bottom wall.");
    }

    #[test]
    fn test_execute_update_corner_collision() {
        // let mut ball = Ball2D {
        //     pos: Vec2D::new(1.5, 1.5),
        //     v: Vec2D::new(-2.0, -2.0),
        //     radius: 1.0,
        // };
        let mut ball = Ball2D::new_massive(17, 1.5, 1.5, 1., 1.,);
        ball.v = Vec2D::new(-2.0, -2.0);
        let accel = Vec2D::new(0.0, 0.0);
        let dt = 1.0;
        let bound = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };

        ball.execute_update(&accel, dt, &bound);

        assert!(ball.pos.x > 1.0, "Ball should bounce off left wall.");
        assert!(ball.pos.y > 1.0, "Ball should bounce off bottom wall.");
        assert_eq!(ball.v.x, 2.0, "Velocity X should be reversed.");
        assert_eq!(ball.v.y, 2.0, "Velocity Y should be reversed.");
    }

    #[test]
    fn test_execute_update_with_acceleration() {
        // let mut ball = Ball2D {
        //     pos: Vec2D::new(5.0, 5.0),   // Initial position
        //     v: Vec2D::new(1.0, 1.0),     // Initial velocity
        //     radius: 1.0,
        // };
        let mut ball = Ball2D::new_massive(17, 5., 5., 1., 1.,);
        ball.v = Vec2D::new(1.0, 1.0);
        let accel = Vec2D::new(2.0, -1.0); // Acceleration in x and y directions
        let dt = 1.0; // Time step
        let bound = Rect { x: 0.0, y: 0.0, w: 20.0, h: 20.0 }; // Sufficiently large boundary

        ball.execute_update(&accel, dt, &bound);

        // Expected position and velocity using kinematics
        let expected_pos_x = 5.0 + 1.0 * dt + 0.5 * 2.0 * dt * dt; // 5 + 1 + 1 = 7.0
        let expected_pos_y = 5.0 + 1.0 * dt + 0.5 * (-1.0) * dt * dt; // 5 + 1 - 0.5 = 5.5

        let expected_vel_x = 1.0 + 2.0 * dt; // 1 + 2 = 3.0
        let expected_vel_y = 1.0 + (-1.0) * dt; // 1 - 1 = 0.0

        assert_approx_eq!(f32, ball.pos.x, expected_pos_x, ulps = 2);
        assert_approx_eq!(f32, ball.pos.y, expected_pos_y, ulps = 2);
        assert_approx_eq!(f32, ball.v.x, expected_vel_x, ulps = 2);
        assert_approx_eq!(f32, ball.v.y, expected_vel_y, ulps = 2);
    }

    #[test]
    fn test_update_no_movement() {
        let mut ball = Ball2D::new_massive(17, 2.0, 3.0, 1.0, 1.0);
        let bound = Rect::new(0.0, 0.0, 100.0, 100.0);

        ball.verbose = true;
        let (min_dist, total_pot) = ball.update(1.0, &bound);

        assert_eq!(ball.v, Vec2D::new(0.0, 0.0), "Velocity should remain zero when no force is applied");
        assert_eq!(ball.pos, Vec2D::new(2.0, 3.0), "Position should remain the same when stationary");
        assert!(min_dist.is_nan(), "Min distance should NaN since no neighbours");
        assert!(total_pot >= 0.0, "Total potential energy should be non-negative");
    }

    #[test]
    fn test_update_with_force() {
        let mut ball = Ball2D::new_massive(17, 2.0, 3.0, 1.0, 1.0);
        ball.v = Vec2D::new(1.0, 1.0); // Initial velocity
        let bound = Rect::new(0.0, 0.0, 100.0, 100.0);

        let (min_dist, total_pot) = ball.update(1.0, &bound);

        assert!(ball.pos.x > 2.0, "Ball should move in the x direction");
        assert!(ball.pos.y > 3.0, "Ball should move in the y direction");
        assert!(min_dist.is_nan(), "Min distance should NaN since not neighbours");
        assert!(total_pot >= 0.0, "Total potential energy should be valid");
    }

    #[test]
    fn test_update_with_boundary_collision() {
        let mut ball = Ball2D::new_massive(17, 95.0, 95.0, 4.0, 1.0);
        ball.v = Vec2D::new(10.0, 10.0);
        let bound = Rect::new(0.0, 0.0, 100.0, 100.0);

        let (_, _) = ball.update(1.0, &bound);

        assert!(ball.pos.x <= 100.0, "Ball should not go out of bounds in x direction");
        assert!(ball.pos.y <= 100.0, "Ball should not go out of bounds in y direction");
        assert!(ball.v.x < 0.0 || ball.v.y < 0.0, "Velocity should reflect if boundary collision occurred");
    }

    #[test]
    fn test_update_with_high_dt_causing_recursion() {
        let mut ball = Ball2D::new_massive(17, 10.0, 10.0, 1.0, 1.0);
        ball.v = Vec2D::new(50.0, 50.0); // High velocity to trigger dt subdivision
        let bound = Rect::new(0.0, 0.0, 100.0, 100.0);

        let (min_dist, total_pot) = ball.update(10.0, &bound);

        assert!(min_dist.is_nan(), "Min distance should NaN since not neighbours");
        assert!(total_pot >= 0.0, "Total potential energy should be non-negative");
        assert!(ball.pos.x <= 100.0, "Ball should stay within bounds");
        assert!(ball.pos.y <= 100.0, "Ball should stay within bounds");
    }

    #[test]
    #[ignore]
    #[should_panic(expected = "Too many substeps for update")]
    fn test_update_panics_on_excessive_substeps() {
        let mut ball = Ball2D::new_massive(17, 10.0, 10.0, 1.0, 1.0);
        ball.v = Vec2D::new(1000.0, 1000.0); // Extreme velocity to trigger panic
        let bound = Rect::new(0.0, 0.0, 100.0, 100.0);

        ball.update(100.0, &bound); // Large dt should cause too many subdivisions and panic
    }

    #[test]
    fn test_update_closeness_stats_no_neighbors() {
        let mut ball = Ball2D::new_massive(17, 10.0, 10.0, 4.0, 1.0);
        ball.update_closeness_stats();

        assert!(ball.hood.nearest.is_nan(), "Nearest distance should become NaN if no neighbors exist");
        assert!(ball.hood.farthest.is_nan(), "Farthest distance should become NaN if no neighbors exist");
    }

    #[test]
    fn test_update_closeness_stats_one_neighbor() {
        let mut ball = Ball2D::new_massive(17, 10.0, 10.0, 4.0, 1.0);
        let neighbor = Ball2D::new_massive(18, 15.0, 10.0, 4.0, 1.0);
        ball.verbose=true;
        //ensure neighbour is held until after update_closeness
        let neighbor_rc = &Rc::new(RefCell::new(neighbor));
        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(neighbor_rc));

        ball.update_closeness_stats();

        let expected_distance = 5.0;
        assert_eq!(ball.hood.nearest, expected_distance, "Nearest distance should be correct for one neighbor");
        assert_eq!(ball.hood.farthest, expected_distance, "Farthest distance should be correct for one neighbor");
    }

    #[test]
    fn test_update_closeness_stats_multiple_neighbors() {
        let mut ball = Ball2D::new_massive(17, 10.0, 10.0, 4.0, 1.0);
        let neighbor1 = Ball2D::new_massive(18, 12.0, 10.0, 4.0, 1.0);
        let neighbor2 = Ball2D::new_massive(19, 20.0, 10.0, 4.0, 1.0);
        ball.verbose = true;

        //ensure neighbour is held until after update_closeness
        let neighbor1_rc = &Rc::new(RefCell::new(neighbor1));
        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(neighbor1_rc));
        let neighbor2_rc = &Rc::new(RefCell::new(neighbor2));
        ball.hood.neighbours.borrow_mut().push(Rc::downgrade(neighbor2_rc));

        ball.update_closeness_stats();

        let expected_nearest = 2.0;
        let expected_farthest = 10.0;
        assert_eq!(ball.hood.nearest, expected_nearest, "Nearest distance should be correct with multiple neighbors");
        assert_eq!(ball.hood.farthest, expected_farthest, "Farthest distance should be correct with multiple neighbors");
    }

    #[test]
    fn test_update_closeness_stats_ignores_dropped_neighbors() {
        let mut ball = Ball2D::new_massive(17, 10.0, 10.0, 4.0, 1.0);
        let neighbor = Ball2D::new_massive(18, 15.0, 10.0, 4.0, 1.0);

        ball.verbose = true;
        let neighbour_rc = Rc::new(RefCell::new(neighbor));
        let weak_ref = Rc::downgrade(&neighbour_rc);
        ball.hood.neighbours.borrow_mut().push(weak_ref);

        // Simulate neighbor being dropped
        drop(neighbour_rc);

        ball.update_closeness_stats();

        assert!(ball.hood.nearest.is_nan(), "Nearest distance should become NaN when all neighbors are dropped");
        assert!(ball.hood.farthest.is_nan(), "Farthest distance should become NaN when all neighbors are dropped");
    }
    #[test]
    fn test_update_hood_adds_neighbors_within_distance_limit() {
        let ball1 = Ball2D::new_massive(1, 0.0, 0.0, 2.0, 1.0);
        let ball2 = Ball2D::new_massive(2, 3.0, 4.0, 2.0, 1.0);

        let ball1_rc = Rc::new(RefCell::new(ball1));
        let ball2_rc = Rc::new(RefCell::new(ball2));

        let dt = 1.0;
        let others = vec![ball2_rc.clone()];

        ball1_rc.borrow_mut().update_hood(dt, &ball1_rc, others);

        let ball1_borrowed = ball1_rc.borrow();
        let ball1_neighbours = ball1_borrowed.hood.neighbours.borrow();

        assert_eq!(ball1_neighbours.len(), 1, "Ball1 should have one neighbor");
        assert_eq!(ball1_neighbours[0].upgrade().is_some(), true, "Ball2 should be in the neighbors list of Ball1");
    }
    #[test]
    fn test_update_hood_does_not_add_neighbors_outside_distance_limit() {
        let mut ball1 = Ball2D::new_massive(1, 0.0, 0.0, 2.0, 1.0);
        let mut ball2 = Ball2D::new_massive(2, 10.0, 10.0, 2.0, 1.0);

        ball1.verbose = true;
        ball2.verbose = true;
        let ball1_rc = Rc::new(RefCell::new(ball1));
        let ball2_rc = Rc::new(RefCell::new(ball2));

        let dt = 1.0;
        let others = vec![ball2_rc.clone()];



        ball1_rc.borrow_mut().update_hood(dt, &ball1_rc, others);

        let ball1_borrowed = ball1_rc.borrow();
        let ball1_neighbours = ball1_borrowed.hood.neighbours.borrow();

        println!("neighbours of ball1: {:?}", ball1_borrowed.get_neighbour_names());
        assert_eq!(ball1_neighbours.len(), 0, "Ball1 should have no neighbors when Ball2 is outside the distance limit");
    }
    #[test]
    fn test_update_hood_no_neighbors() {
        let ball1 = Ball2D::new_massive(1, 0.0, 0.0, 2.0, 1.0);
        let ball1_rc = Rc::new(RefCell::new(ball1));

        let dt = 1.0;
        let others: Vec<Rc<RefCell<Ball2D>>> = Vec::new();

        ball1_rc.borrow_mut().update_hood(dt, &ball1_rc, others);

        let ball1_borrowed = ball1_rc.borrow();
        let ball1_neighbours = ball1_borrowed.hood.neighbours.borrow();

        assert_eq!(ball1_neighbours.len(), 0, "Ball1 should have no neighbors when the others vector is empty");
    }
    #[test]
    fn test_update_hood_multiple_neighbors() {
        let  ball1 = Ball2D::new_massive(1, 0.0, 0.0, 2.0, 1.0);
        let  ball2 = Ball2D::new_massive(2, 3.0, 4.0, 2.0, 1.0);
        let  ball3 = Ball2D::new_massive(3, 6.0, 8.0, 2.0, 1.0);

        let ball1_rc = Rc::new(RefCell::new(ball1));
        let ball2_rc = Rc::new(RefCell::new(ball2));
        let ball3_rc = Rc::new(RefCell::new(ball3));

        let dt = 1.0;
        let others = vec![ball2_rc.clone(), ball3_rc.clone()];

        ball1_rc.borrow_mut().update_hood(dt, &ball1_rc, others);

        let ball1_borrowed = ball1_rc.borrow();
        let ball1_neighbours = ball1_borrowed.hood.neighbours.borrow();

        assert_eq!(ball1_neighbours.len(), 2, "Ball1 should have two neighbors");
    }
    #[test]
    fn test_update_hood_indirect_neighbors() {
        let  ball1 = Ball2D::new_massive(1, 0.0, 0.0, 2.0, 1.0);
        let  ball2 = Ball2D::new_massive(2, 3.0, 0.0, 2.0, 1.0);
        let  ball3 = Ball2D::new_massive(3, 6.5, 0.0, 2.0, 1.0);

        let ball1_rc = Rc::new(RefCell::new(ball1));
        let ball2_rc = Rc::new(RefCell::new(ball2));
        let ball3_rc = Rc::new(RefCell::new(ball3));

        let dt = 1.0;

        // Ball1 updates its hood with Ball2 as a potential neighbor
        ball1_rc.borrow_mut().update_hood(dt, &ball1_rc, vec![ball2_rc.clone()]);

        // Ball2 updates its hood with Ball1 and Ball3 as potential neighbors
        ball2_rc.borrow_mut().update_hood(dt, &ball2_rc, vec![ball1_rc.clone(), ball3_rc.clone()]);

        // Ball3 updates its hood with Ball2 as a potential neighbor
        ball3_rc.borrow_mut().update_hood(dt, &ball3_rc, vec![ball2_rc.clone()]);

        let ball1_borrowed = ball1_rc.borrow();
        let ball2_borrowed = ball2_rc.borrow();
        let ball3_borrowed = ball3_rc.borrow();

        let ball1_neighbours = ball1_borrowed.hood.neighbours.borrow();
        let ball2_neighbours = ball2_borrowed.hood.neighbours.borrow();
        let ball3_neighbours = ball3_borrowed.hood.neighbours.borrow();

        // Check if Ball2 is in Ball1's neighborhood
        let ball1_has_ball2 = ball1_neighbours.iter().any(|w| w.upgrade().map_or(false, |b| b.borrow().id == 2));
        assert!(ball1_has_ball2, "Ball2 should be a neighbor of Ball1");

        // Check if Ball1 is in Ball2's neighborhood
        let ball2_has_ball1 = ball2_neighbours.iter().any(|w| w.upgrade().map_or(false, |b| b.borrow().id == 1));
        assert!(ball2_has_ball1, "Ball1 should be a neighbor of Ball2");

        // Check if Ball3 is in Ball2's neighborhood
        let ball2_has_ball3 = ball2_neighbours.iter().any(|w| w.upgrade().map_or(false, |b| b.borrow().id == 3));
        assert!(ball2_has_ball3, "Ball3 should be a neighbor of Ball2");

        // Check if Ball2 is in Ball3's neighborhood
        let ball3_has_ball2 = ball3_neighbours.iter().any(|w| w.upgrade().map_or(false, |b| b.borrow().id == 2));
        assert!(ball3_has_ball2, "Ball2 should be a neighbor of Ball3");

        // Check that Ball1 is NOT in Ball3's neighborhood
        let ball3_has_ball1 = ball3_neighbours.iter().any(|w| w.upgrade().map_or(false, |b| b.borrow().id == 1));
        assert!(!ball3_has_ball1, "Ball1 should NOT be a neighbor of Ball3");

        // Check that Ball3 is NOT in Ball1's neighborhood
        let ball1_has_ball3 = ball1_neighbours.iter().any(|w| w.upgrade().map_or(false, |b| b.borrow().id == 3));
        assert!(!ball1_has_ball3, "Ball3 should NOT be a neighbor of Ball1");
    }


}
