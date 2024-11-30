use crate::objects2d::point_2d::Vec2D;
use ggez::graphics::Rect;
use std::process;

const MAX_UPDATE_RECURSION: i32 = 10;
const LJ_SIG: f32 = 5.; // equilibrium distance 
const LJ_EPS: f32 = 1.;// attractive strength

pub struct Ball2D {
    pub radius: f32,
    pub pos: Vec2D,
    name: String,
    pub mass: f32,
    pub v: Vec2D,
    hood: Verlet,
}

#[derive(Debug)]
struct NeighbourBall {
    radius: f32,
    pos: Vec2D,
    name: String,
}

struct Verlet {
    pub neighbours: Vec<NeighbourBall>,
    pub nearest: f32,
    pub farthest: f32,
}

impl Default for Verlet {
    fn default() -> Verlet {
        Verlet {
        neighbours: Vec::new(),
        nearest: -1.,
        farthest: -1.,
        }
    }
}

impl Verlet {

    fn get_neighbour_names(&self) -> Vec<String> {
        self.neighbours.iter().map(|n| n.get_name()).collect()
    }

}

impl NeighbourBall {

    fn get_name(&self) -> String {
        self.name.clone()
    }
}

impl From<&mut Ball2D> for NeighbourBall {
   fn from(value: &mut Ball2D) -> Self {
       NeighbourBall{
           radius: value.radius,
           pos: value.pos.clone(),
           name: value.name.clone(),
       }
    } 
}

enum BallError {
    DistanceUnderflow,
    LessThanTwoBalls,
}

type BallResult = Result<f32, BallError>;

impl Default for Ball2D {
    fn default() -> Ball2D {
        Ball2D {
            radius: 1.,
            pos: Vec2D::new(0.,0.),
            name: format!("default"),
            mass: 1.,
            v: Vec2D::new(0.,0.),
            hood: Verlet { ..Default::default() },
        }
    }
}

impl Ball2D {
    pub fn _new_sized(name: String, x: f32, y: f32, radius: f32) -> Ball2D {
        Ball2D::new_massive(name, x, y, radius, 1.)
    }

    pub fn new_massive(_name: String, _x: f32, _y: f32, _radius: f32, _m: f32) -> Ball2D{
        Ball2D {
            name: _name, pos: Vec2D::new(_x,_y), radius: _radius, mass: _m, v: Vec2D::new(0.,0.),
            ..Default::default()
        }
    }

    pub fn moment(&self) -> Vec2D{
        Vec2D::copy_scaled(&self.v, self.mass)
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }
    
    pub fn update_hood(&mut self, dt: f32, others: Vec<&mut Ball2D>) {
       //use approximation for (v*dt+LJ_SIG)^2
       let v_sq = Vec2D::norm_sq(&self.v);
       let eq= LJ_SIG + self.radius;
       let dist_limit_by_v = v_sq * dt * dt + eq * eq + 2. * dt * eq * (self.v.x + self.v.y);
       let dist_limit_by_closest = if self.hood.nearest > 0. { self.hood.nearest * self.hood.nearest *4. } else { f32::MAX };
       let dist_limit = f32::max(dist_limit_by_closest, dist_limit_by_v); 
       println!("dist_lim: {}", dist_limit);
      
       for cur in others {
         let dist_sq=Vec2D::norm_sq(&self.pos.dist(&cur.pos));
         println!("checking {}", cur.name);
         if dist_sq < dist_limit {
             cur.hood.neighbours.push(self.into());
             self.hood.neighbours.push(cur.into());
             println!("updated {}-neighbours {:?}", self.name, self.hood.neighbours);
             println!("updated {}-neighbours {:?}", cur.name, cur.hood.neighbours);
         }
       }
       let mut nearest_sq = f32::MAX;
       let mut farthest_sq:f32 = 0.;
       for cur in &self.hood.neighbours {
         let dist_sq=Vec2D::norm_sq(&self.pos.dist(&cur.pos));
         if dist_sq < nearest_sq {
            nearest_sq = dist_sq;
         }
         if dist_sq > farthest_sq {
             farthest_sq = dist_sq;
         }
       }
       self.hood.nearest = f32::sqrt(nearest_sq);
       self.hood.farthest = f32::sqrt(farthest_sq);
    }

    pub fn is_colliding (&self, other: &Ball2D) -> bool {
        //use norm_sq to avoid sqrt in norm
        let res = Vec2D::norm(&self.pos.dist(&other.pos))<(self.radius + other.radius);
        if res {
            println!("ball {} at {} is too close to {}: dist = {} sum_rad = {}", self.name, self.pos, other.pos, 
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

    fn lenard_jones_force(dist: f32, radius_sum:f32, _name: &String)-> f32 {

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
        for b in &self.hood.neighbours {
            if b.name == self.name {
                continue;
            }
            //from other_ball to this_ball
            let dist_vec = b.pos.dist(&self.pos);
            let dist = Vec2D::norm(&dist_vec);
            let radsum = b.radius + self.radius; 
            min_dist = match Ball2D::check_min_dist(dist - radsum*0.9, min_dist) {
                Err(reason) => {
                    match reason {
                        BallError::LessThanTwoBalls => {
                            return(0., Vec2D::new(0.,0.), 0.);
                        },
                        BallError::DistanceUnderflow => {
                            panic!("ball {} and {} separated by {} have pos {} and {} speed {} ", self.name, b.name, dist - radsum, self.pos, b.pos, self.v);
                        }
                    }
                }
                Ok(dist_result) => dist_result,
            };

            let lj_force = Ball2D::lenard_jones_force(dist, radsum, &self.name);
            let force_to_dist_val = lj_force/dist;
            let local_force = Vec2D::new(dist_vec.x*force_to_dist_val, dist_vec.y*force_to_dist_val);

            //println!("{} ------------ added force: {} for dist {}", self.name, local_force, dist_vec);
            force.add(local_force);
            total_pot += Ball2D::lenard_jones_potential(dist, radsum);
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
        self.hood.neighbours.clear();
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

    pub fn update(&mut self, dt: f32, bound: &Rect) -> (f32, f32) {

        println!("{} ------ [{:.2},{:.2}] neighbours: {:#?}", self.name, self.hood.nearest, self.hood.farthest, self.hood.get_neighbour_names());

        //let (min_dist, total_force, total_pot) = self.force_by(others);
        let (min_dist, total_force, total_pot) = self.force();
        let speed_limit = f32::min(bound.w, bound.h)/1E-1;
        let (fb, accel) = self.accel_with_speed_limit(&total_force, speed_limit);
        let a_norm = Vec2D::norm(&accel);
        if fb>a_norm {
            println!("{} --- has active friction brake: {} at accel {} and force {}", self.name,  fb, a_norm, total_force);
        }
        let p = Vec2D::norm(&self.v)/a_norm;

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
        if int_steps > 1 {
            println!("{} --- handle overflow with {} substeps of dt_sub: {}", self.name,  int_steps, dt_sub);
            println!("steps: {} ({}) min_dist: {} dt_max: {} dt: {} total_force: {} accel: {} vel: {}",
                    int_steps, steps,  min_dist, dt_max, dt, total_force, accel, self.v);
            if int_steps > MAX_UPDATE_RECURSION {
                process::exit(64);
            }
            self.execute_update(&accel, dt_sub, bound);
                println!("{} ---{}-- dt: {} new pos: {} new v: {} fb: {}", self.name, 0, dt, self.pos, self.v, fb);
            for _i in 1..int_steps {
                //let (_min_dist, total_force, _total_pot) = self.force_by(others);
                let (_min_dist, total_force, _total_pot) = self.force();
                let (fb, accel) = self.accel_with_speed_limit(&total_force, speed_limit);
                self.execute_update(&accel, dt_sub, bound);
                println!("{} ---{}-- dt: {} new pos: {} new v: {} fb: {}", self.name, _i, dt, self.pos, self.v, fb);
            }
        } else {
            //update velocity after time step was deemed correct
            self.execute_update(&accel, dt, bound);
        }
        println!("{} --- dt: {} new pos: {} new v: {}", self.name, dt, self.pos, self.v);
        (min_dist, total_pot)
    }

}

impl Clone for Verlet {
    fn clone(&self) -> Verlet {
        Verlet {
            nearest: self.nearest,
            farthest: self.farthest,
            neighbours: self.neighbours.clone(),
        }
    }
}

impl Clone for NeighbourBall {
    fn clone(&self) -> NeighbourBall {
        NeighbourBall {
            name: self.name.clone(),radius: self.radius, pos: self.pos.clone(),
        }
    }
}
/*
impl Clone for Ball2D {
    fn clone(&self) -> Ball2D {
       Ball2D {
           name: self.name.clone(),
           pos: self.pos.clone(),
           radius: self.radius,
           mass: self.mass,
           v: self.v.clone(),
           hood: self.hood.clone(),
       }
    }
}*/
