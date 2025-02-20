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
        nearest: -1.,
        farthest: -1.,
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

enum BallError {
    DistanceUnderflow,
    LessThanTwoBalls,
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
        Vec2D::copy_scaled(&self.v, self.mass)
    }


    pub fn update_hood(&mut self, dt: f32, self_rc: &Rc<RefCell<Self>>, others: Vec<Rc<RefCell<Ball2D<'a>>>>) {
       //use approximation for (v*dt+LJ_SIG)^2
       let v_sq = Vec2D::norm_sq(&self.v);
       let eq= LJ_SIG + self.radius;
       //we only check hood every ten updates --> factors 100 and 10
       let dist_limit_by_v = 100.*v_sq * dt * dt + eq * eq + 2. * 10. * dt * eq * (self.v.x + self.v.y);
       let dist_limit_by_closest = if self.hood.nearest > 0. { 
           let near_used = f32::max(self.hood.nearest, self.radius+LJ_SIG*2.);
           near_used * near_used *4. 
       } else { 
           //f32::MAX 
           (40.*LJ_SIG)*(40.*LJ_SIG)*4.
       };
       let dist_limit = f32::max(dist_limit_by_closest, dist_limit_by_v); 
       if self.verbose {
           println!("update_hood: {} dist_lim: {} #others: {}", self.get_name(), f32::sqrt(dist_limit), others.len());
       }
     

       for cur_rc in others.iter() {

         let cur_ref = cur_rc.borrow();
         let dist_sq=Vec2D::norm_sq(&self.pos.dist(&cur_ref.pos));
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

       let (nearest_sq, farthest_sq) = self.hood.neighbours
           .borrow()
           .iter()
           .filter_map(|weak| weak.upgrade())
           .map(|val| Vec2D::norm_sq(&self.pos.dist(&val.borrow().pos)))
           .fold((f32::INFINITY, f32::NEG_INFINITY),
            |(min, max), val| {
                ( min.min(val), max.max(val) )
            });
           

       if nearest_sq.is_finite() {
           self.hood.nearest = f32::sqrt(nearest_sq);
       }
       if farthest_sq.is_finite() {
           self.hood.farthest = f32::sqrt(farthest_sq);
       }
       if self.verbose {
           println!("update_closeness: {} nearest/farthest: {}/{}", self.get_name(), self.hood.nearest, self.hood.farthest);
       }
    }

    pub fn is_colliding (&self, other: &Ball2D) -> bool {
        //use norm_sq to avoid sqrt in norm
        let radius_sum_sq = (self.radius +  other.radius)*(self.radius + other.radius);
        let res = Vec2D::norm_sq(&self.pos.dist(&other.pos))<radius_sum_sq;
        if res {
            println!("ball {} at {} is too close to {}: dist = {} sum_rad = {}", self.id, self.pos, other.pos, 
                Vec2D::norm(&self.pos.dist(&other.pos)),
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
        if actual_dist < 0. {
            return Err(BallError::LessThanTwoBalls);
        }
        if actual_dist < min_dist {
            if actual_dist <= 0. {
                return Err(BallError::DistanceUnderflow);
            }
            return Ok(actual_dist);
        }
        return Ok(min_dist);
    }

    //fn force_by(&self, others: &Vec<Ball2D>)  -> (f32, Vec2D, f32) {
    fn force(&self) -> (f32, Vec2D, f32) {
        let mut force = Vec2D::new(0.,0.);
        let mut min_dist = -1.;
        let mut total_pot = 0.;
        //for b in others {
        for ref_b in self.hood.neighbours.borrow().iter(){
            if let Some(b_rc) = ref_b.upgrade() {

                //from other_ball to this_ball
                let b = b_rc.borrow();
                let dist_vec = b.pos.dist(&self.pos);
                let dist = Vec2D::norm(&dist_vec);
                let radsum = b.radius + self.radius; 
                /*min_dist = match Ball2D::check_min_dist(dist - radsum*0.9, min_dist) {
                    Err(BallError::DistanceUnderflow) | Err(BallError::LessThanTwoBalls) => return(0., Vec2D::new(0.,0.), 0.),
                    Ok(dist_result) => dist_result,
                };*/

                match  Ball2D::check_min_dist(dist - radsum*0.9, min_dist) {
                    Ok(min_dist_cur) => {
                        min_dist = min_dist_cur;
                        let lj_force = Ball2D::lenard_jones_force(dist, radsum, self.id);
                        let force_to_dist_val = lj_force/dist;
                        let local_force = Vec2D::new(dist_vec.x*force_to_dist_val, dist_vec.y*force_to_dist_val);

                        if self.verbose {
                            println!("\t\t ball {}--force: {:?} at dist: {}", b.get_name(), local_force, dist);
                        }

                        force.add(local_force);
                        total_pot += Ball2D::lenard_jones_potential(dist, radsum);
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
                        total_pot += Ball2D::lenard_jones_potential(dist, radsum);
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
        let speed_sq = Vec2D::norm_sq(&self.v);
        let _friction_number = speed_sq/speed_limit/speed_limit;
        let friction_braking = 2. * _friction_number * _friction_number;
        let mut accel: Vec2D = Vec2D::copy_scaled(&total_force,1./self.mass);
        accel.add(Vec2D::new(-friction_braking*self.v.x, -friction_braking*self.v.y));
        //println!("... F_t: {} and limit: {} for speed {}", total_force, speed_limit, self.v);
        //println!("...... friction_braking:{} and mass: {}", friction_braking, self.mass);
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

        let a_norm = Vec2D::norm(&accel);
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
    use float_cmp::approx_eq;

    use super::*;
    
    #[test]
    fn test_name() {
        let b = Ball2D::new_massive(0, 0., 0., 3., 1.);
        assert_eq!(b.get_name(), "Ball0");

    }

    
    #[test]
    fn test_verlet_default(){
        let vlist: Verlet = Verlet::default();
        assert_eq!(vlist.nearest, -1.);
        assert_eq!(vlist.farthest, -1.);

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
    fn test_lenard_jones_equilibrium_distance(){

        let ball1 = Ball2D::new_massive(17, 2., 3., 4., 5.,);
        let distance = ball1.equilibrium_radius()*2.;
        let result_force: f32 = Ball2D::lenard_jones_force(
            distance,
            ball1.radius*2., 
            0);

        assert!(result_force.abs()<1e-4,
            "force {} is not close to 0", result_force);
    }

}
