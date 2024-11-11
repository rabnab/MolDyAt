use crate::objects2d::point_2d::Point2D;
use ggez::graphics::Rect;
use std::process;
use std::f32::EPSILON;

const MAX_UPDATE_RECURSION: i32 = 100000;

pub struct Ball2D {
    pub radius: f32,
    pub pos: Point2D,
    name: String,
    mass: f32,
    v: Point2D,
}


impl Ball2D {
    pub fn new_sized(name: String, x: f32, y: f32, radius: f32)  -> Ball2D {
        Ball2D::new_massive(name, x, y, radius, 1.)
    }
    pub fn new_massive(_name: String, _x: f32, _y: f32, _radius: f32, _m: f32) -> Ball2D{
        Ball2D {
            name: _name, pos: Point2D::new(_x,_y), radius: _radius, mass: _m, v: Point2D::new(0.,0.)
        }
    }

    pub fn is_colliding (&self, other: &Ball2D) -> bool {
        //use norm_sq to avoid sqrt in norm
        //Point2D::norm_sq(&self.pos.dist(&other.pos))<(self.radius*self.radius + self.radius*other.radius*2. + other.radius*other.radius)  
        let res = Point2D::norm(&self.pos.dist(&other.pos))<(self.radius + other.radius);
        if res {
            println!("ball {} at {} is too close to {}: dist = {} sum_rad = {}", self.name, self.pos, other.pos, 
                Point2D::norm(&self.pos.dist(&other.pos)),
                self.radius + other.radius)
        }
        res
    }

    fn lenard_jones_potential(dist: f32, radius_sum:f32)->f32{
        const LJ_SIG: f32 = 5.; // equilibrium distance 
        const LJ_EPS: f32 = 1E-3; // attractive strength
                                  //
        let r_m = radius_sum + 1.12*LJ_SIG;

        let quot = r_m/dist;
        let quot_six = quot*quot*quot*quot*quot*quot;
        4.* LJ_EPS* quot_six * (quot_six - 1.)
    }

    fn lenard_jones_force(dist: f32, radius_sum:f32, level: i32, name: &String)-> f32 {
        const LJ_SIG: f32 = 50.; // equilibrium distance 
        const LJ_EPS: f32 = 1E-1; // attractive strength

        let r_m = radius_sum + 1.12*LJ_SIG;
                                  //
        let quot = r_m/dist;
        let quot_six = quot*quot*quot*quot*quot*quot;
        let res = 4. * LJ_EPS*  quot_six * ( 12. * quot_six - 6.)/dist;
        println!("{}.level {} ------------ dist: {} ljForce: {} rad_sum: {} quot: {} r_m: {}", 
           name, level, dist, res, radius_sum, quot, r_m);
        res
    }

    fn force_by(&self, others: &Vec<Ball2D>, level: i32) -> (f32, Point2D) {
        let mut force = Point2D::new(0.,0.);
        let mut min_dist:f32 = 1E6;
        for b in others {
            if b.name == self.name {
                continue;
            }
            //from other_ball to this_ball
            let dist_vec = b.pos.dist(&self.pos);
            let dist = Point2D::norm(&dist_vec);
            if dist >= EPSILON {

                let radsum = b.radius + self.radius; 
                if dist-radsum<min_dist {
                    min_dist = dist-radsum;
                    if min_dist<=0.{
                        println!("{}.level {} ------------ ERROR: dist {} radsum {} mdist {} pos {} other {}", self.name, level, dist, radsum, min_dist, self.pos, b.pos);
                        process::exit(32);
                    }
                }
            


                let lj_force = Ball2D::lenard_jones_force(dist, radsum, level, &self.name);
                let force_to_dist_val = lj_force/dist;
                let local_force = Point2D::new(dist_vec.x*force_to_dist_val, dist_vec.y*force_to_dist_val);

                println!("{}.level {} ------------ added force: {} for dist {}", self.name, level, local_force, dist_vec);
                force.add(local_force);
            }
        }
        //println!("force: {}", force);
        (min_dist, force)
    }

    pub fn update(&mut self, dt: f32, others: &Vec<Ball2D>, bound: &Rect, level: i32) {
        let (min_dist, total_force) = self.force_by(others, level);
        let accel: Point2D = Point2D::copy_scaled(&total_force,1./self.mass);

        let a_norm = Point2D::norm(&accel);
        let p = Point2D::norm(&self.v)/a_norm;


        let dx = self.v.x * dt + 0.5*accel.x * dt * dt;
        let dy = self.v.y * dt + 0.5*accel.y * dt * dt;
        
        let mut dt_max = dt;
        if dx*dx+dy*dy > min_dist*min_dist*0.5 {
            dt_max = -p + f32::sqrt(p*p+min_dist/ a_norm);
        }
        if dt_max < dt/(MAX_UPDATE_RECURSION as f32) {
            dt_max = 2. * dt/(MAX_UPDATE_RECURSION as f32);
        }
        let steps = f32::ceil(dt/dt_max);
        let int_steps = steps as i32;
        let dt_sub = dt/steps;
        if int_steps > 1 {
            println!("{}.level {} --- handle overflow with {} substeps of dt_sub: {}", self.name,  level, int_steps, dt_sub);
            println!("steps: {} ({}) min_dist: {} dt_max: {} dt: {} total_force: {} accel: {} vel: {}",
                    int_steps, steps,  min_dist, dt_max, dt, total_force, accel, self.v);
            if int_steps > MAX_UPDATE_RECURSION {
                process::exit(64);
            }
            for _i in 0..int_steps {
                    self.update(dt_sub, others, bound, level + 1);
            }
            return
        } 

        //update velocity after time step was deemed correct
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
        println!("{}.level {} --- dt: {} new pos: {} new v: {}", self.name, level, dt, self.pos, self.v);

        //println!("level {} --- dt: {} dx: {} dy: {}", level, dt, dx, dy);

    }
}

impl Clone for Ball2D {
    fn clone(&self) -> Ball2D {
       Ball2D {
           name: self.name.clone(),
           pos: self.pos.clone(),radius: self.radius, mass: self.mass, v: self.v.clone(),
       }
    }
}
