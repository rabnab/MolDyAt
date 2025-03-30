use crate::objects2d::{Ball2D, Vec2D, VerboseInfoMode};
use ggez::event::EventHandler;
use ggez::glam::Vec2;
use ggez::graphics::{
    Canvas, Color, DrawMode, DrawParam, Drawable, Mesh, MeshBuilder, Rect, Shader, Text,
    TextFragment,
};
use ggez::input::keyboard::{KeyCode, KeyInput};
use ggez::input::mouse::MouseButton;
use ggez::{Context, GameError, GameResult};
use rand::{Rng, rng};
use rand_distr::{Distribution, Normal};

use ggez::mint::Point2;
use std::cell::{Ref, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

pub const MARGIN_PX_X: f32 = 50.;
pub const MARGIN_PX_Y: f32 = 50.;
const _U_KG: f32 = 1.66E-27;
const _MASS_SCALE_KG: f32 = 39.948 * _U_KG; //6.6E-26kg mass of Ar
const ENERGY_SCALE_J: f32 = 122. * KBOLTZMANN_J_P_K; //energy scale for Ar at 122K
const LEN_SCALE_M: f32 = 3.33729598E-9 / 6.; // about 0.5nm reason -- f32::cbrt(6.631E-26/1.784)/6; //density 1.784 kg/m³
const TIME_SCALE: f32 = 3.48235E-11; //=f32::sqrt(MASS_SCALE_KG/ ENERGY_SCALE_J)*LEN_SCALE_M; //0.35ns
const TIME_SCALE_NS: f32 = 3.48235E-2; //=f32::sqrt(MASS_SCALE_KG/ ENERGY_SCALE_J)*LEN_SCALE_M; //0.35ns
const KBOLTZMANN_J_P_K: f32 = 1.380649E-23;
const TEMP_SCALE: f32 = 122.; //=ENERGY_SCALE_J / KBOLTZMANN_J_P_K; //exact 122K
const LEN_TO_PX: f32 = 1. / 10.; // 50 px are one LEN_SCALE (SIG is about 0.5 to 1 LEN_SCALE)

const MIN_BALL_SIZE: f32 = 0.15;
const FPS_GOAL: u32 = 60;
const DT_GOAL: f32 = 0.01; // 1. / (FPS_GOAL as f32);
const SHOW_FORCE: bool = false;
const RADIUS_COMPARISON_EPSILON: f32 = 1e-4; // Extracted constant for the comparison threshold
const CROSSHAIR_SIZE: f32 = 10.0;
const LINE_THICKNESS: f32 = 2.0;
const CROSSHAIR_COLOR: Color = Color::WHITE;

struct WindowDefinition {
    curs_pos: Vec2D,
    region_px_x: f32,
    region_px_y: f32,
    margin_px_x: f32,
    margin_px_y: f32,
}

#[derive(Default)]
struct NewBallDef {
    ball_size_idx: usize,
    ball_size: Vec<f32>,
    ball_mesh: Vec<Mesh>,
    rand_speed: bool,
}

impl NewBallDef {
    pub(crate) fn get_mesh(&self, _ctx: &Context, ball_radius: f32) -> Option<&Mesh> {
        let idx = self.find_ball_index_by_radius(ball_radius)?;
        self.ball_mesh.get(idx)
    }

    pub(crate) fn new(ctx: &Context) -> NewBallDef {
        const NUM_BALL_SIZES: usize = 5; // Number of ball sizes to create
        const CIRCLE_TOLERANCE: f32 = 0.1; // Tolerance for circle drawing
        const STROKE_THICKNESS: f32 = 2.0; // Border thickness for stroke drawing
        const BASE_EXPONENT: f32 = 2.0; // Base scaling exponent for sizes

        let ball_sizes: Vec<f32> = (0..NUM_BALL_SIZES)
            .map(|i| MIN_BALL_SIZE * BASE_EXPONENT.powi(i as i32))
            .collect();

        let ball_meshes: Vec<Mesh> = ball_sizes
            .iter()
            .map(|&radius| Self::create_ball_mesh(ctx, radius, CIRCLE_TOLERANCE, STROKE_THICKNESS))
            .collect();

        NewBallDef {
            ball_size: ball_sizes,
            ball_size_idx: 2,
            ball_mesh: ball_meshes,
            rand_speed: false,
        }
    }

    /// Helper function to create the mesh for a ball
    fn create_ball_mesh(
        ctx: &Context,
        ball_size: f32,
        tolerance: f32,
        stroke_thickness: f32,
    ) -> Mesh {
        let mut builder = MeshBuilder::new();

        // Inner filled circle
        builder
            .circle(
                DrawMode::fill(),
                Vec2D::new(0.0, 0.0),
                MainContent::len_to_px(ball_size),
                tolerance,
                Color::RED,
            )
            .unwrap();

        // Outer stroked circle
        builder
            .circle(
                DrawMode::stroke(stroke_thickness),
                Vec2D::new(0.0, 0.0),
                MainContent::len_to_px(Ball2D::equilibrium_of_radius(ball_size)),
                tolerance,
                Color::MAGENTA,
            )
            .unwrap();

        let ball_mesh_data = builder.build();
        Mesh::from_data(ctx, ball_mesh_data)
    }

    fn find_ball_index_by_radius(&self, target_radius: f32) -> Option<usize> {
        self.ball_size.iter().position(|&r| {
            let difference = (r - target_radius).abs(); // Introduced variable for readability
            difference < RADIUS_COMPARISON_EPSILON
        })
    }

    pub(crate) fn get_ball_size(&self) -> f32 {
        self.ball_size[self.ball_size_idx]
    }

    pub(crate) fn get_ball_mass(&self) -> f32 {
        let density = 2.;
        1. + density * (self.get_ball_size() / MIN_BALL_SIZE - 1.).powi(2)
    }

    pub(crate) fn next_ball_size(&mut self) {
        self.ball_size_idx = (self.ball_size_idx + 1) % self.ball_size.len();
    }
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
    step: usize,
    pause: bool,
    speedup: u32,
    draw_cursor: bool,
    frames_since_draw: u32,
    v_pot: VecDeque<f32>,
    v_kin: VecDeque<f32>,
    max_neighbours: usize,
    update_hood_cnt: usize,
}

#[derive(PartialEq)]
enum PlotContent {
    NONE,
    EKIN,
    EPOT,
    ETOT,
    ALL,
}
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
    show_plot: PlotContent,
    step_count: i32,
    crosshair_mesh: Option<Mesh>,
    shader: Option<Shader>,
}
/*
 * activate when needed
#[derive(AsStd140)]
struct ShaderUniforms {
    radius: f32,
    glow_reach: f32,
}
*/

trait DrawingStuff {
    fn draw_cursor(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
    fn draw_plot(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
    fn draw_text_on_ball(&self, ctx: &Context, canvas: &mut Canvas, ball: &Ball2D) -> GameResult;
    fn draw_simple_balls(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
    fn draw_ball_by_mesh(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        ball_cell: &Rc<RefCell<Ball2D>>,
    ) -> GameResult;
    fn draw_neighbour_lines(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError>;
    fn draw_single_ball(
        &self,
        circle: &Mesh,
        // scale_px: f32,
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError>;
    fn draw_opengl_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        shader_used: &Shader,
    ) -> GameResult;
    fn draw_balls(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
    fn draw_cross_hair(&self, canvas: &mut Canvas) -> GameResult<()>;
    fn draw_frame(&mut self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult;
    fn resize(&mut self, width: f32, height: f32) -> Result<(), GameError>;
}
trait LogStuff {
    fn any_log(&self) -> bool;
    fn print_help_message();

    fn status_text(&self, fps: u32) -> String;
}

trait GameLogic {
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool;
    fn is_ball_colliding(&self, pt: &Ball2D) -> bool;
    fn add_candidate_px(&mut self, x_px: f32, y_px: f32) -> GameResult;
    fn collect_neighbours(&mut self, dt: f32);
    fn remove_invalid_balls(&mut self);
    fn calculate_temperature(&mut self);
    fn simulation_step(&mut self) -> GameResult;
}
trait SimulationToggler {
    fn make_all_verbose(&mut self, mode: VerboseInfoMode);
    fn toggle_ball_verbose(&mut self, number: usize, mode: VerboseInfoMode);
    fn clear_verbose_balls(&mut self);
}

trait VerletIntegration {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect) -> GameResult;
}

impl Default for WindowDefinition {
    fn default() -> WindowDefinition {
        WindowDefinition {
            curs_pos: Vec2D::new(0., 0.),
            region_px_x: 100.,
            region_px_y: 100.,
            margin_px_x: 10.,
            margin_px_y: 10.,
        }
    }
}
impl Default for SimStats {
    fn default() -> SimStats {
        SimStats {
            energy_pot: 0.,
            energy_kin: 0.,
            temperature: 0.,
            max_v: 0.,
            max_id: 0,
            min_dist: 0.,
            v_com: Vec2D::new(0., 0.),
            time: 0.,
            step: 0,
            pause: false,
            speedup: 1,
            draw_cursor: false,
            frames_since_draw: 0,
            v_pot: VecDeque::with_capacity(100),
            v_kin: VecDeque::with_capacity(100),
            max_neighbours: 0,
            update_hood_cnt: 0,
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
            new_ball: NewBallDef::default(),
            num_balls: 0,
            v_points: Vec::new(),
            real_data: false,
            step_by_step: false,
            step_count: 0,
            show_timings: false,
            show_plot: PlotContent::NONE,
            show_ball_stats: false,
            shader: None,
            crosshair_mesh: None,
        }
    }
}

impl<'a> MainContent<'a> {
    pub fn new(
        ctx: &Context,
        reg_x: f32,
        reg_y: f32,
        shader_in: Option<Shader>,
    ) -> GameResult<MainContent<'a>> {
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
            new_ball: NewBallDef::new(ctx),
            crosshair_mesh: Self::create_crosshair(ctx),
            ..Default::default()
        })
    }

    fn create_crosshair(ctx: &Context) -> Option<Mesh> {
        let mut builder = MeshBuilder::new();

        // Draw horizontal and vertical lines
        Self::draw_line(
            &mut builder,
            Point2 {
                x: -CROSSHAIR_SIZE,
                y: 0.,
            },
            Point2 {
                x: CROSSHAIR_SIZE,
                y: 0.,
            },
        );
        Self::draw_line(
            &mut builder,
            Point2 {
                x: 0.,
                y: -CROSSHAIR_SIZE,
            },
            Point2 {
                x: 0.,
                y: CROSSHAIR_SIZE,
            },
        );

        // Build the mesh and draw on canvas
        let mesh = Mesh::from_data(ctx, builder.build());
        Some(mesh)
    }

    fn draw_line(
        builder: &mut MeshBuilder,
        start: Point2<f32>,
        end: Point2<f32>,
    ) -> &mut MeshBuilder {
        builder
            .line(&[start, end], LINE_THICKNESS, CROSSHAIR_COLOR)
            .unwrap()
    }
}

impl<'a> MainContent<'a> {
    fn toggle_pause(&mut self) {
        self.sim.pause = !self.sim.pause;
    }

    fn enable_drawing_cursor(&mut self, repeated: bool) {
        if !repeated {
            self.sim.draw_cursor = true;
        }
    }

    fn clear_points(&mut self) {
        self.v_points.clear();
    }

    fn enable_random_ball_speed(&mut self) {
        self.new_ball.rand_speed = true;
    }

    fn format_g(value: f32, precision: usize) -> String {
        if value.abs() >= 1e4 || value.abs() < 1e-3 && value != 0.0 {
            format!("{:.*e}", precision - 1, value) // Use scientific notation
        } else {
            format!("{:.*}", precision, value) // Use normal float notation
        }
    }
    fn len_to_px(coord_len: f32) -> f32 {
        coord_len / LEN_TO_PX
    }
    fn px_to_len(coord_px: f32) -> f32 {
        coord_px * LEN_TO_PX
    }
    fn px_to_coord(&self, px_x: f32, px_y: f32) -> Vec2D {
        //window top left (0,0) px
        //margin margin_px_[xy]
        //region region_px_[xy]
        //window bottom [xy] = region_px_[xy]+2*margin_px[xy]
        //pos = px_to_len(px_pos - (region_px_[xy]+2*margin_px_[xy])*0.5)
        Vec2D::new(
            Self::px_to_len(
                px_x - self.win_definition.region_px_x * 0.5 - self.win_definition.margin_px_x,
            ),
            Self::px_to_len(
                px_y - self.win_definition.region_px_y * 0.5 - self.win_definition.margin_px_y,
            ),
        )
    }
    fn coord_to_px(&self, coord: &Vec2D) -> Vec2D {
        Vec2D::new(
            Self::len_to_px(coord.x)
                + self.win_definition.region_px_x * 0.5
                + self.win_definition.margin_px_x,
            Self::len_to_px(coord.y)
                + self.win_definition.region_px_y * 0.5
                + self.win_definition.margin_px_y,
        )
    }

    fn elapsed_since(start: Instant) -> u128 {
        let dur = start.elapsed();
        let updating_ms = dur.as_millis();
        updating_ms
    }

    fn start_handling_verbosity_input(&mut self) {
        println!("Enter a ball ID (or 'a' to set all balls to verbose) followed by mode ([cmos]):");
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let input_trimed = input.trim();
            let mut mode = VerboseInfoMode::MAIN;
            let mut particle_indicator = input_trimed;
            if input_trimed.len() >= 2 {
                match input_trimed.chars().last() {
                    Some('c') => mode = VerboseInfoMode::CONSOLE,
                    Some('m') => mode = VerboseInfoMode::MAIN,
                    Some('o') => mode = VerboseInfoMode::OTHER,
                    Some('s') => mode = VerboseInfoMode::SPEED,
                    _ => {}
                }
                (particle_indicator, _) = input_trimed.split_at(input_trimed.len() - 1).into();
            }

            if let Ok(number) = particle_indicator.parse::<usize>() {
                match number {
                    0 => {
                        self.clear_verbose_balls();
                    }
                    _ => {
                        self.toggle_ball_verbose(number, mode);
                    }
                }
            } else if particle_indicator.trim() == "a" {
                self.make_all_verbose(mode);
            }
        }
    }

    fn randomize_speeds(&mut self) {
        let normal = self.calculate_normal_distribution_of_speed();
        let mut rng = rng();

        self.v_points
            .iter()
            .map(|ball| ball.borrow_mut())
            .for_each(|mut ball| {
                ball.v.x = normal.sample(&mut rng);
                ball.v.y = normal.sample(&mut rng);
            })
    }

    fn calculate_normal_distribution_of_speed(&mut self) -> Normal<f32> {
        let (avg_vx, avg_vy) = self
            .v_points
            .iter()
            .map(|ball| ball.borrow())
            .fold((0., 0.), |(v_x, v_y), b| {
                (v_x + b.v.x.powi(2), v_y + b.v.y.powi(2))
            });
        let len = self.v_points.len() as f32;
        let sig = f32::sqrt(f32::sqrt(avg_vx / (len - 1.5) * avg_vy / (len - 1.5)));

        let normal = Normal::new(0., f32::max(0.3, sig)).expect("Invalid distribution parameters");
        println!("new normal distri {:?}", normal);
        normal
    }

    fn toggle_ball_internal(ball_found: &mut Ball2D<'a>, mode: &VerboseInfoMode) {
        match mode {
            VerboseInfoMode::CONSOLE => ball_found.verbose.toggle_console(),
            VerboseInfoMode::MAIN => ball_found.verbose.toggle_main_neighbour(),
            VerboseInfoMode::OTHER => ball_found.verbose.toggle_other_neighbour(),
            VerboseInfoMode::SPEED => ball_found.verbose.toggle_speed(),
        }
    }

    fn calculate_plot_boundary<F>(&self, initial: f32, operation: F) -> f32
    where
        F: Fn(f32, f32) -> f32,
    {
        let val: f32 = match self.show_plot {
            PlotContent::NONE => f32::NAN,
            PlotContent::EKIN => self.sim.v_kin.iter().cloned().fold(initial, operation),
            PlotContent::EPOT => self.sim.v_pot.iter().cloned().fold(initial, operation),
            PlotContent::ETOT => self
                .sim
                .v_kin
                .iter()
                .zip(self.sim.v_pot.iter())
                .map(|(kin, pot)| kin + pot)
                .fold(initial, operation),
            PlotContent::ALL => self
                .sim
                .v_kin
                .iter()
                .chain(self.sim.v_pot.iter())
                .cloned()
                .fold(initial, operation),
        };
        val
    }
}

impl DrawingStuff for MainContent<'_> {
    fn draw_cursor(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        if self.sim.draw_cursor {
            let ball_mesh = self.new_ball.get_mesh(ctx, self.new_ball.get_ball_size());
            canvas.draw(
                ball_mesh.unwrap(),
                DrawParam::default()
                    .color(Color::WHITE)
                    .dest(self.win_definition.curs_pos.clone()),
            );
        } else {
            self.draw_cross_hair(canvas)?;
        }
        let curs_pos = self.px_to_coord(
            self.win_definition.curs_pos.x,
            self.win_definition.curs_pos.y,
        );

        let txt_cursor = Text::new(
            TextFragment::new(format!(
                "x: {}, y: {} pos ({:.3},{:.3})",
                self.win_definition.curs_pos.x,
                self.win_definition.curs_pos.y,
                curs_pos.x,
                curs_pos.y,
            ))
            .scale(20.),
        );
        let txt_cursor_pos: [f32; 2] = match txt_cursor.dimensions(ctx) {
            None => [0., 0.],
            Some(rec) => [
                self.win_definition.margin_px_x * 2. + self.win_definition.region_px_x - rec.w,
                self.win_definition.margin_px_y * 2. + self.win_definition.region_px_y - rec.h,
            ],
        };

        canvas.draw(&txt_cursor, DrawParam::default().dest(txt_cursor_pos));

        Ok(())
    }

    fn draw_plot(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        if self.show_plot == PlotContent::NONE {
            return Ok(());
        }
        let max_len = self.sim.v_kin.len().max(self.sim.v_pot.len());
        if max_len == 0 {
            return Ok(()); // No data to plot
        }

        const PLOT_WIDTH: f32 = 2. * 200.0;
        const PLOT_HEIGHT: f32 = 2. * 166.0;
        const PLOT_MARGIN: f32 = MARGIN_PX_Y;
        let screen_rect =
            canvas
                .screen_coordinates()
                .unwrap_or(Rect::new(0.0, 0.0, PLOT_WIDTH, PLOT_HEIGHT));
        let screen_width = screen_rect.w;
        let screen_height = screen_rect.h;

        let plot_x = screen_width - PLOT_WIDTH - PLOT_MARGIN;
        let plot_y = screen_height - PLOT_HEIGHT - PLOT_MARGIN;

        // Determine the min/max range for scaling
        let min_val = self.calculate_plot_boundary(f32::INFINITY, f32::min);
        let max_val = self.calculate_plot_boundary(f32::NEG_INFINITY, f32::max);

        let value_range = (max_val - min_val).max(1.0); // Avoid division by zero

        let mut mesh_builder = MeshBuilder::new();

        // Helper to convert (index, value) to screen coordinates
        let to_screen_coords = |index: usize, value: f32| -> (f32, f32) {
            let x = plot_x + (index as f32 / max_len as f32) * PLOT_WIDTH;
            let y = plot_y + PLOT_HEIGHT - ((value - min_val) / value_range * PLOT_HEIGHT);
            (x, y)
        };

        // Draw e_kin line (red)
        if self.show_plot == PlotContent::ALL || self.show_plot == PlotContent::EKIN {
            for i in 1..self.sim.v_kin.len() {
                let (x1, y1) = to_screen_coords(i - 1, self.sim.v_kin[i - 1]);
                let (x2, y2) = to_screen_coords(i, self.sim.v_kin[i]);

                mesh_builder.line(
                    &[Point2 { x: x1, y: y1 }, Point2 { x: x2, y: y2 }],
                    2.0,
                    Color::RED,
                )?;
            }
        }

        // Draw e_pot line (blue)
        if self.show_plot == PlotContent::ALL || self.show_plot == PlotContent::EPOT {
            for i in 1..self.sim.v_pot.len() {
                let (x1, y1) = to_screen_coords(i - 1, self.sim.v_pot[i - 1]);
                let (x2, y2) = to_screen_coords(i, self.sim.v_pot[i]);
                mesh_builder.line(
                    &[Point2 { x: x1, y: y1 }, Point2 { x: x2, y: y2 }],
                    2.0,
                    Color::BLUE,
                )?;
            }
        }

        // Draw e_tot line (Orange)
        if self.show_plot == PlotContent::ALL || self.show_plot == PlotContent::ETOT {
            for i in 1..self.sim.v_pot.len() {
                let (x1, y1) =
                    to_screen_coords(i - 1, self.sim.v_pot[i - 1] + self.sim.v_kin[i - 1]);
                let (x2, y2) = to_screen_coords(i, self.sim.v_pot[i] + self.sim.v_kin[i]);
                mesh_builder.line(
                    &[Point2 { x: x1, y: y1 }, Point2 { x: x2, y: y2 }],
                    2.0,
                    Color::YELLOW,
                )?;
            }
        }

        // Draw bounding box
        mesh_builder.rectangle(
            DrawMode::stroke(2.0),
            Rect::new(plot_x, plot_y, PLOT_WIDTH, PLOT_HEIGHT),
            Color::WHITE,
        )?;

        let mesh = Mesh::from_data(ctx, mesh_builder.build());
        canvas.draw(&mesh, DrawParam::default());

        let max_value_text =
            Text::new(TextFragment::new(format!("{:.2e}", max_val)).color(Color::WHITE));
        let text_position = Point2 {
            x: plot_x + 5.0, // Small margin inside the plot
            y: plot_y + 5.0,
        };
        canvas.draw(&max_value_text, text_position);

        let min_value_text =
            Text::new(TextFragment::new(format!("{:.2e}", min_val)).color(Color::WHITE));
        let text_position = Point2 {
            x: plot_x + 5.0,                      // Small margin inside the plot
            y: plot_y + PLOT_HEIGHT - 5.0 - 10.0, /*font height */
        };
        canvas.draw(&min_value_text, text_position);

        Ok(())
    }

    fn draw_text_on_ball(&self, ctx: &Context, canvas: &mut Canvas, ball: &Ball2D) -> GameResult {
        let txt_name = Text::new(TextFragment::new(format!("{}", ball.get_id())).scale(15.));

        let ball_pos_px = self.coord_to_px(&ball.pos);

        let txt_name_pos: [f32; 2] = match txt_name.dimensions(ctx) {
            None => [0., 0.],
            Some(rec) => [ball_pos_px.x - rec.w * 0.5, ball_pos_px.y - rec.h * 0.5],
        };

        canvas.draw(&txt_name, DrawParam::default().dest(txt_name_pos));
        Ok(())
    }

    fn draw_simple_balls(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        for ball_cell in &self.v_points {
            self.draw_ball_by_mesh(ctx, canvas, ball_cell)?;
        }
        Ok(())
    }

    fn draw_ball_by_mesh(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        ball_cell: &Rc<RefCell<Ball2D>>,
    ) -> GameResult {
        let ball = ball_cell.borrow();
        let ball_coord_px = self.coord_to_px(&ball.pos);

        if let Some(cur_mesh) = self.new_ball.get_mesh(ctx, ball.radius) {
            self.draw_single_ball(cur_mesh, canvas, &ball, &ball_coord_px)?;
        }

        self.draw_text_on_ball(ctx, canvas, &ball)?;
        Ok(())
    }

    fn draw_neighbour_lines(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError> {
        let neighbour_info = ball.get_neighbour_force_and_pos();
        let start: Point2<f32> = ball_coord_px.clone().into();
        let max_force = neighbour_info
            .iter()
            .map(|info| info.1)
            .into_iter()
            .reduce(f32::max)
            .unwrap_or(0.);
        for (other_pos, other_force_ref) in neighbour_info.iter() {
            let other_force = *other_force_ref;
            let is_main = other_force > 0.5 * max_force;
            if ball.verbose.is_main() && !is_main {
                continue;
            }
            let color_line = if other_force > 0. {
                if is_main {
                    Color::CYAN
                } else {
                    Color::from_rgb(128, 128, 128)
                }
            } else {
                Color::from_rgb(64, 64, 64)
            };
            let end: Point2<f32> = self.coord_to_px(&other_pos).into();
            // Create a line mesh
            let line = Mesh::new_line(
                ctx,
                &[start, end], // Line points
                2.0,           // Line thickness
                color_line,    // Line color
            )?;
            // Draw the line
            canvas.draw(&line, DrawParam::default());
            if SHOW_FORCE {
                // Calculate the midpoint of the line
                let mid_x = (start.x + end.x) / 2.0;
                let mid_y = (start.y + end.y) / 2.0;

                let text_force = Self::format_g(other_force, 3);
                // Create the text
                let text = Text::new(TextFragment::new(text_force).color(Color::YELLOW));

                // Calculate centered position for the text
                let text_pos: [f32; 2] = match text.dimensions(ctx) {
                    None => [0., 0.],
                    Some(rec) => {
                        let pos = [mid_x - (rec.w / 2.0), mid_y - (rec.h / 2.0)];

                        let rectangle = Mesh::new_rectangle(
                            ctx,
                            DrawMode::fill(),
                            rec,
                            Color::from_rgb(64, 64, 64),
                        )?;
                        canvas.draw(&rectangle, DrawParam::default().dest(pos));
                        pos
                    }
                };

                // Draw the text
                canvas.draw(&text, DrawParam::default().dest(text_pos));
            }
        }
        Ok(())
    }

    fn draw_single_ball(
        &self,
        pt_circle: &Mesh,
        // std_size_px: f32,
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError> {
        let draw_par = DrawParam::default().dest(ball_coord_px.clone());

        if ball.get_id() == self.sim.max_id {
            draw_par.color(Color::CYAN);
        }
        // let ball_scale_px = Self::len_to_px(ball.radius)/std_size_px;
        //println!("drawing ball at {:?} of size {}", ball_coord_px, ball_size_px);
        //draw the ball itself
        // let pt_circle = Mesh::new_circle(
        //     ctx,
        //     DrawMode::fill(),
        //     ball_coord_px.clone(),
        //     ball_size_px,
        //     0.1,
        //     ball_color_used,
        // )?;
        canvas.draw(pt_circle, draw_par);
        //.scale(Vec2::new(ball_scale_px,ball_scale_px))

        Ok(())
    }

    fn draw_opengl_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        shader_used: &Shader,
    ) -> GameResult {
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
            canvas.draw(&pt_influence, Vec2::new(0., 0.));
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
        let verbose_balls: Vec<Rc<RefCell<Ball2D>>> = self
            .v_points
            .iter()
            .filter(|any| any.borrow().verbose.lines())
            .cloned()
            .collect();

        for ball_ref in verbose_balls.iter() {
            let ball = ball_ref.borrow();
            let ball_coord_px = self.coord_to_px(&ball.pos);
            self.draw_neighbour_lines(ctx, canvas, &ball, &ball_coord_px)?;
        }

        Ok(())
    }

    fn draw_cross_hair(&self, canvas: &mut Canvas) -> GameResult<()> {
        canvas.draw(
            self.crosshair_mesh.as_ref().unwrap(),
            self.win_definition.curs_pos.clone(),
        );
        Ok(())
    }

    fn draw_frame(&mut self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        let rect = Mesh::new_rectangle(
            ctx,
            DrawMode::stroke(3.),
            Rect::new(
                self.win_definition.margin_px_x,
                self.win_definition.margin_px_y,
                self.win_definition.region_px_x,
                self.win_definition.region_px_y,
            ),
            Color::BLUE,
        )?;

        canvas.draw(&rect, Vec2::new(0., 0.));
        if self.show_ball_stats {
            let fps = ctx.time.fps();
            let txt_balls = Text::new(TextFragment::new(self.status_text(fps as u32)).scale(30.));
            let txt_balls_pos: [f32; 2] = match txt_balls.dimensions(ctx) {
                None => [0., 0.],
                Some(rec) => [
                    self.win_definition.margin_px_x * 2. + self.win_definition.region_px_x - rec.w,
                    0.,
                ],
            };
            canvas.draw(&txt_balls, DrawParam::default().dest(txt_balls_pos));
        }
        Ok(())
    }

    fn resize(&mut self, width: f32, height: f32) -> Result<(), GameError> {
        self.win_definition.region_px_x = width - 2. * self.win_definition.margin_px_x;
        self.win_definition.region_px_y = height - 2. * self.win_definition.margin_px_y;
        Ok(())
    }
}

impl LogStuff for MainContent<'_> {
    fn any_log(&self) -> bool {
        self.show_timings //|| self.show_ball_stats
    }

    fn print_help_message() {
        const HELP_KEYS_INFO: &str = "HELP: The following Keys are available:";
        const HELP_OPTION_H: &str = "H - Show this help message.";
        const HELP_OPTION_D: &str = "D - Toggle the display of real data.";
        const HELP_OPTION_S: &str = "S - Toggle step-by-step mode.";
        const HELP_OPTION_T: &str = "T - Toggle the display of timing information.";
        const HELP_OPTION_INCREASE_SPEED: &str = "+ - Increase the simulation speed.";
        const HELP_OPTION_DECREASE_SPEED: &str =
            "- - Decrease the simulation speed (minimum speed is 1).";
        const HELP_OPTION_B: &str = "B - Toggle the display of ball statistics.";
        const HELP_OPTION_V: &str = "V - Set verbosity for ball statistics:\n\
            - Enter a specific ball ID to toggle verbose mode for that ball.\n\
            - Enter 'a' to set all balls to verbose.\n\
            - Enter '0' to disable verbose mode for all balls.";
        const HELP_OPTION_SPACE: &str = "SPACE - Pause or resume the simulation.";

        println!(
            "{}",
            format!(
                "{header}\n\
             {opt_h}\n\
             {opt_d}\n\
             {opt_s}\n\
             {opt_t}\n\
             {opt_plus}\n\
             {opt_minus}\n\
             {opt_b}\n\
             {opt_v}\n\
             {opt_space}",
                header = HELP_KEYS_INFO,
                opt_h = HELP_OPTION_H,
                opt_d = HELP_OPTION_D,
                opt_s = HELP_OPTION_S,
                opt_t = HELP_OPTION_T,
                opt_plus = HELP_OPTION_INCREASE_SPEED,
                opt_minus = HELP_OPTION_DECREASE_SPEED,
                opt_b = HELP_OPTION_B,
                opt_v = HELP_OPTION_V,
                opt_space = HELP_OPTION_SPACE,
            )
        );
    }

    fn status_text(&self, fps: u32) -> String {
        let text = if self.real_data {
            format!(
                "fps {} time: {:.3}[ns] ({})\n#Balls: {} #Frms: {}\nmaxNeig: {}\nPotEnergy: \
                {:.2E}[J]\nKinEnergy: {:.2E}[J]\nTotalE: {:.3E}[J]\nTemp: {:.2}[K]\nmaxV: {:.2}[m/s]\nminDist: {:.2E}[m]\nvCOM: {}[m/s]",
                fps,
                self.sim.time * TIME_SCALE_NS,
                self.sim.speedup,
                self.num_balls,
                self.sim.frames_since_draw,
                self.sim.max_neighbours,
                self.sim.energy_pot * ENERGY_SCALE_J,
                self.sim.energy_kin * ENERGY_SCALE_J,
                (self.sim.energy_pot + self.sim.energy_kin) * ENERGY_SCALE_J,
                self.sim.temperature * TEMP_SCALE,
                self.sim.max_v * LEN_SCALE_M / TIME_SCALE,
                self.sim.min_dist * LEN_SCALE_M,
                self.sim.v_com.scaled(LEN_SCALE_M / TIME_SCALE),
            )
        } else {
            format!(
                "fps {} time: {:.3} ({})\n#Balls: {} #hoodCnt: {}/{}\nmaxNeig: {}\nPotEnergy: {:.2E}\nKinEnergy: {:.2E}\nTotalE: {:.3E}\nTemp: {:.3}\nmaxV: {:.2}\nminDist: {:.2E}\nvCOM: {}",
                fps,
                self.sim.time,
                self.sim.speedup,
                self.num_balls,
                self.sim.update_hood_cnt,
                Ball2D::get_update_hood_call_cnt(),
                self.sim.max_neighbours,
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
}

impl<'a> GameLogic for MainContent<'a> {
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let min_x = self.win_definition.margin_px_x + Self::len_to_px(pt.radius);
        let max_x = min_x + self.win_definition.region_px_x - 2. * Self::len_to_px(pt.radius);
        let min_y = self.win_definition.margin_px_y + Self::len_to_px(pt.radius);
        let max_y = min_y + self.win_definition.region_px_y - 2. * Self::len_to_px(pt.radius);

        // Check if the point is within the region in both x and y directions
        let pos_px = self.coord_to_px(&pt.pos);
        let within_x = pos_px.x >= min_x && pos_px.x <= max_x;
        let within_y = pos_px.y >= min_y && pos_px.y <= max_y;

        let within = within_x && within_y;

        within
    }

    fn is_ball_colliding(&self, pt: &Ball2D) -> bool {
        self.v_points
            .iter()
            .map(|ball_rc| ball_rc.borrow())
            .any(|ball| pt.is_colliding(&(*ball)))
    }

    fn add_candidate_px(&mut self, x_px: f32, y_px: f32) -> GameResult {
        let ball_pos = self.px_to_coord(x_px, y_px);

        let ball_v = if self.new_ball.rand_speed {
            let mut rng = rng();
            let normal = self.calculate_normal_distribution_of_speed();
            Vec2D::new(normal.sample(&mut rng), normal.sample(&mut rng))
        } else {
            Vec2D::new(0., 0.)
        };
        let candidate: Ball2D<'a> = Ball2D::new_massive_with_v(
            self.num_balls + 1,
            ball_pos.x,
            ball_pos.y,
            self.new_ball.get_ball_size(),
            self.new_ball.get_ball_mass(),
            Some(ball_v),
        );

        if !self.is_ball_in_region(&candidate) {
            return Err(GameError::CustomError(
                "new ball not within region".to_string(),
            ));
        }
        if self.is_ball_colliding(&candidate) {
            return Err(GameError::CustomError(
                "new ball colliding with existing ball".to_string(),
            ));
        }
        let candidate_rc: Rc<RefCell<Ball2D<'a>>> = Rc::new(RefCell::new(candidate));
        self.v_points.push(candidate_rc);
        self.num_balls += 1;
        self.sim.v_kin.clear();
        self.sim.v_pot.clear();
        Ok(())
    }

    fn collect_neighbours(&mut self, dt: f32) {
        if self.step_count % 1 == 0 {
            let others_base = self.v_points.clone();

            //collect neighbours
            for ball_rc in self.v_points.iter() {
                let mut others = others_base.clone();
                {
                    let ball = ball_rc.borrow();
                    others.retain(|rc| rc.borrow().get_id() != ball.id);
                }

                let mut ball = ball_rc.borrow_mut();
                ball.update_hood(dt, ball_rc, others); // Updated to work with Rc<RefCell<Ball2D>>
            }

            //update stats
            for ball_rc in self.v_points.iter() {
                let mut ball = ball_rc.borrow_mut(); // Mutable borrow for updating the ball
                ball.update_closeness_stats();
            }
        }
    }

    fn remove_invalid_balls(&mut self) {
        self.v_points.retain(|cur| {
            let cur_borrow = cur.borrow();
            let keep = !cur_borrow.pos.x.is_nan() && !cur_borrow.pos.y.is_nan();
            if !keep {
                println!("ball {:?} became invalid. removing", cur_borrow);
            }
            keep
        });

        self.num_balls = self.v_points.len();
    }

    fn calculate_temperature(&mut self) {
        let mut sum_rel_kin_doubled: f32 = 0.;
        let mut max_neighbours: usize = 0;

        for ball_rc in &self.v_points {
            let cur = ball_rc.borrow();
            let v_abs = &cur.v;
            let v_rel_x = v_abs.x - self.sim.v_com.x;
            let v_rel_y = v_abs.y - self.sim.v_com.y;
            sum_rel_kin_doubled += cur.mass * (v_rel_x * v_rel_x + v_rel_y * v_rel_y);

            let cur_num_neighbour = cur.hood.get_neighbour_names().len();
            max_neighbours = max_neighbours.max(cur_num_neighbour);
        }
        self.sim.max_neighbours = max_neighbours;
        //self.sim.temperature = 0.5 / (/*KB*/ 1. * (3. * self.v_points.len() as f32 - 3.)) * sum_rel_kin;
        self.sim.temperature = sum_rel_kin_doubled / 3. / (self.v_points.len() as f32) /* /KB == /1*/
    }

    // fn global_update_euler(&mut self, dt: f32, bound_rect: &Rect) -> GameResult {
    //     let mut _min_dist = bound_rect.w + bound_rect.h;
    //     let mut max_v: f32 = 0.;
    //     let mut max_id: usize = 0;
    //     let mut pot: f32 = 0.;
    //     let mut kin: f32 = 0.;
    //     let mut _v_com: Vec2D = Vec2D::new(0., 0.);
    //     let mut sum_mass: f32 = 0.;
    //
    //     for ball_rc in &mut self.v_points {
    //         let mut cur = ball_rc.borrow_mut();
    //
    //         let (min_dist, cur_pot) = cur.update_positions(dt, &bound_rect);
    //
    //         pot += cur_pot * 0.5; //double counts
    //         _v_com.add(cur.moment());
    //         let speed_sq = cur.v.norm_sq();
    //         kin += speed_sq * cur.mass * 0.5;
    //         sum_mass += cur.mass;
    //         if max_v * max_v < speed_sq {
    //             max_v = f32::sqrt(speed_sq);
    //             max_id = cur.get_id();
    //         }
    //
    //         if _min_dist > min_dist && min_dist > 0. {
    //             _min_dist = min_dist;
    //             self.sim.min_dist = _min_dist;
    //         }
    //     }
    //     self.v_points.iter().for_each(|ball| { ball.borrow_mut().update_old_pos()});
    //
    //     self.sim.energy_kin = kin;
    //     self.sim.energy_pot = pot;
    //     self.sim.max_v = max_v;
    //     self.sim.max_id = max_id;
    //     self.sim.time += dt;
    //     self.sim.v_com = _v_com.scaled_copy(1. / sum_mass);
    //
    //     Ok(())
    // }

    fn simulation_step(&mut self) -> GameResult {
        if self.sim.pause {
            return Ok(());
        }
        if self.any_log() {
            println!("-------------------------------------------------");
        }
        if self.step_by_step {
            self.sim.pause = true;
        }
        let top_left = self.px_to_coord(
            self.win_definition.margin_px_x,
            self.win_definition.margin_px_y,
        );

        let bound_rect = Rect::new(
            top_left.x,
            top_left.y,
            Self::px_to_len(self.win_definition.region_px_x),
            Self::px_to_len(self.win_definition.region_px_y),
        );

        self.sim.min_dist = 0.;
        //let dt = self.sim.speedup * DT_GOAL;
        let dt = DT_GOAL;
        let start_all = Instant::now();
        self.collect_neighbours(dt);
        let collecting_ms = Self::elapsed_since(start_all);

        let start = Instant::now();
        self.update_sim(dt, &bound_rect)?;
        self.remove_invalid_balls();
        let updating_ms = Self::elapsed_since(start);

        let start = Instant::now();
        self.calculate_temperature();
        let props_ms = Self::elapsed_since(start);
        let all_ms = Self::elapsed_since(start_all);

        if self.show_timings && all_ms > 1 {
            let text_timing = format!(
                "coll/upd/prop/all: {}/{}/{}/{}",
                collecting_ms, updating_ms, props_ms, all_ms
            );
            println!("{:>5}", text_timing);
        }

        self.step_count += 1;
        Ok(())
    }
}

const PLT_SIZE: usize = 1000;
const PLT_STEP: usize = 100;

impl<'a> VerletIntegration for MainContent<'a> {
    fn update_sim(&mut self, dt: f32, bound_rect: &Rect) -> GameResult {
        let mut _min_dist = bound_rect.w + bound_rect.h;
        let mut max_v: f32 = 0.;
        let mut max_id: usize = 0;
        let mut pot: f32 = 0.;
        let mut kin: f32 = 0.;
        let mut _v_com: Vec2D = Vec2D::new(0., 0.);
        let mut sum_mass: f32 = 0.;

        //update positions
        for ball_rc in &mut self.v_points {
            let mut cur = ball_rc.borrow_mut();

            cur.update_positions(dt, &bound_rect);
        }
        self.v_points
            .iter()
            .for_each(|ball| ball.borrow_mut().update_old_pos());

        //update velocities and accel with new positions
        for ball_rc in &mut self.v_points {
            let mut cur = ball_rc.borrow_mut();
            let (min_dist, cur_pot) = cur.update_accel_and_speed(dt, &bound_rect);

            //calculate properties
            if _min_dist > min_dist && min_dist > 0. {
                _min_dist = min_dist;
                self.sim.min_dist = _min_dist;
            }
            pot += cur_pot * 0.5; //double counts
            _v_com.add(cur.moment());
            let speed_sq = cur.v.norm_sq();
            kin += speed_sq * cur.mass * 0.5;
            sum_mass += cur.mass;
            if max_v * max_v < speed_sq {
                max_v = f32::sqrt(speed_sq);
                max_id = cur.get_id();
            }
        }

        self.sim.energy_kin = kin;
        self.sim.energy_pot = pot;
        if (self.sim.step % PLT_STEP) == 0 {
            if self.sim.v_pot.len() >= PLT_SIZE {
                self.sim.v_pot.pop_front();
            }
            self.sim.v_pot.push_back(pot);
            if self.sim.v_kin.len() >= PLT_SIZE {
                self.sim.v_kin.pop_front();
            }
            self.sim.v_kin.push_back(kin);
        }

        self.sim.max_v = max_v;
        self.sim.max_id = max_id;
        self.sim.time += dt;
        self.sim.step += 1;
        self.sim.v_com = _v_com.scaled(1. / sum_mass);

        Ok(())
    }
}

impl<'a> SimulationToggler for MainContent<'a> {
    fn make_all_verbose(&mut self, mode: VerboseInfoMode) {
        for b in self.v_points.iter() {
            Self::toggle_ball_internal(&mut b.borrow_mut(), &mode);
        }
    }
    fn toggle_ball_verbose(&mut self, number: usize, mode: VerboseInfoMode) {
        if let Some(ball_rc) = self
            .v_points
            .iter()
            .find(|&ball_rc_in| ball_rc_in.borrow().get_id() == number)
        {
            Self::toggle_ball_internal(&mut ball_rc.borrow_mut(), &mode);
        }
    }

    fn clear_verbose_balls(&mut self) {
        for b in self.v_points.iter() {
            let mut ba = b.borrow_mut();
            ba.verbose.clear_all();
        }
    }
}
impl<'a> EventHandler for MainContent<'a> {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        // let mut cnt = 0;
        // let mut sum: Duration = Duration::new(0, 0);
        let tmp_cnt = Ball2D::get_update_hood_call_cnt();
        while _ctx.time.check_update_time(FPS_GOAL * self.sim.speedup) {
            // let frame_start = Instant::now();
            // let dur = frame_start.checked_duration_since(self.sim.time_last_frame);
            // if let Some(dur) = dur {
            //     cnt+=1;
            //     sum+=dur;
            // }
            //
            // self.sim.time_last_frame = frame_start;
            self.sim.frames_since_draw += 1;
            self.simulation_step()?;
        }
        self.sim.update_hood_cnt = Ball2D::get_update_hood_call_cnt() - tmp_cnt;
        // if cnt > 0 {
        //     println!("simulate {} avg {:?} simtime {}", cnt, sum/cnt, self.sim.time);
        // }
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        // Get the window's width and height
        // println!("{} draw", self.sim.time);
        let mut canvas = Canvas::from_frame(ctx, Color::from_rgb(0, 0, 0));
        self.draw_balls(ctx, &mut canvas)?;
        self.draw_cursor(ctx, &mut canvas)?;
        self.draw_frame(ctx, &mut canvas)?;
        self.draw_plot(ctx, &mut canvas)?;
        canvas.finish(ctx)?;
        self.sim.frames_since_draw = 0;
        Ok(())
    }

    fn mouse_button_down_event(
        &mut self,
        _ctx: &mut Context,
        button: MouseButton,
        _x: f32,
        _y: f32,
    ) -> GameResult {
        match button {
            MouseButton::Right => {
                self.new_ball.next_ball_size();
            }
            _ => (),
        }
        Ok(())
    }

    // fn mouse_button_up_event(
    //     &mut self,
    //     _ctx: &mut Context,
    //     button: MouseButton,
    //     x: f32,
    //     y: f32,
    // ) -> Result<(), GameError> {
    //     match button {
    //         MouseButton::Left => {
    //             self.add_candidate_px(x, y).ok(); //ignore any errors because of not being able to add
    //             self.sim.draw_cursor = false;
    //         }
    //         _ => (),
    //     }
    //     Ok(())
    // }

    fn mouse_motion_event(
        &mut self,
        _ctx: &mut Context,
        x: f32,
        y: f32,
        _dx: f32,
        _dy: f32,
    ) -> GameResult {
        let used_x = f32::max(
            self.win_definition.margin_px_x,
            f32::min(
                self.win_definition.margin_px_x + self.win_definition.region_px_x,
                x,
            ),
        );
        let used_y = f32::max(
            self.win_definition.margin_px_y,
            f32::min(
                self.win_definition.margin_px_y + self.win_definition.region_px_y,
                y,
            ),
        );
        self.win_definition.curs_pos = Vec2D::new(used_x, used_y);
        Ok(())
    }

    fn key_down_event(
        &mut self,
        _context: &mut Context,
        input: KeyInput,
        repeated: bool,
    ) -> Result<(), GameError> {
        if let Some(key) = input.keycode {
            match key {
                KeyCode::Space => self.toggle_pause(),
                KeyCode::LControl => self.enable_drawing_cursor(repeated),
                KeyCode::Escape => self.clear_points(),
                KeyCode::LShift => self.enable_random_ball_speed(),
                _ => {} // explicit ignoring of other keys
            }
        }

        Ok(())
    }

    fn key_up_event(&mut self, _ctx: &mut Context, input: KeyInput) -> Result<(), GameError> {
        match input.keycode {
            Some(key) => match key {
                KeyCode::H => {
                    MainContent::print_help_message();
                }
                KeyCode::D => self.real_data = !self.real_data,
                KeyCode::S => self.step_by_step = !self.step_by_step,
                KeyCode::T => self.show_timings = !self.show_timings,
                KeyCode::P => {
                    self.show_plot = match self.show_plot {
                        PlotContent::NONE => PlotContent::EKIN,
                        PlotContent::EKIN => PlotContent::EPOT,
                        PlotContent::EPOT => PlotContent::ETOT,
                        PlotContent::ETOT => PlotContent::ALL,
                        PlotContent::ALL => PlotContent::NONE,
                    }
                }
                KeyCode::Equals => self.sim.speedup += 1,
                /*KeyCode::Plus => {
                    self.sim.speedup += 1.
                },*/
                KeyCode::Minus => {
                    if self.sim.speedup > 1 {
                        self.sim.speedup -= 1;
                    } else {
                        self.sim.speedup = 1;
                    }
                }
                KeyCode::R => self.randomize_speeds(),
                KeyCode::B => self.show_ball_stats = !self.show_ball_stats,
                KeyCode::V => {
                    self.start_handling_verbosity_input();
                }
                KeyCode::LControl => {
                    self.add_candidate_px(
                        self.win_definition.curs_pos.x,
                        self.win_definition.curs_pos.y,
                    )
                    .ok(); //ignore any errors because of not being able to add
                    self.sim.draw_cursor = false;
                }
                KeyCode::F5 => {
                    let x_min = self.win_definition.margin_px_x;
                    let y_min = self.win_definition.margin_px_y;
                    let x_range = self.win_definition.region_px_x;
                    let y_range = self.win_definition.region_px_y;
                    let mut rng = rand::rng();
                    let rand_xy: Vec<(f32, f32)> = (0..100)
                        .map(|_| {
                            (
                                x_min + rng.random_range(0.0..x_range),
                                y_min + rng.random_range(0.0..y_range),
                            )
                        })
                        .collect();
                    for xy in rand_xy {
                        self.add_candidate_px(xy.0, xy.1).ok();
                    }
                }
                KeyCode::LShift => self.new_ball.rand_speed = false,
                // KeyCode::Space => self.sim.pause = !self.sim.pause,
                _ => {}
            },
            None => {}
        }

        Ok(())
    }

    fn resize_event(
        &mut self,
        _ctx: &mut Context,
        width: f32,
        height: f32,
    ) -> Result<(), GameError> {
        self.resize(width, height)?;
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
        assert_eq!(default_ball_def.get_ball_size(), MIN_BALL_SIZE);
        assert_eq!(default_ball_def.ball_mass, 1.);

        let default_sim_stats = SimStats::default();
        assert_eq!(default_sim_stats.energy_pot, 0.);
        assert_eq!(default_sim_stats.energy_kin, 0.);
    }

    #[test]
    fn test_add_candidate_px() {
        let mut main_content = MainContent::default();

        // Add a valid candidate
        let x_px = main_content.win_definition.margin_px_x
            + MainContent::len_to_px(main_content.new_ball.get_ball_size() * 1.1);
        let y_px = main_content.win_definition.margin_px_y
            + MainContent::len_to_px(main_content.new_ball.get_ball_size() * 1.1);
        println!(
            "test adding ball at px ({},{}) | LEN_TO_PX: {}",
            x_px, y_px, LEN_TO_PX
        );
        main_content.add_candidate_px(x_px, y_px).unwrap();

        assert_eq!(main_content.num_balls, 1);

        // Ensure the added ball is in the correct position
        {
            let ball = main_content.v_points[0].borrow();
            let ball_pos_px = main_content.coord_to_px(&ball.pos);
            assert!(
                approx_eq!(f32, ball_pos_px.x, x_px, ulps = 2),
                "values in x are not close: {} vs {}",
                ball_pos_px.x,
                x_px
            );
            assert!(
                approx_eq!(f32, ball_pos_px.y, y_px, ulps = 2),
                "values in y are not close: {} vs {}",
                ball_pos_px.y,
                y_px
            );
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
        let ball_px: Vec2D = Vec2D::new(30., 30.);
        let ball_coord = main_content.px_to_coord(ball_px.x, ball_px.y);
        println!(
            "ball coordindates are {:?} from px ({},{})",
            ball_coord, ball_px.x, ball_px.y
        );
        let ball_radius = MainContent::px_to_len(10.);
        let ball = Ball2D::_new_massive(1, ball_coord.x, ball_coord.y, ball_radius, 1.0);

        assert!(main_content.is_ball_in_region(&ball));

        // Move the ball outside the region
        let invalid_ball = Ball2D::_new_massive(2, -10.0, -10.0, 5.0, 1.0);
        assert!(!main_content.is_ball_in_region(&invalid_ball));
    }

    #[test]
    fn test_global_update() {
        let mut main_content = MainContent::default();

        // Add some balls
        let ball_coord_px = Vec2D::new(30., 30.);
        let ball_radius_px = MainContent::len_to_px(main_content.new_ball.get_ball_size());
        main_content
            .add_candidate_px(ball_coord_px.x, ball_coord_px.y)
            .unwrap();
        main_content
            .add_candidate_px(
                ball_coord_px.x + ball_radius_px * 2.2,
                ball_coord_px.y + ball_radius_px * 2.2,
            )
            .unwrap();
        main_content.make_all_verbose(VerboseInfoMode::CONSOLE);
        main_content.simulation_step().unwrap();

        // Validate that the balls' velocities, positions, and energies are updated correctly
        assert!(
            main_content.sim.energy_kin > 0.0,
            "Kinetic energy should be positive but was {}",
            main_content.sim.energy_kin
        );
        //assert!(main_content.sim.energy_pot >= 0.0, "Potential energy should be non-negative");
    }

    #[test]
    fn test_resize_event() {
        let mut main_content = MainContent::default();
        let new_width = 300.0;
        let new_height = 200.0;

        main_content.resize(new_width, new_height).unwrap();

        assert_eq!(
            main_content.win_definition.region_px_x,
            300.0 - 2.0 * main_content.win_definition.margin_px_x
        );
        assert_eq!(
            main_content.win_definition.region_px_y,
            200.0 - 2.0 * main_content.win_definition.margin_px_y
        );
    }

    #[test]
    fn test_ball_collision() {
        let mut main_content = MainContent::default();

        // Add two overlapping balls
        let ball_coord_px = Vec2D::new(30., 30.);
        let ball_radius_px = MainContent::len_to_px(main_content.new_ball.get_ball_size());
        let ball_radius = main_content.new_ball.get_ball_size();
        main_content
            .add_candidate_px(ball_coord_px.x, ball_coord_px.y)
            .unwrap();

        let ball_coord = main_content.px_to_coord(ball_coord_px.x, ball_coord_px.y);
        let new_ball_coord = Vec2D::new(
            ball_coord.x + 0.5 * ball_radius,
            ball_coord.y + 0.5 * ball_radius,
        );
        let colliding_ball =
            Ball2D::_new_massive(2, new_ball_coord.x, new_ball_coord.y, ball_radius, 1.0);

        println!(
            "existing ball {:?} and new ball {:?} both with radius {}",
            ball_coord_px,
            main_content.coord_to_px(&new_ball_coord),
            ball_radius_px
        );

        assert!(
            main_content.is_ball_colliding(&colliding_ball),
            "Collision not detected"
        );
    }

    #[test]
    fn test_coord_trafo_idempotent() {
        let main_content = MainContent::new(500., 500., None).expect("could not create content");
        let test_coord = Vec2D::new(18., 234.);
        let vec = main_content.coord_to_px(&test_coord);
        let res_coord = main_content.px_to_coord(vec.x, vec.y);

        assert!(
            approx_eq!(f32, test_coord.x, res_coord.x, ulps = 2),
            "values in x are not close: {} vs {}",
            test_coord.x,
            res_coord.x
        );
        assert!(
            approx_eq!(f32, test_coord.y, res_coord.y, ulps = 2),
            "values in y are not close: {} vs {}",
            test_coord.y,
            res_coord.y
        );
    }

    #[test]
    fn test_px_to_coord() {
        let main_content = MainContent::new(500., 300., None).expect("could not create content");

        println!(
            "window size: {} {} margin {} {} and region {} {}",
            main_content.win_definition.margin_px_x * 2. + main_content.win_definition.region_px_x,
            main_content.win_definition.margin_px_y * 2. + main_content.win_definition.region_px_y,
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
            assert!(
                approx_eq!(f32, coord.x, x_len, ulps = 2),
                "values in x are not close: {} vs {}",
                coord.x,
                x_len
            );
            assert!(
                approx_eq!(f32, coord.y, y_len, ulps = 2),
                "values in y are not close: {} vs {}",
                coord.y,
                y_len
            );
        }
    }

    #[test]
    fn test_collect_neighbours() {
        let mut main_content =
            MainContent::new(500., 500., None).expect("could not create content");

        // Add balls to the system
        let x1 = 80.0;
        let y1 = 80.0;
        let x2 = 105.0;
        let y2 = 105.0;
        let x3 = 200.0;
        let y3 = 200.0;

        let ball1_coord = main_content.px_to_coord(x1, y1);
        let ball2_coord = main_content.px_to_coord(x2, y2);
        let ball3_coord = main_content.px_to_coord(x3, y3);

        println!(
            "b1 {:?} b2 {:?} b3 {:?}",
            ball1_coord, ball2_coord, ball3_coord
        );
        main_content.add_candidate_px(x1, y1).unwrap();
        main_content.add_candidate_px(x2, y2).unwrap();
        main_content.add_candidate_px(x3, y3).unwrap();

        // Initial state check
        {
            let ball1: &Ball2D = &*main_content.v_points[0].borrow();
            let ball2: &Ball2D = &*main_content.v_points[1].borrow();
            let ball3: &Ball2D = &*main_content.v_points[2].borrow();

            assert!(
                ball1.get_neighbour_names().is_empty(),
                "Ball 1 should have no neighbours initially"
            );
            assert!(
                ball2.get_neighbour_names().is_empty(),
                "Ball 2 should have no neighbours initially"
            );
            assert!(
                ball3.get_neighbour_names().is_empty(),
                "Ball 3 should have no neighbours initially"
            );
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

            assert_eq!(
                ball1.get_neighbour_names().len(),
                1,
                "Ball 1 should have 1 neighbour"
            );
            assert_eq!(
                ball2.get_neighbour_names().len(),
                1,
                "Ball 2 should have 1 neighbour"
            );
            assert!(
                ball1.get_neighbour_names().contains(&"Ball2".to_string()),
                "Ball 1 should be neighbour of Ball 2"
            );
            assert!(
                ball2.get_neighbour_names().contains(&"Ball1".to_string()),
                "Ball 2 should be neighbour of Ball 1"
            );

            assert!(
                ball3.get_neighbour_names().is_empty(),
                "Ball 3 should have no neighbours because it is far from others"
            );
        }
    }
}
