use crate::objects2d::{Vec2D, Ball2D};
use ggez::input::keyboard::KeyCode;
use ggez::{Context, GameResult };
use ggez::graphics::{Canvas, Color, DrawMode,DrawParam, Drawable, Mesh, Rect, Text,TextFragment, Shader,};
use ggez::input::mouse::MouseButton;
use ggez::event::EventHandler;
use ggez::glam::Vec2;

use std::cell::RefCell;
use std::rc::Rc;

pub const MARGIN_PX_X: f32 = 50.;
pub const MARGIN_PX_Y: f32 = 50.;
pub const KB: f32 = 1.380649E-23;
const U: f32 = 1.66E-27;
const MASS_SCALE: f32 = 39.948*U; //Ar 
const ENERGY_SCALE: f32 = 122.* KBOLTZMANN; //energy scale for Ar
const LEN_SCALE: f32 = 3.33729598E-9/6.;//f32::cbrt(6.631E-26/1.784)/6; //densit 1.784 kg/m³
const TIME_SCALE: f32 = MASS_SCALE*LEN_SCALE/ENERGY_SCALE;
const KBOLTZMANN: f32 = 1.38E-23;
const TEMP_SCALE: f32 = ENERGY_SCALE/KBOLTZMANN;
const LEN_TO_PX: f32 = 1./10.; // 50 px are one LEN_SCALE (SIG is about 0.5 to 1 LEN_SCALE)
                               
const DEFAULT_BALL_SIZE: f32 = 1.5;

const FPS_GOAL: u32 = 60;
const DT_GOAL: f32 = 1./(FPS_GOAL as f32); 

struct WindowDefinition {
    curs_pos: Vec2D,
    region_px_x: f32,
    region_px_y: f32,
    margin_px_x: f32,
    margin_px_y: f32,
}

struct NewBallDef {
    ball_size: f32,
    ball_mass: f32,
}

struct SimStats {
    energy_pot: f32,
    energy_kin: f32,
    temperature: f32,
    max_v: f32,
    max_id: usize,
    min_dist: f32,
    v_com: Vec2D,
    time: f32,
    pause: bool,
    speedup: f32,
}

/*
 * activate when needed
#[derive(AsStd140)]
struct ShaderUniforms {
    radius: f32,
    glow_reach: f32, 
}
*/


pub struct MainContent<'a> {
    win_definition: WindowDefinition,
    new_ball: NewBallDef,
    sim: SimStats,
    num_balls: usize,
    v_points: Vec<Rc<RefCell<Ball2D<'a>>>>,
    real_data: bool,
    step_by_step: bool,
    show_timings: bool,
    show_ball_stats: bool, 
    step_count: i32,
    shader: Option<Shader>,
}

impl Default for WindowDefinition {
    fn default() -> WindowDefinition {
        WindowDefinition {
            curs_pos: Vec2D::new(0.,0.),
            region_px_x: 100.,
            region_px_y: 100.,
            margin_px_x: 10.,
            margin_px_y: 10.,
        }
    }
}
impl Default for NewBallDef {
    fn default() -> NewBallDef {
        NewBallDef {
            ball_size: DEFAULT_BALL_SIZE,
            ball_mass: 1., //approx Ar mass
        }
    }
}
impl Default for SimStats {
    fn default() -> SimStats{
        SimStats {
            energy_pot: 0.,
            energy_kin: 0.,
            temperature: 0.,
            max_v: 0.,
            max_id: 0,
            min_dist: 0.,
            v_com: Vec2D::new(0.,0.),
            time: 0.,
            pause: false,
            speedup: 1.,
        }
    }
}

impl<'a> Default for MainContent<'a> {
    fn default() -> MainContent<'a> {
        MainContent {
            win_definition: WindowDefinition {
                ..Default::default()
            },
            sim: SimStats {
                ..Default::default()
            },
            new_ball: NewBallDef {
                ..Default::default()
            },
            num_balls: 0,
            v_points: Vec::new(),
            real_data: false,
            step_by_step: false,
            step_count: 0,
            show_timings: false,
            show_ball_stats: false, 
            shader: None,
        }
    }
}

impl<'a> MainContent<'a> {

    pub fn new(reg_x:f32, reg_y: f32, shader_in:Option<Shader>) -> GameResult<MainContent<'a>> {
        Ok(MainContent {
            win_definition: WindowDefinition {
                region_px_x: reg_x,
                region_px_y: reg_y,
                margin_px_x: MARGIN_PX_X,
                margin_px_y: MARGIN_PX_Y,
                ..Default::default()
            },
            v_points: Vec::new(),
            shader: shader_in,
            ..Default::default()
        })
    }

    fn draw_cursor(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        let cursor_circle = Mesh::new_circle(
            ctx,
            DrawMode::fill(),
            self.win_definition.curs_pos.clone(),
            Self::len_to_px(self.new_ball.ball_size),
            0.1,
            Color::WHITE 
        )?;

        canvas.draw(&cursor_circle, Vec2::new(0.,0.));

        let curs_pos = self.px_to_coord(self.win_definition.curs_pos.x, self.win_definition.curs_pos.y);

        let txt_cursor = Text::new( 
            TextFragment::new(
                format!("x: {}, y: {} pos ({:.3},{:.3})", 
                    self.win_definition.curs_pos.x,
                    self.win_definition.curs_pos.y,
                    curs_pos.x,
                    curs_pos.y,
                    )
                )
                .scale(10.));
        let txt_cursor_pos: [f32; 2] = match txt_cursor.dimensions(ctx) {
            None => [0., 0.], 
            Some(rec) => [
                self.win_definition.margin_px_x*2.+self.win_definition.region_px_x-rec.w,
                self.win_definition.margin_px_y*2.+self.win_definition.region_px_y-rec.h,
            ],
        };

        canvas.draw(&txt_cursor, DrawParam::default().dest(txt_cursor_pos));


        Ok(())
    }

    fn draw_text_on_ball(&self, ctx: &mut Context, canvas: &mut Canvas, ball: &Ball2D) -> GameResult {
        let txt_name = Text::new( 
            TextFragment::new(format!("{}", ball.get_id()))
            .scale(15.));

        let txt_name_pos: [f32; 2] = match txt_name.dimensions(ctx) {
            None => [0., 0.], 
            Some(rec) => [
                Self::len_to_px(ball.pos.x) - rec.w*0.5, 
                Self::len_to_px(ball.pos.y)  - rec.h*0.5,
            ],
        };

        canvas.draw(&txt_name, DrawParam::default().dest(txt_name_pos));
        Ok(())
    }


    fn draw_simple_balls(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        for ball_ref in &self.v_points {
            let ball = ball_ref.borrow();
            let ball_coord_px = self.coord_to_px(&ball.pos);
            let ball_size_px = Self::len_to_px(ball.radius);
            println!("drawing ball at {:?} of size {}", ball_coord_px, ball_size_px);
            //draw the ball itself
            let pt_circle = Mesh::new_circle(
                ctx,
                DrawMode::fill(),
                ball_coord_px,
                ball_size_px,
                0.1,
                if ball.get_id() == self.sim.max_id {
                    Color::CYAN 
                } else if ball.verbose {
                    Color::BLUE
                } else {
                    Color::RED
                },
            )?;
            canvas.draw(&pt_circle, Vec2::new(0.,0.));

            //draw the equilibrium position of an identical ball (different mass/size might
            //result in slightly different eq position)
            let pt_eq = Mesh::new_circle(
                ctx,
                DrawMode::stroke(1.),
                self.coord_to_px(&ball.pos),
                Self::len_to_px(ball.equilibrium_radius()),
                0.1,
                Color::MAGENTA,
            )?;
            canvas.draw(&pt_eq, Vec2::new(0.,0.));
            self.draw_text_on_ball(ctx, canvas, &ball)?;

        }
        Ok(())
    }

    fn draw_opengl_balls(&self, ctx: &mut Context, canvas: &mut Canvas, shader_used: &Shader) -> GameResult {
        canvas.set_shader(shader_used);
        //let ball_paramsa = ShaderParamsBuilder::new(&ShaderUniforms);
        //canvas.set_shader_params(ball_params);
        for ball_ref in &self.v_points {
            let ball = ball_ref.borrow();
            
            //to set variables shared between shader and rust code 
            //canvas.set_shader_params(params)
            let pt_influence = Mesh::new_circle(
                ctx,
                DrawMode::stroke(1.),
                self.coord_to_px(&ball.pos),
                Self::len_to_px(ball.equilibrium_radius()),
                0.1,
                Color::MAGENTA,
            )?;
            canvas.draw(&pt_influence, Vec2::new(0.,0.));
            //canvas.set_shader(&ggez::graphics::default_shader());
            self.draw_text_on_ball(ctx, canvas, &ball)?;
        }
        canvas.set_default_shader();
        Ok(())
    }

    fn draw_balls(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {

        if let Some(shader_used) = &self.shader {
            self.draw_opengl_balls(ctx, canvas, &shader_used)?;
        } else {
            self.draw_simple_balls(ctx, canvas)?;
        }
        Ok(())
    }

    fn status_text(&self) -> String {
        let text = if self.real_data {
            format!("time: {:.0}[ns] ({})\n#Balls: {}\nPotEnergy: {:.2E}[J]\nKinEnergy: {:.2E}[J]\nTotalE: {:.3E}[J]\nTemp: {:.2E}[K]\nmaxV: {:.2}[m/s]\nminDist: {:.2E}[m]\nvCOM: {}[m/s]",
                    self.sim.time*TIME_SCALE*1.0E9,
                    self.sim.speedup,
                    self.num_balls,
                    self.sim.energy_pot*ENERGY_SCALE,
                    self.sim.energy_kin*ENERGY_SCALE,
                    (self.sim.energy_pot + self.sim.energy_kin)*ENERGY_SCALE,
                    self.sim.temperature*TEMP_SCALE,
                    self.sim.max_v*LEN_SCALE/TIME_SCALE,
                    self.sim.min_dist*LEN_SCALE,
                    self.sim.v_com.scaled_copy(LEN_SCALE/TIME_SCALE),
            )
        } else {
            format!("time: {} ({})\n#Balls: {}\nPotEnergy: {:.2E}\nKinEnergy: {:.2E}\nTotalE: {:.3E}\nTemp: {:.2E}\nmaxV: {:.2}\nminDist: {:.2E}\nvCOM: {}", 
                self.sim.time,
                self.sim.speedup,
                self.num_balls,
                self.sim.energy_pot,
                self.sim.energy_kin,
                self.sim.energy_pot + self.sim.energy_kin,
                self.sim.temperature,
                self.sim.max_v,
                self.sim.min_dist,
                self.sim.v_com,
            )
        };
        text
    }

    fn draw_frame(&mut self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {

        let rect = Mesh::new_rectangle(
            ctx, 
            DrawMode::stroke(3.), 
            Rect::new(
                self.win_definition.margin_px_x,
                self.win_definition.margin_px_y,
                self.win_definition.region_px_x,
                self.win_definition.region_px_y
                ),
            Color::BLUE,
        )?;

        let txt_balls = Text::new( TextFragment::new(self.status_text()).scale(30.));
        let txt_balls_pos: [f32; 2] = match txt_balls.dimensions(ctx) {
            None => [0., 0.], 
            Some(rec) => [self.win_definition.margin_px_x*2.+self.win_definition.region_px_x-rec.w,0.],
        };

        canvas.draw(&rect, Vec2::new(0.,0.));
        if self.show_ball_stats {
            canvas.draw(&txt_balls, DrawParam::default().dest(txt_balls_pos));
        }
        Ok(())
    }

    fn len_to_px(coord_len: f32) -> f32{
        coord_len/LEN_TO_PX
    }
    fn px_to_len(coord_px: f32) -> f32{
        coord_px*LEN_TO_PX
    }
    fn px_to_coord(&self, px_x: f32, px_y: f32 ) -> Vec2D {
        //window top left (0,0) px
        //margin margin_px_[xy]
        //region region_px_[xy]
        //window bottom [xy] = region_px_[xy]+2*margin_px[xy]
        //pos = px_to_len(px_pos - (region_px_[xy]+2*margin_px_[xy])*0.5)
        Vec2D::new(Self::px_to_len(px_x-self.win_definition.region_px_x*0.5 -self.win_definition.margin_px_x), 
            Self::px_to_len(px_y-self.win_definition.region_px_y*0.5 - self.win_definition.margin_px_y) 
        )
    }
    fn coord_to_px(&self, coord: &Vec2D) -> Vec2D {
        Vec2D::new(Self::len_to_px(coord.x)+self.win_definition.region_px_x*0.5+self.win_definition.margin_px_x, 
            Self::len_to_px(coord.y) + self.win_definition.region_px_y*0.5+self.win_definition.margin_px_y)
    }

    fn resize(&mut self, width: f32, height: f32) -> Result<(), ggez::GameError> {
        self.win_definition.region_px_x = width-2.*self.win_definition.margin_px_x;
        self.win_definition.region_px_y = height-2.*self.win_definition.margin_px_y;
        Ok(())
    }
    
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let min_x = self.win_definition.margin_px_x + Self::len_to_px(pt.radius);
        let max_x = min_x + self.win_definition.region_px_x - 2.*Self::len_to_px(pt.radius);
        let min_y = self.win_definition.margin_px_y + Self::len_to_px(pt.radius);
        let max_y = min_y + self.win_definition.region_px_y - 2.*Self::len_to_px(pt.radius);

        // Check if the point is within the region in both x and y directions
        let pos_px = self.coord_to_px(&pt.pos);
        let within_x = pos_px.x >= min_x && pos_px.x <= max_x;
        let within_y = pos_px.y >= min_y && pos_px.y <= max_y;

        let within = within_x && within_y;

        within
    }
 
    fn is_ball_colliding(&self, pt: &Ball2D) -> bool {
        self.v_points.iter().
            map(|ball_rc| ball_rc.borrow()).
            any(|ball| pt.is_colliding(&(*ball)))
    }

    fn add_candidate_px(&mut self, x_px:f32, y_px:f32) -> GameResult {
        let ball_pos = self.px_to_coord(x_px, y_px);
        let candidate: Ball2D<'a> = Ball2D::new_massive(
            self.num_balls+1, 
            ball_pos.x,
            ball_pos.y, 
            self.new_ball.ball_size, 
            self.new_ball.ball_mass);

        if !self.is_ball_in_region(&candidate) {
            return Err(ggez::GameError::CustomError( format!("new ball not within region")));
        }
        if self.is_ball_colliding(&candidate) {
            return Err(ggez::GameError::CustomError( format!("new ball colliding with existing ball")));
        }
        let candidate_rc: Rc<RefCell<Ball2D<'a>>> = Rc::new(RefCell::new(candidate));
        self.v_points.push(candidate_rc);
        self.num_balls+=1;
        Ok(())
    }

    fn collect_neighbours(&mut self, dt: f32) {
        if self.step_count % 1 == 0 {
        //if self.step_count % 10 == 0 {
            //clear all neighbours
            for ball_rc in self.v_points.iter() {
                
                let mut ball = ball_rc.borrow_mut(); // Mutable borrow for updating the ball
                ball.clear_hood();
            }
            //collect neighbours
            for (i, ball_rc) in self.v_points.iter().enumerate() {

                let neighbors: Vec<Rc<RefCell<Ball2D>>> = self.v_points[i + 1..]
                    .iter()
                    .filter(|&other_rc| {
                        let ball_borrowed = ball_rc.borrow();
                        let other_borrowed = other_rc.borrow();
                        other_borrowed.has_larger_id(&ball_borrowed)
                    })
                    .cloned()
                    .collect();

                {
                    let mut ball = ball_rc.borrow_mut();
                    ball.update_hood(dt, ball_rc,  neighbors); // Updated to work with Rc<RefCell<Ball2D>>
                }
            }

            //update stats
            for ball_rc in self.v_points.iter() {
                
                let mut ball = ball_rc.borrow_mut(); // Mutable borrow for updating the ball
                ball.update_closeness_stats();
            }
        }
    }

    fn any_log(&self) -> bool {
        self.show_timings||self.show_ball_stats
    }

    fn print_help_message(&self){
        println!(
        "HELP: The following Keys are available:\n\
        H - Show this help message.\n\
        D - Toggle the display of real data.\n\
        S - Toggle step-by-step mode.\n\
        T - Toggle the display of timing information.\n\
        + - Increase the simulation speed.\n\
        - - Decrease the simulation speed (minimum speed is 1).\n\
        B - Toggle the display of ball statistics.\n\
        V - Set verbosity for ball statistics:\n\
            - Enter a specific ball ID to toggle verbose mode for that ball.\n\
            - Enter 'a' to set all balls to verbose.\n\
            - Enter '0' to disable verbose mode for all balls.\n\
        SPACE - Pause or resume the simulation."
    );
    }

    fn global_update(&mut self) -> GameResult {
        if self.sim.pause {
            return Ok(());
        }
        if self.any_log(){
            println!("-------------------------------------------------");
        }
        if self.step_by_step {
                self.sim.pause = true;
        }
        let bound_rect = Rect::new(
            self.win_definition.margin_px_x, 
            self.win_definition.margin_px_y, 
            self.win_definition.region_px_x,
            self.win_definition.region_px_y
        );

        let mut _min_dist = bound_rect.w + bound_rect.h;
        self.sim.min_dist = 0.;
        let mut max_v:f32 = 0.;
        let mut max_id: usize = 0;
        let mut pot: f32 = 0.;
        let mut kin: f32 = 0.;
        let mut _v_com:Vec2D = Vec2D::new(0.,0.);
        let mut sum_mass: f32 = 0.;

        let start_all = std::time::Instant::now();
        let dt = self.sim.speedup*DT_GOAL;

        self.collect_neighbours(dt);
        let dur = start_all.elapsed();
        let collecting_ms = dur.as_millis();

        let start = std::time::Instant::now();
        for ball_rc in &mut self.v_points {
            let mut cur = ball_rc.borrow_mut();

            let (min_dist, cur_pot) =cur.update(dt, &bound_rect);

            pot += cur_pot*0.5;//double counts
            _v_com.add(cur.moment());
            let speed_sq = cur.v.norm_sq();
            kin+= speed_sq*cur.mass*0.5;
            sum_mass+=cur.mass;
            if max_v*max_v<speed_sq {
                max_v = f32::sqrt(speed_sq);
                max_id = cur.get_id();
            }

            if _min_dist > min_dist && min_dist>0. {
                _min_dist = min_dist;
                self.sim.min_dist = _min_dist;

            }
        }
        let dur = start.elapsed();
        let updating_ms = dur.as_millis();

        self.v_points.retain(|cur| !(*cur.borrow()).pos.x.is_nan() && !(*cur.borrow()).pos.y.is_nan());
        
        self.num_balls = self.v_points.len();

        self.sim.energy_kin = kin;
        self.sim.energy_pot = pot;
        self.sim.max_v = max_v;
        self.sim.max_id = max_id;
        self.sim.time += dt;
        self.sim.v_com = _v_com.scaled_copy(1./sum_mass);

        let mut sum_rel_kin: f32 = 0.;
        let start = std::time::Instant::now();
        for ball_rc in &self.v_points {
            let cur = ball_rc.borrow();
            let v_abs = &cur.v;
            let v_rel_x = v_abs.x - self.sim.v_com.x;
            let v_rel_y = v_abs.y - self.sim.v_com.y;
            sum_rel_kin+= cur.mass*(v_rel_x*v_rel_x + v_rel_y*v_rel_y);
        }
        self.sim.temperature = 0.5/(KB*(3.*self.v_points.len() as f32 - 3.)) * sum_rel_kin;
        let dur = start.elapsed();
        let props_ms = dur.as_millis();
        let all_ms = start_all.elapsed().as_millis();

        if self.show_timings && all_ms > 1 {
            let text_timing = format!("coll/upd/prop/all: {}/{}/{}/{}",collecting_ms, updating_ms, props_ms, all_ms);
            println!("{:>5}",text_timing);
        }

        self.step_count += 1;
        Ok(())
    }
    
    fn handle_verbose_output_setting(&mut self, ctx: &mut ggez::Context) -> Result<(), ggez::GameError> {

        let mut text = String::new();


        // Step 1: Display instructions
        let instructions = "Enter a ball ID (or 'a' for all, '0' to disable verbosity):";
        let instructions_text = Text::new(instructions);
        

        // Step 2: Capture Input

        loop {
            let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(0,0,0)); 
            canvas.draw(&instructions_text, Vec2::new(50.0, 50.0 ));
            let input_display = Text::new(format!("Your input: {}", text)); 
            canvas.draw(&input_display, Vec2::new(50.0, 100.0));
            canvas.finish(ctx)?;
            

            for key in ctx.keyboard.pressed_keys() {
                match key {
                    KeyCode::Return => {
                        return self.process_input(&text)
                    },
                    KeyCode::Back  => {
                        text.pop();
                    },
                    KeyCode::A => {
                        text.push('a');
                    },
                    KeyCode::Key0| KeyCode::Key1 | KeyCode::Key2 | KeyCode::Key3 | KeyCode::Key4
                        |KeyCode::Key5 | KeyCode::Key6 | KeyCode::Key7 |KeyCode::Key8 | KeyCode::Key9 => {
                        if let Some(c) = Self::keycode_to_char(*key) {
                            text.push(c);
                        }
                    },
                    _ => {}
                }
            }
        }
    }

    fn process_input(&mut self, text: &str) -> ggez::GameResult<()> {
        // Process the user's input after pressing Enter
        if let Ok(number) = text.trim().parse::<usize>() {
            match number {
                0 => {
                    for b in self.v_points.iter() {
                        let mut ba = b.borrow_mut();
                        ba.verbose = false;
                    }
                }
                _ => {
                    if let Some(ball_rc) = self.v_points.iter().find(|&ball_rc_in| ball_rc_in.borrow().get_id() == number) {
                        let mut ball_found = ball_rc.borrow_mut();
                        ball_found.verbose = !ball_found.verbose;
                    }
                }
            }
        } else if text.trim() == "a" {
            for b in self.v_points.iter() {
                let mut ba = b.borrow_mut();
                ba.verbose = true;
            }
        }
        Ok(())
    } 

    /// Helper function to map KeyCode to a character.
    fn keycode_to_char(key: KeyCode) -> Option<char> {
        match key {
            KeyCode::Key0 => Some('0'),
            KeyCode::Key1 => Some('1'),
            KeyCode::Key2 => Some('2'),
            KeyCode::Key3 => Some('3'),
            KeyCode::Key4 => Some('4'),
            KeyCode::Key5 => Some('5'),
            KeyCode::Key6 => Some('6'),
            KeyCode::Key7 => Some('7'),
            KeyCode::Key8 => Some('8'),
            KeyCode::Key9 => Some('9'),
            KeyCode::A => Some('a'),
            _ => None, // Add more mappings if needed
        }
    }

}

impl<'a> EventHandler for MainContent<'a> {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {

        while _ctx.time.check_update_time(FPS_GOAL) {
            self.global_update()?;
        }
        Ok(())
    }

    fn resize_event(&mut self, _ctx: &mut Context, _width: f32, _height: f32) -> Result<(), ggez::GameError> {
        self.resize(_width,_height)?;
        Ok(())
    }

    fn mouse_motion_event(&mut self, _ctx: &mut Context, x: f32, y: f32, _dx: f32, _dy: f32) -> GameResult {
        let used_x = f32::max(self.win_definition.margin_px_x, f32::min(self.win_definition.margin_px_x+self.win_definition.region_px_x, x));
        let used_y = f32::max(self.win_definition.margin_px_y, f32::min(self.win_definition.margin_px_y+self.win_definition.region_px_y, y));
        self.win_definition.curs_pos = Vec2D::new(used_x, used_y);
        Ok(())
    }

    fn mouse_button_down_event(&mut self, _ctx: &mut Context, button: MouseButton, x: f32, y: f32) -> GameResult {
        match button {
            MouseButton::Left => {
                self.add_candidate_px(x,y)
                    .ok(); //ignore any errors because of not being able to add
            }
            MouseButton::Right => {
                self.new_ball.ball_size+=1.;
                if self.new_ball.ball_size > 5.5 {
                    self.new_ball.ball_size = DEFAULT_BALL_SIZE;
                }
            }
            _ => (),
        }
        Ok(())
    }


    fn key_up_event(&mut self, ctx: &mut Context, input: ggez::input::keyboard::KeyInput) -> Result<(), ggez::GameError> {
       match input.keycode {
           Some(key) => match key {
               KeyCode::H => {
                   self.print_help_message();
               },
               KeyCode::D => {
                   self.real_data = !self.real_data
               },
               KeyCode::S => {
                   self.step_by_step = !self.step_by_step
               },
               KeyCode::T => {
                   self.show_timings = !self.show_timings
               },
               KeyCode::Equals => {
                   self.sim.speedup += 1.
               },
               /*KeyCode::Plus => {
                   self.sim.speedup += 1.
               },*/
               KeyCode::Minus => {
                   if self.sim.speedup > 1.{
                       self.sim.speedup -= 1.;
                   } else {
                       self.sim.speedup = 1.;
                   }
               },
               KeyCode::B => {
                   self.show_ball_stats = !self.show_ball_stats
               },
               KeyCode::V => {
                   self.handle_verbose_output_setting(ctx)?;

                   //println!("Enter a ball ID (or 'a' to set all balls to verbose):");
                   //let mut input = String::new();
                   //if std::io::stdin().read_line(&mut input).is_ok() {
                   //    if let Ok(number) = input.trim().parse::<usize>() {
                   //        match number {
                   //            0 => {
                   //                for b in self.v_points.iter() {
                   //                    let mut ba = b.borrow_mut();
                   //                    ba.verbose = false;
                   //                }

                   //          },
                   //            _ => {
                   //                if let Some(ball_rc) = self.v_points.iter()
                   //                    .find(|&ball_rc_in| ball_rc_in.borrow().get_id() == number) 
                   //                {
                   //                    let mut ball_found = ball_rc.borrow_mut();
                   //                    ball_found.verbose = !ball_found.verbose;
                   //                }
                   //            }
                   //        }
                   //    } else if input.trim() == "a" {
                   //        for b in self.v_points.iter() {
                   //            let mut ba = b.borrow_mut();
                   //            ba.verbose = true;
                   //        }
                   //    }
                   //}
               },
               KeyCode::Space => {
                   self.sim.pause = !self.sim.pause
               },
               _ => {
               }
           }
           None => {
           }
       }

       Ok(()) 
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        // Get the window's width and height
        
        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(0,0,0)); 
        self.draw_balls(ctx, &mut canvas)?;
        self.draw_cursor(ctx, &mut canvas)?;
        self.draw_frame(ctx, &mut canvas)?;
        
        canvas.finish(ctx)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use float_cmp::approx_eq;

    #[test]
    fn test_defaults() {
        let default_win = WindowDefinition::default();
        assert_eq!(default_win.region_px_x, 100.);
        assert_eq!(default_win.region_px_y, 100.);
        assert_eq!(default_win.margin_px_x, 10.);
        assert_eq!(default_win.margin_px_y, 10.);

        let default_ball_def = NewBallDef::default();
        assert_eq!(default_ball_def.ball_size, DEFAULT_BALL_SIZE);
        assert_eq!(default_ball_def.ball_mass, 1.);

        let default_sim_stats = SimStats::default();
        assert_eq!(default_sim_stats.energy_pot, 0.);
        assert_eq!(default_sim_stats.energy_kin, 0.);
    }

    #[test]
    fn test_add_candidate_px() {
        let mut main_content = MainContent::default();

        // Add a valid candidate
        let x_px = main_content.win_definition.margin_px_x + MainContent::len_to_px(main_content.new_ball.ball_size*1.1);
        let y_px = main_content.win_definition.margin_px_y + MainContent::len_to_px(main_content.new_ball.ball_size*1.1);
        println!("test adding ball at px ({},{}) | LEN_TO_PX: {}", x_px, y_px, LEN_TO_PX);
        main_content.add_candidate_px(x_px, y_px).unwrap();

        assert_eq!(main_content.num_balls, 1);

        // Ensure the added ball is in the correct position
        {
            let ball = main_content.v_points[0].borrow();
            let ball_pos_px = main_content.coord_to_px(&ball.pos);
            assert!(approx_eq!(f32, ball_pos_px.x, x_px, ulps = 2), "values in x are not close: {} vs {}", ball_pos_px.x, x_px);
            assert!(approx_eq!(f32, ball_pos_px.y, y_px, ulps = 2), "values in y are not close: {} vs {}", ball_pos_px.y, y_px);
        }

        // Attempt to add a candidate out of region
        {
            let invalid_x_px = -10.0;
            let invalid_y_px = -10.0;
            let _ = main_content.add_candidate_px(invalid_x_px, invalid_y_px);
            assert_eq!(main_content.num_balls, 1, "Ball added out of valid region");
        }
    }

    #[test]
    fn test_is_ball_in_region() {
        let main_content = MainContent::default();
        let ball_px: Vec2D = Vec2D::new(30.,30.);
        let ball_coord=main_content.px_to_coord(ball_px.x, ball_px.y);
        println!("ball coordindates are {:?} from px ({},{})", ball_coord, ball_px.x, ball_px.y);
        let ball_radius = MainContent::px_to_len(10.);
        let ball = Ball2D::new_massive(1, ball_coord.x, ball_coord.y, ball_radius, 1.0);

        assert!(main_content.is_ball_in_region(&ball));

        // Move the ball outside the region
        let invalid_ball = Ball2D::new_massive(2, -10.0, -10.0, 5.0, 1.0);
        assert!(!main_content.is_ball_in_region(&invalid_ball));
    }

    #[test]
    fn test_global_update() {
        let mut main_content = MainContent::default();

        // Add some balls
        let ball_coord_px=Vec2D::new(30., 30.);
        let ball_radius_px = MainContent::len_to_px(main_content.new_ball.ball_size);
        main_content.add_candidate_px(ball_coord_px.x, ball_coord_px.y).unwrap();
        main_content.add_candidate_px(ball_coord_px.x+ball_radius_px*2.2, ball_coord_px.y+ball_radius_px*2.2).unwrap();
        main_content.global_update().unwrap();

        // Validate that the balls' velocities, positions, and energies are updated correctly
        assert!(main_content.sim.energy_kin > 0.0, "Kinetic energy should be positive");
        //assert!(main_content.sim.energy_pot >= 0.0, "Potential energy should be non-negative");
    }

    #[test]
    fn test_resize_event() {
        let mut main_content = MainContent::default();
        let new_width = 300.0;
        let new_height = 200.0;

        main_content.resize(new_width, new_height).unwrap();

        assert_eq!(main_content.win_definition.region_px_x, 300.0 - 2.0 * main_content.win_definition.margin_px_x);
        assert_eq!(main_content.win_definition.region_px_y, 200.0 - 2.0 * main_content.win_definition.margin_px_y);
    }

    #[test]
    fn test_ball_collision() {
        let mut main_content = MainContent::default();

        // Add two overlapping balls
        let ball_coord_px=Vec2D::new(30., 30.);
        let ball_radius_px = MainContent::len_to_px(main_content.new_ball.ball_size);
        let ball_radius = main_content.new_ball.ball_size;
        main_content.add_candidate_px(ball_coord_px.x, ball_coord_px.y).unwrap();

        let ball_coord=main_content.px_to_coord(ball_coord_px.x, ball_coord_px.y);
        let new_ball_coord = Vec2D::new(ball_coord.x+0.5*ball_radius, ball_coord.y+0.5*ball_radius);
        let colliding_ball = Ball2D::new_massive(2, new_ball_coord.x, new_ball_coord.y, ball_radius, 1.0);

        println!("existing ball {:?} and new ball {:?} both with radius {}", ball_coord_px, main_content.coord_to_px(&new_ball_coord), ball_radius_px);

        assert!(main_content.is_ball_colliding(&colliding_ball), "Collision not detected");
    }


    #[test]
    fn test_coord_trafo_idempotent() {
        let main_content = MainContent::new(500.,500., None).expect("could not create content");
        let test_coord = Vec2D::new(18., 234.); 
        let vec=main_content.coord_to_px(&test_coord);
        let res_coord=main_content.px_to_coord(vec.x,vec.y);

        assert!(approx_eq!(f32, test_coord.x, res_coord.x, ulps = 2), "values in x are not close: {} vs {}", test_coord.x, res_coord.x);
        assert!(approx_eq!(f32, test_coord.y, res_coord.y, ulps = 2), "values in y are not close: {} vs {}", test_coord.y, res_coord.y);
    }

    #[test]
    fn test_px_to_coord() {

        let main_content = MainContent::new(500.,300., None).expect("could not create content");


        println!("window size: {} {} margin {} {} and region {} {}",
            main_content.win_definition.margin_px_x*2.+main_content.win_definition.region_px_x,
            main_content.win_definition.margin_px_y*2.+main_content.win_definition.region_px_y,
            main_content.win_definition.margin_px_x,
            main_content.win_definition.margin_px_y,
            main_content.win_definition.region_px_x,
            main_content.win_definition.region_px_y,
            );
        {
        let x_px = 500.;
        let y_px = 300.;
        // 50 | 500 | 50 |   distribution of margin and region  vertical
        // 50 | 300 | 50 |   distribution of margin and region  horiconatl 
        //margin is 50 --> region is 400x400 px 
        let x_len = 20.; //(500-250-50)*0.1 = 200*0.1 = 20 
        let y_len = 10.; //(300-150-50)/0.1 = 100*0.1 = 10
        let coord = main_content.px_to_coord(x_px, y_px);
        assert!(approx_eq!(f32, coord.x, x_len, ulps = 2), "values in x are not close: {} vs {}", coord.x, x_len);
        assert!(approx_eq!(f32, coord.y, y_len, ulps = 2), "values in y are not close: {} vs {}", coord.y, y_len);
        }
    }

    #[test]
    fn test_collect_neighbours() {
        let mut main_content = MainContent::new(500.,500., None).expect("could not create content");

        // Add balls to the system
        let x1 = 80.0;
        let y1 = 80.0;
        let x2 = 105.0;
        let y2 = 105.0;
        let x3 = 200.0;
        let y3 = 200.0;

        let ball1_coord = main_content.px_to_coord(x1,y1);
        let ball2_coord = main_content.px_to_coord(x2,y2);
        let ball3_coord = main_content.px_to_coord(x3,y3);

        println!("b1 {:?} b2 {:?} b3 {:?}", ball1_coord, ball2_coord, ball3_coord);
        main_content.add_candidate_px(x1, y1).unwrap();
        main_content.add_candidate_px(x2, y2).unwrap();
        main_content.add_candidate_px(x3, y3).unwrap();


        // Initial state check
        {
            let ball1: &Ball2D = &*main_content.v_points[0].borrow();
            let ball2: &Ball2D = &*main_content.v_points[1].borrow();
            let ball3: &Ball2D = &*main_content.v_points[2].borrow();


            assert!(ball1.get_neighbour_names().is_empty(), "Ball 1 should have no neighbours initially");
            assert!(ball2.get_neighbour_names().is_empty(), "Ball 2 should have no neighbours initially");
            assert!(ball3.get_neighbour_names().is_empty(), "Ball 3 should have no neighbours initially");
        }

        // Call collect_neighbours with the appropriate timestep (let's assume step_count is divisible by 10)
        {
            main_content.step_count = 10;
            println!("--------------------------------");
            main_content.collect_neighbours(DT_GOAL);
            println!("--------------------------------");
            main_content.collect_neighbours(DT_GOAL);

            // After collecting neighbors, Ball1 and Ball2 should be neighbors
            let ball1: &Ball2D = &*main_content.v_points[0].borrow();
            let ball2: &Ball2D = &*main_content.v_points[1].borrow();
            let ball3: &Ball2D = &*main_content.v_points[2].borrow();

            println!("ball1.neigh: {:?}", ball1.get_neighbour_names());
            println!("ball2.neigh: {:?}", ball2.get_neighbour_names());
            println!("ball3.neigh: {:?}", ball3.get_neighbour_names());

            assert_eq!(ball1.get_neighbour_names().len(), 1, "Ball 1 should have 1 neighbour");
            assert_eq!(ball2.get_neighbour_names().len(), 1, "Ball 2 should have 1 neighbour");
            assert!(ball1.get_neighbour_names().contains(&format!("Ball2")), "Ball 1 should be neighbour of Ball 2");
            assert!(ball2.get_neighbour_names().contains(&format!("Ball1")), "Ball 2 should be neighbour of Ball 1");

            assert!(ball3.get_neighbour_names().is_empty(), "Ball 3 should have no neighbours because it is far from others");
        }
    }
}



