use std::f32::EPSILON;
use std::process;
use std::fmt;
use ggez::glam::Vec2;
use ggez::{Context, ContextBuilder, GameResult};
use ggez::mint::Point2;
use ggez::event::{self, EventHandler, MouseButton};
use ggez::graphics::{self, Color, DrawMode, Mesh, Rect};
use ggez::conf::WindowMode;
use ggez::input::mouse;

const MARGIN_X: f32 = 50.;
const MARGIN_Y: f32 = 50.;
const MAX_UPDATE_RECURSION: i32 = 100000;

struct Point2D {
    x: f32,
    y: f32,
}

struct Ball2D {
    name: String,
    pos: Point2D,
    radius: f32,
    mass: f32,
    v: Point2D,
}

impl Point2D {
    fn new(_x: f32, _y: f32)  -> Point2D {
        Point2D {
            x: _x,
            y: _y,
        }
    }

    fn norm_sq(pt: &Point2D) -> f32 {
        pt.x*pt.x + pt.y * pt.y
    }

    fn norm(pt: &Point2D) -> f32 {
        f32::sqrt(Point2D::norm_sq(pt))
    }
    
    fn copy_scaled(pt: &Point2D, scale:f32) -> Point2D {
        Point2D::new( pt.x * scale, pt.y * scale)
    }


    /// calculates the distance vector from this ball to the other ball
    fn dist(&self, other: &Point2D) -> Point2D {
        let dist_x = other.x - self.x;
        let dist_y = other.y - self.y;
        Point2D::new(dist_x, dist_y)
    }
    
    fn add(&mut self, pt: Point2D) {
       self.x += pt.x;
       self.y += pt.y;
    }

    fn scale(&mut self, scale:f32) {
        self.x *= scale;
        self.y *= scale;
    }

    fn negate(&mut self) {
        self.scale(-1.);
    }
    

}


impl Ball2D {
    fn new_sized(name: String, x: f32, y: f32, radius: f32)  -> Ball2D {
        Ball2D::new_massive(name, x, y, radius, 1.)
    }
    fn new_massive(_name: String, _x: f32, _y: f32, _radius: f32, _m: f32) -> Ball2D{
        Ball2D {
            name: _name, pos: Point2D::new(_x,_y), radius: _radius, mass: _m, v: Point2D::new(0.,0.)
        }
    }

    fn is_colliding (&self, other: &Ball2D) -> bool {
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

    fn update(&mut self, dt: f32, others: &Vec<Ball2D>, bound: &Rect, level: i32) {
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

//        if dx>bound.w*0.25 || dy > bound.h*0.25 {
//            println!("{}.level {} --- handle overflow dt: {} dx: {} dy: {}", self.name,  level, dt, dx, dy);
//            if level> MAX_UPDATE_RECURSION {
//                println!("avoiding {}.level {} --- dt: {} dx: {} dy: {}", self.name, level, dt, dx, dy);
//                if dx>bound.w*0.25 {
//                    let scale = 0.5*bound.w*f32::signum(dx)/dx;
//                    self.update(dt*scale, others, bound, MAX_UPDATE_RECURSION);
//                } else {
//                    let scale = 0.5*bound.h*f32::signum(dy)/dy;
//                    self.update(dt*scale, others, bound, MAX_UPDATE_RECURSION);
//                }
//                process::exit(128);
//            } else {
//                println!("{}.level {}.1 --- dt: {}", self.name, level, dt*0.1 );
//                self.update(dt*0.1, others, bound, level + 1);
//                println!("{}.level {}.2 --- dt: {}", self.name,  level, dt*0.9 );
//                self.update(dt*0.9, others, bound, level +1);
//            }
//            return
//        } 
//
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

impl fmt::Display for Point2D {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

impl Into<Point2<f32>> for Point2D {
    fn into(self) -> Point2<f32> {
        Point2 { x: self.x, y: self.y }
    }
}

impl Clone for Point2D {
    fn clone(&self) -> Point2D {
        Point2D::new(self.x, self.y)
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

struct MainContent {
    num_balls: i32,
    v_points: Vec<Ball2D>,
    curs_pos: Point2D,
    region_size_x: f32,
    region_size_y: f32,
    margin_x: f32,
    margin_y: f32,
    ball_size: f32,
    ball_mass: f32,
}

impl MainContent {
    fn new(reg_x:f32, reg_y: f32) -> GameResult<MainContent> {
        let s = MainContent {
            num_balls: 0,
            curs_pos: Point2D::new(0.,0.),
            v_points: Vec::new(),
            region_size_x: reg_x,
            region_size_y: reg_y,
            margin_x: MARGIN_X,margin_y: MARGIN_Y,
            ball_size: 3.,
            ball_mass: 1.,
        };
        Ok(s)
    }

    fn draw_cursor(&self, ctx: &mut Context, canvas: &mut graphics::Canvas) -> GameResult {
        let cursor_circle = Mesh::new_circle(ctx,DrawMode::fill(),self.curs_pos.clone(),
            self.ball_size,
            0.1,
            Color::WHITE 
        )?;

        canvas.draw(&cursor_circle, Vec2::new(0.,0.));
        Ok(())
    }

    fn draw_balls(&self, ctx: &mut Context, canvas: &mut graphics::Canvas) -> GameResult {

        for ball in &self.v_points {
            let pt_circle = Mesh::new_circle(ctx,DrawMode::fill(),
                ball.pos.clone(),
                ball.radius,
                0.1,
                Color::RED
            )?;

            canvas.draw(&pt_circle, Vec2::new(0.,0.));
        }
        Ok(())
    }


    fn draw_frame(&mut self, ctx: &mut Context, canvas: &mut graphics::Canvas) -> GameResult {

        let rect = Mesh::new_rectangle(ctx, DrawMode::stroke(3.), Rect::new(
                self.margin_x,
                self.margin_y,
                self.region_size_x,
                self.region_size_y
                ),Color::BLUE)?;

        //let x = 3.*self.margin_x/2.+ self.region_size_x;
        //let y = self.margin_y/2.;
        //let sample_pt = Ball2D::new_massive("SampleBall".to_string(), x, y, self.ball_size, self.ball_mass);
        //let pt_sample = Mesh::new_circle(ctx,DrawMode::fill(),
        //    sample_pt.pos.clone(),
        //    sample_pt.radius,
        //    0.1,
        //    Color::YELLOW
        //)?;
        //canvas.draw(&pt_sample, Vec2::new(0.,0.));
        canvas.draw(&rect, Vec2::new(0.,0.));
        Ok(())
    }

    fn resize(&mut self, width: f32, height: f32) -> Result<(), ggez::GameError> {
        self.region_size_x = width-2.*self.margin_x;
        self.region_size_y = height-2.*self.margin_y;
        Ok(())
    }
    
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let min_x = self.margin_x + pt.radius;
        let max_x = min_x + self.region_size_x - 2.*pt.radius;
        let min_y = self.margin_y + pt.radius;
        let max_y = min_y + self.region_size_y - 2.*pt.radius;

        // Check if the point is within the region in both x and y directions
        let within_x = pt.pos.x >= min_x && pt.pos.x <= max_x;
        let within_y = pt.pos.y >= min_y && pt.pos.y <= max_y;

        let within = within_x && within_y;

        within
    }

    fn is_ball_colliding(&self, pt: &Ball2D) -> bool {
        self.v_points.iter().any(|ball| pt.is_colliding(ball))
    }
}

impl EventHandler for MainContent {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        println!("------------- starting update --------------------");
        let dt = _ctx.time.delta().as_secs_f32();

        let other_balls: Vec<_>  = self.v_points.clone();
        //println!("other balls: {} ", other_balls.len());
        let bound_rect = Rect::new(self.margin_x, self.margin_y, self.region_size_x, self.region_size_y);
        for cur in &mut self.v_points {
            cur.update(dt, &other_balls, &bound_rect, 0);
        }

        Ok(())
    }

    fn resize_event(&mut self, _ctx: &mut Context, _width: f32, _height: f32) -> Result<(), ggez::GameError> {
        self.resize(_width,_height)?;
        Ok(())
    }

    fn mouse_motion_event(&mut self, _ctx: &mut Context, x: f32, y: f32, _dx: f32, _dy: f32) -> GameResult {
        let used_x = f32::max(self.margin_x, f32::min(self.margin_x+self.region_size_x, x));
        let used_y = f32::max(self.margin_y, f32::min(self.margin_y+self.region_size_y, y));
        self.curs_pos = Point2D::new(used_x, used_y);
        Ok(())
    }

    fn mouse_button_down_event(&mut self, _ctx: &mut Context, button: MouseButton, x: f32, y: f32) -> GameResult {
        match button {
            MouseButton::Left => {

                let candidate = Ball2D::new_massive(format!("Ball{}",self.num_balls).to_string(), x,y, self.ball_size, self.ball_mass);
                if self.is_ball_in_region(&candidate) && !self.is_ball_colliding(&candidate) {
                    self.v_points.push(candidate);
                    self.num_balls+=1;
                }
            }
            MouseButton::Right => {
                self.ball_size+=3.;
                if self.ball_size > self.margin_y/2. {
                    self.ball_size = 3.;
                }
            }
            _ => (),
        }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        // Get the window's width and height
        
        let mut canvas = graphics::Canvas::from_frame(ctx, Color::from_rgb(0,0,0)); 
        self.draw_frame(ctx, &mut canvas)?;
        self.draw_balls(ctx, &mut canvas)?;

        //let mouse_pos = ctx.mouse.position();
        //if 0.<mouse_pos.x && mouse_pos.x<self.region_size_x+2.*self.margin_x {
            self.draw_cursor(ctx, &mut canvas)?;
        //}
        

        


        canvas.finish(ctx)?;

        Ok(())
    }
}

fn main() -> GameResult { 
    let (mut ctx, event_loop) = ContextBuilder::new("ThermoRust", "Rainer Kaltseis")
        .window_mode(WindowMode {
            resizable: true,
            min_width: MARGIN_Y,
            min_height: MARGIN_X,
            ..Default::default()
        })
        .build()
        .expect("Failed to build context");

    mouse::set_cursor_hidden(&mut ctx, true);
    let state = MainContent::new(800.,500.)?;
    event::run(ctx, event_loop, state)
}

