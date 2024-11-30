use crate::objects2d::{Vec2D, Ball2D};
use ggez::input::keyboard::KeyCode;
use ggez::{Context, GameResult };
use ggez::graphics::{Canvas, Color, DrawMode,DrawParam, Drawable, Mesh, Rect, Text,TextFragment};
use ggez::input::mouse::MouseButton;
use ggez::event::EventHandler;
use ggez::glam::Vec2;

pub const MARGIN_X: f32 = 50.;
pub const MARGIN_Y: f32 = 50.;
pub const KB: f32 = 1.380649E-23;
const U: f32 = 1.66E-27;
const MASS_SCALE: f32 = 39.948*U; //Ar 
const ENERGY_SCALE: f32 = 122.* KBOLTZMANN; //energy scale
const LEN_SCALE: f32 = 3.33729598E-9/6.;//f32::cbrt(6.631E-26/1.784)/6; //densit 1.784 kg/m³
const TIME_SCALE: f32 = MASS_SCALE*LEN_SCALE/ENERGY_SCALE;
const KBOLTZMANN: f32 = 1.38E-23;
const TEMP_SCALE: f32 = ENERGY_SCALE/KBOLTZMANN;

const FPS_GOAL: u32 = 60;
const DT_GOAL: f32 = 1./(FPS_GOAL as f32); 

struct WindowDefinition {
    curs_pos: Vec2D,
    region_size_x: f32,
    region_size_y: f32,
    margin_x: f32,
    margin_y: f32,
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
    min_dist: f32,
    v_com: Vec2D,
    time: f32,
    pause: bool,
}

pub struct MainContent {
    win_definition: WindowDefinition,
    new_ball: NewBallDef,
    sim: SimStats,
    num_balls: usize,
    v_points: Vec<Ball2D>,
    real_data: bool,
    step_by_step: bool,
    step_count: i32,
}

impl Default for WindowDefinition {
    fn default() -> WindowDefinition {
        WindowDefinition {
            curs_pos: Vec2D::new(0.,0.),
            region_size_x: 100.,
            region_size_y: 100.,
            margin_x: 10.,
            margin_y: 10.,
        }
    }
}
impl Default for NewBallDef {
    fn default() -> NewBallDef {
        NewBallDef {
            ball_size: 3.,
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
            min_dist: 0.,
            v_com: Vec2D::new(0.,0.),
            time: 0.,
            pause: false,
        }
    }
}

impl Default for MainContent {
    fn default() -> MainContent {
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
        }
    }
}

impl MainContent {
    pub fn new(reg_x:f32, reg_y: f32) -> GameResult<MainContent> {
        let s = MainContent {
            win_definition: WindowDefinition {
                region_size_x: reg_x,
                region_size_y: reg_y,
                margin_x: MARGIN_X,margin_y: MARGIN_Y,
                ..Default::default()
            },
            ..Default::default()
        };
        Ok(s)
    }

    fn draw_cursor(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        let cursor_circle = Mesh::new_circle(ctx,DrawMode::fill(),self.win_definition.curs_pos.clone(),
            self.new_ball.ball_size,
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

    fn status_text(&self) -> String {
        let text: String;
        if self.real_data {
            text = format!("time: {:.0}[ns]\n#Balls: {}\nPotEnergy: {:.2E}[J]\nKinEnergy: {:.2E}[J]\nTotalE: {:.3E}[J]\nTemp: {:.2E}[K]\nmaxV: {:.2}[m/s]\nminDist: {:.2E}[m]\nvCOM: {}[m/s]", 
                self.sim.time*TIME_SCALE*1.0E9,
                self.num_balls,
                self.sim.energy_pot*ENERGY_SCALE,
                self.sim.energy_kin*ENERGY_SCALE,
                (self.sim.energy_pot + self.sim.energy_kin)*ENERGY_SCALE,
                self.sim.temperature*TEMP_SCALE,
                self.sim.max_v*LEN_SCALE/TIME_SCALE,
                self.sim.min_dist*LEN_SCALE,
                Vec2D::copy_scaled(&self.sim.v_com,LEN_SCALE/TIME_SCALE),
                )
        } else {
            text = format!("time: {}\n#Balls: {}\nPotEnergy: {:.2E}\nKinEnergy: {:.2E}\nTotalE: {:.3E}\nTemp: {:.2E}\nmaxV: {:.2}\nminDist: {:.2E}\nvCOM: {}", 
                self.sim.time,
                self.num_balls,
                self.sim.energy_pot,
                self.sim.energy_kin,
                self.sim.energy_pot + self.sim.energy_kin,
                self.sim.temperature,
                self.sim.max_v,
                self.sim.min_dist,
                self.sim.v_com,
                )
        }
        text
    }

    fn draw_frame(&mut self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {

        let rect = Mesh::new_rectangle(ctx, DrawMode::stroke(3.), Rect::new(
                self.win_definition.margin_x,
                self.win_definition.margin_y,
                self.win_definition.region_size_x,
                self.win_definition.region_size_y
                ),Color::BLUE)?;

        //let x = 3.*self.win_definition.margin_x/2.+ self.win_definition.region_size_x;
        //let y = self.win_definition.margin_y/2.;
        //let sample_pt = Ball2D::new_massive("SampleBall".to_string(), x, y, self.new_ball.ball_size, self.new_ball.ball_mass);
        //let pt_sample = Mesh::new_circle(ctx,DrawMode::fill(),
        //    sample_pt.pos.clone(),
        //    sample_pt.radius,
        //    0.1,
        //    Color::YELLOW
        //)?;
        //canvas.draw(&pt_sample, Vec2::new(0.,0.));
        //
        let txt_balls = Text::new(
            TextFragment::new(self.status_text())
                .scale(60.)

                //.font("DejaVuSansMono")
            );
        let txt_balls_pos: [f32; 2] = match txt_balls.dimensions(ctx) {
            None => [0., 0.], 
            Some(rec) => [self.win_definition.margin_x*2.+self.win_definition.region_size_x-rec.w,0.],
        };

        canvas.draw(&rect, Vec2::new(0.,0.));
        canvas.draw(&txt_balls, DrawParam::default().dest(txt_balls_pos));
        Ok(())
    }

    fn resize(&mut self, width: f32, height: f32) -> Result<(), ggez::GameError> {
        self.win_definition.region_size_x = width-2.*self.win_definition.margin_x;
        self.win_definition.region_size_y = height-2.*self.win_definition.margin_y;
        Ok(())
    }
    
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let min_x = self.win_definition.margin_x + pt.radius;
        let max_x = min_x + self.win_definition.region_size_x - 2.*pt.radius;
        let min_y = self.win_definition.margin_y + pt.radius;
        let max_y = min_y + self.win_definition.region_size_y - 2.*pt.radius;

        // Check if the point is within the region in both x and y directions
        let within_x = pt.pos.x >= min_x && pt.pos.x <= max_x;
        let within_y = pt.pos.y >= min_y && pt.pos.y <= max_y;

        let within = within_x && within_y;

        within
    }

    fn is_ball_colliding(&self, pt: &Ball2D) -> bool {
        self.v_points.iter().any(|ball| pt.is_colliding(ball))
    }

    fn collect_neighbours(&mut self, dt: f32) {
        let mut other_balls: Vec<&mut Ball2D>  = self.v_points.iter_mut().collect();
        if (self.step_count % 10) == 0 {
            other_balls.iter_mut().for_each(|ball| ball.clear_hood());
            self.v_points.iter_mut().for_each(|ball| {
                other_balls.retain(|other| other.get_name() != ball.get_name());
                ball.update_hood(dt, other_balls);
            })
        }
    }

    fn global_update(&mut self) -> GameResult {
        if self.sim.pause {
            return Ok(());
        }
        println!("-------------------------------------------------");
        if self.step_by_step {
                self.sim.pause = true;
        }
        let bound_rect = Rect::new(self.win_definition.margin_x, self.win_definition.margin_y, self.win_definition.region_size_x, self.win_definition.region_size_y);

        let dt = DT_GOAL;
        let mut _min_dist = bound_rect.w + bound_rect.h;
        self.sim.min_dist = 0.;
        let mut max_v:f32 = 0.;
        let mut pot: f32 = 0.;
        let mut kin: f32 = 0.;
        let mut _v_com:Vec2D = Vec2D::new(0.,0.);
        let mut sum_mass: f32 = 0.;

        self.collect_neighbours(dt);


        for cur in &mut self.v_points {
            let (min_dist, cur_pot) =cur.update(dt, &bound_rect);

            pot += cur_pot*0.5;//double counts
            _v_com.add(cur.moment());
            let speed_sq = Vec2D::norm_sq(&cur.v);
            kin+= speed_sq*cur.mass*0.5;
            sum_mass+=cur.mass;
            if max_v*max_v<speed_sq {
                max_v = f32::sqrt(speed_sq);
            }
            if _min_dist > min_dist && min_dist>0. {
                _min_dist = min_dist;
                self.sim.min_dist = _min_dist;

            }
        }
        self.v_points.retain(|cur| {
            return !cur.pos.x.is_nan() && !cur.pos.y.is_nan() 
        });
        
        self.num_balls = self.v_points.len();

        self.sim.energy_kin = kin;
        self.sim.energy_pot = pot;
        self.sim.max_v = max_v;
        self.sim.time += DT_GOAL;
        self.sim.v_com = Vec2D::copy_scaled(&_v_com, 1./sum_mass);

        let mut sum_rel_kin: f32 = 0.;
        for cur in &self.v_points {
            let v_abs = &cur.v;
            let v_rel_x = v_abs.x - self.sim.v_com.x;
            let v_rel_y = v_abs.y - self.sim.v_com.y;
            sum_rel_kin+= cur.mass*(v_rel_x*v_rel_x + v_rel_y*v_rel_y);
        }
        self.sim.temperature = 0.5/(KB*(3.*self.v_points.len() as f32 - 3.)) * sum_rel_kin;
        self.step_count += 1;
        Ok(())
    }


}

impl EventHandler for MainContent {
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
        let used_x = f32::max(self.win_definition.margin_x, f32::min(self.win_definition.margin_x+self.win_definition.region_size_x, x));
        let used_y = f32::max(self.win_definition.margin_y, f32::min(self.win_definition.margin_y+self.win_definition.region_size_y, y));
        self.win_definition.curs_pos = Vec2D::new(used_x, used_y);
        Ok(())
    }

    fn mouse_button_down_event(&mut self, _ctx: &mut Context, button: MouseButton, x: f32, y: f32) -> GameResult {
        match button {
            MouseButton::Left => {

                let candidate = Ball2D::new_massive(format!("Ball{}",self.num_balls).to_string(), x,y, self.new_ball.ball_size, self.new_ball.ball_mass);
                if self.is_ball_in_region(&candidate) && !self.is_ball_colliding(&candidate) {
                    self.v_points.push(candidate);
                    self.num_balls+=1;
                }
            }
            MouseButton::Right => {
                self.new_ball.ball_size+=3.;
                if self.new_ball.ball_size > self.win_definition.margin_y/2. {
                    self.new_ball.ball_size = 3.;
                }
            }
            _ => (),
        }
        Ok(())
    }

    fn key_up_event(&mut self, _ctx: &mut Context, input: ggez::input::keyboard::KeyInput) -> Result<(), ggez::GameError> {
       match input.keycode {
           Some(key) => match key {
               KeyCode::D => {
                   self.real_data = !self.real_data
               },
               KeyCode::S => {
                   self.step_by_step = !self.step_by_step
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
        self.draw_frame(ctx, &mut canvas)?;
        self.draw_balls(ctx, &mut canvas)?;

        //let mouse_pos = ctx.mouse.position();
        //if 0.<mouse_pos.x && mouse_pos.x<self.win_definition.region_size_x+2.*self.win_definition.margin_x {
            self.draw_cursor(ctx, &mut canvas)?;
        //}
        

        


        canvas.finish(ctx)?;

        Ok(())
    }
}
