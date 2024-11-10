use std::fmt;
use ggez::glam::Vec2;
use ggez::{Context, ContextBuilder, GameResult};
use ggez::mint::Point2;
use ggez::event::{self, EventHandler, MouseButton};
use ggez::graphics::{self, Color, DrawMode, Mesh, Rect};
use ggez::conf::{WindowSetup, WindowMode};
use ggez::input::mouse;
use ggez::timer;

const MARGIN_X: f32 = 50.;
const MARGIN_Y: f32 = 50.;

struct Point2D {
    x: f32,
    y: f32,
}

struct Ball2D {
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
        pt.x*pt.x + pt.y + pt.y
    }

    fn norm(pt: &Point2D) -> f32 {
        f32::sqrt(Point2D::norm_sq(pt))
    }
    
    fn copy_scaled(pt: &Point2D, scale:f32) -> Point2D {
        Point2D::new( pt.x * scale, pt.y * scale)
    }

    fn dist(&self, other: &Point2D) -> Point2D {
        let dist_x = self.x - other.x;
        let dist_y = self.y - other.y;
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
    fn new_sized(x: f32, y: f32, radius: f32)  -> Ball2D {
        Ball2D::new_massive(x, y, radius, 1.)
    }
    fn new_massive(_x: f32, _y: f32, _radius: f32, _m: f32) -> Ball2D{
        Ball2D {
            pos: Point2D::new(_x,_y), radius: _radius, mass: _m, v: Point2D::new(0.,0.)
        }
    }

    fn is_colliding (&self, other: &Ball2D) -> bool {
        Point2D::norm(&self.pos.dist(&other.pos))<(self.radius + other.radius)  
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

    fn lenard_jones_force(dist: f32, radius_sum:f32)-> f32 {
        const LJ_SIG: f32 = 5.; // equilibrium distance 
        const LJ_EPS: f32 = 1E-3; // attractive strength

        let r_m = radius_sum + 1.12*LJ_SIG;
                                  //
        let quot = r_m/dist;
        let quot_six = quot*quot*quot*quot*quot*quot;
        4. * LJ_EPS*  quot_six * ( 12. * quot_six - 6.)/dist
    }

    fn force_by(&self, others: Vec<Ball2D>) -> Point2D {
        let mut force = Point2D::new(0.,0.);
        for b in others {
            let dist_vec = b.pos.dist(&self.pos);
            let dist = Point2D::norm(&dist_vec);
            
            let radsum = b.radius + self.radius; 


            let force_val = Ball2D::lenard_jones_force(dist, radsum);
            let local_force = Point2D::new(dist_vec.x/dist*force_val, dist_vec.y/dist*force_val);

            force.add(local_force);
        }
        force
    }

    fn update(&mut self, dt: f32, others: Vec<Ball2D>) {
        let total_force = self.force_by(others);
        let accel: Point2D = Point2D::copy_scaled(&total_force,1./self.mass);
        self.v.x += accel.x * dt;
        self.v.y += accel.y * dt;

        self.pos.x += self.v.x*dt + accel.x*dt*dt*0.5;
        self.pos.y += self.v.y*dt + accel.y*dt*dt*0.5;
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
           pos: self.pos.clone(),radius: self.radius, mass: self.mass, v: self.v.clone(),
       }
    }
}

struct MainContent {
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
            3.0,
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

        let x = 3.*self.margin_x/2.+ self.region_size_x;
        let y = self.margin_y/2.;
        let sample_pt = Ball2D::new_massive(x, y, self.ball_size, self.ball_mass);
        let pt_sample = Mesh::new_circle(ctx,DrawMode::fill(),
            sample_pt.pos.clone(),
            sample_pt.radius,
            0.1,
            Color::YELLOW
        )?;
        canvas.draw(&pt_sample, Vec2::new(0.,0.));
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
}

impl EventHandler for MainContent {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {

        let dt = _ctx.time.delta().as_secs_f32();
        let other_balls = self.v_points.clone();
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

                let candidate = Ball2D::new_massive(x,y, self.ball_size, self.ball_mass);
                if self.is_ball_in_region(&candidate) {
                    self.v_points.push(candidate);
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
        self.draw_cursor(ctx, &mut canvas)?;
        


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

