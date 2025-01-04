use crate::objects2d::point_2d::Vec2D;
use ggez::graphics::Rect;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

const MAX_UPDATE_RECURSION: i32 = 10;
const LJ_SIG: f32 = 5.; // equilibrium distance 
const LJ_EPS: f32 = 1.;// attractive strength

#[derive(Debug)]
pub struct Ball2D<'a> {
    pub radius: f32,
    pub pos: Vec2D,
    id: usize,
    pub mass: f32,
    pub v: Vec2D,
    hood: Verlet<'a>,
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
       let dist_limit_by_v = v_sq * dt * dt + eq * eq + 2. * dt * eq * (self.v.x + self.v.y);
       let dist_limit_by_closest = if self.hood.nearest > 0. { self.hood.nearest * self.hood.nearest *4. } else { f32::MAX };
       let dist_limit = f32::max(dist_limit_by_closest, dist_limit_by_v); 
       println!("update_hood: {} dist_lim: {}", self.get_name(), f32::sqrt(dist_limit));
     

       for cur_rc in others.iter() {

         let cur_ref = cur_rc.borrow();
         let dist_sq=Vec2D::norm_sq(&self.pos.dist(&cur_ref.pos));
         //println!("checking {}", cur_ref.get_name());
         if dist_sq < dist_limit {
             //cur.hood.neighbours.push(self.into());
             //let new_neighbour = Rc::new( RefCell::new(cur));
             self.hood.neighbours.borrow_mut().push(Rc::downgrade(cur_rc));
             cur_ref.hood.neighbours.borrow_mut().push(Rc::downgrade(self_rc));
             println!("\tneighbour {}", cur_ref.get_name());
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
       println!("update_closeness: {} nearest/farthest: {}/{}", self.get_name(), self.hood.nearest, self.hood.farthest);
    }

    pub fn is_colliding (&self, other: &Ball2D) -> bool {
        //use norm_sq to avoid sqrt in norm
        let res = Vec2D::norm(&self.pos.dist(&other.pos))<(self.radius + other.radius);
        if res {
            println!("ball {} at {} is too close to {}: dist = {} sum_rad = {}", self.id, self.pos, other.pos, 
                Vec2D::norm(&self.pos.dist(&other.pos)),
                self.radius + other.radius)
        }
        res
    }

    fn calc_mean_distance(radius_sum: f32) -> f32 {
        (radius_sum + 1.12*LJ_SIG)/1.1
    }
    
    fn lenard_jones_potential(dist: f32, radius_sum:f32) -> f32 {
                                  //
        let r_m = Ball2D::calc_mean_distance(radius_sum);
        let quot = r_m/dist;
        let quot_six = quot*quot*quot*quot*quot*quot;
        4.* LJ_EPS* quot_six * (quot_six - 1.)
    }

    fn lenard_jones_force(dist: f32, radius_sum:f32, _id: usize)-> f32 {

        let r_m = Ball2D::calc_mean_distance(radius_sum);
                                  //
        let quot = r_m/dist;
        let quot_six = quot*quot*quot*quot*quot*quot;
        let res = 4. * LJ_EPS*  quot_six * ( 12. * quot_six - 6.)/dist;
        //println!("{} ------------ dist: {} ljForce: {} rad_sum: {} quot: {} r_m: {}", 
        //   name, dist, res, radius_sum, quot, r_m);
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
            if let Some(b) = ref_b.upgrade() {

                //from other_ball to this_ball
                let dist_vec = b.borrow().pos.dist(&self.pos);
                let dist = Vec2D::norm(&dist_vec);
                let radsum = b.borrow().radius + self.radius; 
                min_dist = match Ball2D::check_min_dist(dist - radsum*0.9, min_dist) {
                    Err(BallError::DistanceUnderflow) | Err(BallError::LessThanTwoBalls) => return(0., Vec2D::new(0.,0.), 0.),
                    Ok(dist_result) => dist_result,
                };

                let lj_force = Ball2D::lenard_jones_force(dist, radsum, self.id);
                let force_to_dist_val = lj_force/dist;
                let local_force = Vec2D::new(dist_vec.x*force_to_dist_val, dist_vec.y*force_to_dist_val);

                //println!("{} ------------ added force: {} for dist {}", self.get_name(), local_force, dist_vec);
                force.add(local_force);
                total_pot += Ball2D::lenard_jones_potential(dist, radsum);
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

        println!("{} ------ [{:.2},{:.2}] neighbours: {:#?}", self.get_name(), self.hood.nearest, self.hood.farthest, self.hood.get_neighbour_names());

        let (min_dist, total_force, total_pot) = self.force();
        let speed_limit = f32::min(bound.w, bound.h)/1E-1;
        let (fb, accel) = self.accel_with_speed_limit(&total_force, speed_limit);

        let a_norm = Vec2D::norm(&accel);
        let p = Vec2D::norm(&self.v)/a_norm;
        
        if fb>a_norm {
            println!("{} --- has active friction brake: {} at accel {} and force {}", self.get_name(),  fb, a_norm, total_force);
        }

        let dx = self.v.x * dt + 0.5*accel.x * dt * dt ;
        let dy = self.v.y * dt + 0.5*accel.y * dt * dt;
        
        let mut dt_max = dt;
        if min_dist > 0. && (dx*dx+dy*dy) > (min_dist*min_dist*0.5) {
            dt_max = -p + f32::sqrt(p*p+min_dist/ a_norm);
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

        println!("{} --- dt: {} new pos: {} new v: {}", self.get_name(), dt, self.pos, self.v);
        (min_dist, total_pot)
    }

}

impl<'a> Clone for Verlet<'a> {
    fn clone(&self) -> Verlet<'a> {
        Verlet {
            nearest: self.nearest,
            farthest: self.farthest,
            neighbours: self.neighbours.clone(),
        }
    }
}

impl<'a> Clone for Ball2D<'a> {
    fn clone(&self) -> Ball2D<'a> {
       Ball2D {
           id: self.id,
           pos: self.pos.clone(),
           radius: self.radius,
           mass: self.mass,
           v: self.v.clone(),
           hood: self.hood.clone(),
       }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_name() {
        let b = Ball2D::new_massive(0, 0., 0., 3., 1.);
        assert_eq!(b.get_name(), "Ball0");

    }
}
