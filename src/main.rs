use std::fmt;
use ggez::glam::Vec2;
use ggez::{Context, ContextBuilder, GameResult};
use ggez::mint::Point2;
use ggez::event::{self, EventHandler, MouseButton};
use ggez::graphics::{self, Color, DrawMode, Mesh, Rect};

struct Point2D (f32, f32);
impl fmt::Display for Point2D {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.0, self.1)
    }
}

impl Into<Point2<f32>> for Point2D {
    fn into(self) -> Point2<f32> {
        Point2 { x: self.0, y: self.1 }
    }
}

impl Clone for Point2D {
    fn clone(&self) -> Point2D {
        Point2D(self.0, self.1)
    }
}

struct MainContent {
    v_points: Vec<Point2D>,
    curs_pos: Point2D,
    region_size_x: f32,
    region_size_y: f32,
    margin_x: f32,
    margin_y: f32,
}

impl MainContent {
    fn new(reg_x:f32, reg_y: f32) -> GameResult<MainContent> {
        let s = MainContent {
            curs_pos: Point2D(0.,0.),
            v_points: Vec::new(),
            region_size_x: reg_x,
            region_size_y: reg_y,
            margin_x: 50.,margin_y: 50.,
        };
        Ok(s)
    }

    fn draw_cursor(&self, _ctx: &mut Context, canvas: &mut graphics::Canvas) -> GameResult {
        let cursor_circle = Mesh::new_circle(_ctx,DrawMode::fill(),self.curs_pos.clone(),
            3.0,
            0.1,
            Color::WHITE 
        )?;

        canvas.draw(&cursor_circle, Vec2::new(0.,0.));
        Ok(())
    }

    fn draw_frame(&self, _ctx: &mut Context, canvas: &mut graphics::Canvas) -> GameResult {
        let rect = Mesh::new_rectangle(_ctx, DrawMode::stroke(3.), Rect::new(
                self.margin_x,
                self.margin_y,
                self.region_size_x,
                self.region_size_y
                ),Color::BLUE)?;

        canvas.draw(&rect, Vec2::new(0.,0.));
        Ok(())
    }
}

impl EventHandler for MainContent {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        //no logic yet
        Ok(())
    }

    fn mouse_motion_event(&mut self, _ctx: &mut Context, x: f32, y: f32, _dx: f32, _dy: f32) -> GameResult {
        self.curs_pos = Point2D(x, y);
        Ok(())
    }

    fn draw(&mut self, _ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(_ctx, Color::from_rgb(0,0,0)); 
        self.draw_frame(_ctx, &mut canvas)?;
        self.draw_cursor(_ctx, &mut canvas)?;
        canvas.finish(_ctx)?;

        Ok(())
    }
}

fn main() -> GameResult { 
    let _square_points: [Point2D; 4] = [ Point2D(0., 0.), Point2D(1.,0.), Point2D(1.,1.), Point2D(0.,1.) ]; 
    //for pt in &square_points {
        //println!("{}", pt);
    //}
    let (ctx, event_loop) = ContextBuilder::new("ThermoRust", "Rainer Kaltseis")
        .build()
        .expect("Failed to build context");

    let state = MainContent::new(800.,500.)?;
    event::run(ctx, event_loop, state)
}

