use crate::objects2d::{Point2D, Ball2D};
use ggez::{Context, GameResult};
use ggez::graphics::{Canvas, DrawMode, Mesh, Color, Rect};
use ggez::input::mouse::MouseButton;
use ggez::event::EventHandler;
use ggez::glam::Vec2;

pub const MARGIN_X: f32 = 50.;
pub const MARGIN_Y: f32 = 50.;

pub struct MainContent {
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
    pub fn new(reg_x:f32, reg_y: f32) -> GameResult<MainContent> {
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

    fn draw_cursor(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        let cursor_circle = Mesh::new_circle(ctx,DrawMode::fill(),self.curs_pos.clone(),
            self.ball_size,
            0.1,
            Color::WHITE 
        )?;

        canvas.draw(&cursor_circle, Vec2::new(0.,0.));
        Ok(())
    }

    fn draw_balls(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {

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


    fn draw_frame(&mut self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {

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
        
        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(0,0,0)); 
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
