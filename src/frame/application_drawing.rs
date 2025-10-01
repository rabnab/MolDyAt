use crate::frame::LogStuff;
use crate::frame::window_definition::WindowDefinition;
use crate::frame::{DrawingStuff, MARGIN_PX_Y, MainContent};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use ggez::glam::Vec2;
use ggez::graphics::{
    Canvas, Color, DrawMode, DrawParam, Drawable, Mesh, MeshBuilder, Rect, Shader, Text,
    TextFragment,
};
use ggez::mint::Point2;
use ggez::{Context, GameError, GameResult};
use std::cell::{Ref, RefCell};
use std::cmp::PartialEq;
use std::rc::Rc;

const SHOW_FORCE: bool = false;
const CROSSHAIR_SIZE: f32 = 10.0;
const LINE_THICKNESS: f32 = 2.0;
const CROSSHAIR_COLOR: Color = Color::WHITE;
#[derive(PartialEq, Copy, Clone)]
pub(crate) enum PlotContent {
    NONE,
    EKIN,
    EPOT,
    ETOT,
    ALL,
}

impl<'a> MainContent<'a> {
    pub(crate) fn create_crosshair(ctx: &Context) -> Option<Mesh> {
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

    fn rect_of_tuple(top_left: (f32, f32), format: (f32, f32)) -> Rect {
        Rect::new(top_left.0, top_left.1, format.0, format.1)
    }
}

impl<'a> DrawingStuff<'a> for MainContent<'a> {
    fn draw_cursor(&self, canvas: &mut Canvas) -> GameResult {
        let win_def = self.get_window_definition();
        if self.sim.draw_cursor() {
            let ball_mesh = self.new_ball.get_mesh(self.new_ball.get_ball_size());
            canvas.draw(
                ball_mesh.unwrap(),
                DrawParam::default()
                    .color(Color::WHITE)
                    .dest(win_def.curs_pos.clone()),
            );
        } else {
            self.draw_cross_hair(canvas)?;
        }

        let txt_cursor_pos = [
            win_def.margin_px.0 * 2. + win_def.region_px.0,
            win_def.margin_px.1 * 2. + win_def.region_px.1,
        ];
        canvas.draw(
            &win_def.txt_cursor,
            DrawParam::default().dest(txt_cursor_pos).offset([1., 1.]),
        );

        Ok(())
    }

    fn calculate_plot_boundary<F>(&self, initial: f32, operation: F) -> f32
    where
        F: Fn(f32, f32) -> f32,
    {
        let val: f32 = match self.plot_type() {
            PlotContent::NONE => f32::NAN,
            PlotContent::EKIN => self.sim.iter_vkin().cloned().fold(initial, operation),
            PlotContent::EPOT => self.sim.iter_vpot().cloned().fold(initial, operation),
            PlotContent::ETOT => self
                .sim
                .iter_vkin()
                .zip(self.sim.iter_vpot())
                .map(|(kin, pot)| kin + pot)
                .fold(initial, operation),
            PlotContent::ALL => self
                .sim
                .iter_vkin()
                .chain(self.sim.iter_vpot())
                .cloned()
                .fold(initial, operation),
        };
        val
    }

    fn draw_plot(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        if self.plot_type() == PlotContent::NONE {
            return Ok(());
        }
        let max_len = self.sim.len_v_energy();
        if max_len <= 3 {
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

        // Helper to convert (index, value) to screen coordinates
        let to_screen_coords = |index: usize, value: f32| -> Point2<f32> {
            let x = plot_x + (index as f32 / max_len as f32) * PLOT_WIDTH;
            let y = plot_y + PLOT_HEIGHT - ((value - min_val) / value_range * PLOT_HEIGHT);
            Point2 { x, y }
        };

        let mut mesh_builder = MeshBuilder::new();

        // Draw e_kin line (red)
        if self.plot_type() == PlotContent::ALL || self.plot_type() == PlotContent::EKIN {
            self.sim
                .add_vkin_to_line(&mut mesh_builder, to_screen_coords)?;
        }

        // Draw e_pot line (blue)
        if self.plot_type() == PlotContent::ALL || self.plot_type() == PlotContent::EPOT {
            self.sim
                .add_vpot_to_line(&mut mesh_builder, to_screen_coords)?;
        }

        // Draw e_tot line (Yellow)
        if self.plot_type() == PlotContent::ALL || self.plot_type() == PlotContent::ETOT {
            self.sim
                .add_vtot_to_line(&mut mesh_builder, to_screen_coords)?;
        }

        // Draw bounding box
        mesh_builder.rectangle(
            DrawMode::stroke(2.0),
            Rect::new(plot_x, plot_y, PLOT_WIDTH, PLOT_HEIGHT),
            Color::WHITE,
        )?;

        let mesh = Mesh::from_data(ctx, mesh_builder.build());
        canvas.draw(&mesh, DrawParam::default());

        // Draw max value label
        canvas.draw(
            &Text::new(TextFragment::new(format!("{:.2e}", max_val)).color(Color::WHITE)),
            Point2 {
                x: plot_x + 5.0,
                y: plot_y + 5.0,
            },
        );

        // Draw min value label
        canvas.draw(
            &Text::new(TextFragment::new(format!("{:.2e}", min_val)).color(Color::WHITE)),
            Point2 {
                x: plot_x + 5.0,
                y: plot_y + PLOT_HEIGHT - 15.0,
            },
        );

        Ok(())
    }

    fn draw_text_on_ball(&self, canvas: &mut Canvas, ball: &Ball2D) -> GameResult {
        let txt_name = Text::new(TextFragment::new(format!("{}", ball.get_id())).scale(15.));
        //TODO move this text to ball for performance
        let ball_pos_px = self
            .get_window_definition()
            .coord_to_px(&ball.get_position());
        let txt_name_pos = [ball_pos_px.x, ball_pos_px.y];

        canvas.draw(
            &txt_name,
            DrawParam::default().dest(txt_name_pos).offset([0.5, 0.5]), //for text to be centered
        );
        Ok(())
    }

    fn draw_simple_balls(
        &self,
        canvas: &mut Canvas,
        balls: &Vec<Rc<RefCell<Ball2D>>>,
    ) -> GameResult {
        for ball_cell in balls {
            self.draw_ball_by_mesh(canvas, ball_cell)?;
        }
        Ok(())
    }

    fn draw_ball_by_mesh(
        &self,
        canvas: &mut Canvas,
        ball_cell: &Rc<RefCell<Ball2D>>,
    ) -> GameResult {
        let ball = ball_cell.borrow();
        let ball_coord_px = self
            .get_window_definition()
            .coord_to_px(&ball.get_position());

        if let Some(cur_mesh) = self.new_ball.get_mesh(ball.radius) {
            self.draw_single_ball(cur_mesh, canvas, &ball, &ball_coord_px)?;
        }

        self.draw_text_on_ball(canvas, &ball)?;
        Ok(())
    }

    fn draw_neighbour_lines(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError> {
        let neighbour_info = self.physics.get_neighbour_force_and_pos(ball);
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
            let end: Point2<f32> = self.get_window_definition().coord_to_px(&other_pos).into();
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

                let text_force = MainContent::format_g(other_force, 3);
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
        canvas: &mut Canvas,
        ball: &Ref<Ball2D>,
        ball_coord_px: &Vec2D,
    ) -> Result<(), GameError> {
        // Draw the ball's trajectory tail

        let draw_par = DrawParam::default().dest(ball_coord_px.clone());

        if self.sim.is_max_id(ball.get_id()) {
            draw_par.color(Color::CYAN);
        } else if let Some(col) = ball.ball_color() {
            draw_par.color(col);
        }
        canvas.draw(pt_circle, draw_par);

        Ok(())
    }

    fn draw_all_ball_tails(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        balls: &Vec<Rc<RefCell<Ball2D>>>,
    ) -> Result<(), GameError> {
        let mut mesh_builder = MeshBuilder::new();
        let to_screen_coords =
            |pos: &Vec2D| -> Point2<f32> { self.get_window_definition().coord_to_px(pos).into() };

        for ball_ref in balls {
            let ball = ball_ref.borrow();
            if let Some(tail_positions) = ball.get_tail_positions() {
                let points: Vec<Point2<f32>> =
                    tail_positions.iter().map(to_screen_coords).collect();
                if points.len() > 1 {
                    mesh_builder.polyline(DrawMode::stroke(1.0), &points, Color::WHITE)?;
                }
            }
        }

        let tail_mesh = Mesh::from_data(ctx, mesh_builder.build());
        canvas.draw(&tail_mesh, DrawParam::default());
        Ok(())
    }

    fn draw_opengl_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        shader_used: &Shader,
        balls: &Vec<Rc<RefCell<Ball2D>>>,
    ) -> GameResult {
        canvas.set_shader(shader_used);
        //let ball_paramsa = ShaderParamsBuilder::new(&ShaderUniforms);
        //canvas.set_shader_params(ball_params);
        for ball_ref in balls {
            let ball = ball_ref.borrow();
            if let Some(cur_mesh) = self.new_ball.get_mesh(ball.radius) {
                let ball_coord_px = self
                    .get_window_definition()
                    .coord_to_px(&ball.get_position());
                canvas.draw(cur_mesh, DrawParam::default().dest(ball_coord_px));
            }
            //canvas.set_shader(&ggez::graphics::default_shader());
            self.draw_text_on_ball(canvas, &ball)?;
        }
        canvas.set_default_shader();
        Ok(())
    }

    fn draw_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        balls: &Vec<Rc<RefCell<Ball2D<'a>>>>,
    ) -> GameResult {
        match &self.get_shader() {
            Some(shader) => self.draw_opengl_balls(ctx, canvas, shader, balls)?,
            None => {
                self.draw_all_ball_tails(ctx, canvas, balls)?;
                self.draw_simple_balls(canvas, balls)?;
            }
        }

        let verbose_balls = Self::get_verbose_balls(balls);
        self.draw_verbose_balls(ctx, canvas, &verbose_balls)?;
        Ok(())
    }

    fn get_verbose_balls(balls: &Vec<Rc<RefCell<Ball2D<'a>>>>) -> Vec<Rc<RefCell<Ball2D<'a>>>> {
        balls
            .iter()
            .filter(|ball| ball.borrow().verbose.lines())
            .cloned()
            .collect()
    }

    fn draw_verbose_balls(
        &self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        verbose_balls: &[Rc<RefCell<Ball2D>>],
    ) -> GameResult {
        for ball_ref in verbose_balls.iter() {
            let ball = ball_ref.borrow();
            let ball_coord_px = self
                .get_window_definition()
                .coord_to_px(&ball.get_position());
            self.draw_neighbour_lines(ctx, canvas, &ball, &ball_coord_px)?;
        }
        Ok(())
    }

    fn draw_cross_hair(&self, canvas: &mut Canvas) -> GameResult<()> {
        canvas.draw(
            self.crosshair_mesh.as_ref().unwrap(),
            self.get_window_definition().curs_pos.clone(),
        );
        Ok(())
    }

    fn draw_frame(
        &mut self,
        ctx: &mut Context,
        canvas: &mut Canvas,
        num_balls: usize,
    ) -> GameResult {
        // Draw cell grid background
        self.draw_cell_grid_background(ctx, canvas)?;

        let win_def = self.get_window_definition();
        let rect = Mesh::new_rectangle(
            ctx,
            DrawMode::stroke(3.),
            Self::rect_of_tuple(win_def.margin_px, win_def.region_px),
            Color::BLUE,
        )?;

        canvas.draw(&rect, Vec2::new(0., 0.));

        // First, get all the values you need before the mutable borrow
        let show_ball_stats = self.show_ball_stats;
        let status_text = self.status_text(ctx.time.fps() as u32, num_balls);

        // Then do the mutable borrow
        let possible_text =
            self.mut_window_definition()
                .update_status_text(ctx, show_ball_stats, status_text);
        if let Some(text_to_draw) = possible_text {
            canvas.draw(text_to_draw.0, text_to_draw.1);
        }

        Ok(())
    }

    fn resize(&mut self, width: f32, height: f32) -> Result<(), GameError> {
        let win_def = self.mut_window_definition();
        win_def.region_px.0 = width - 2. * win_def.margin_px.0;
        win_def.region_px.1 = height - 2. * win_def.margin_px.1;
        Ok(())
    }
}

impl<'a> MainContent<'a> {
    /// Draws the cell grid as faint colored rectangles in the background.
    fn draw_cell_grid_background(&self, ctx: &mut Context, canvas: &mut Canvas) -> GameResult {
        let grid = &self.cell_grid;
        let (rows, cols) = grid.get_grid_size();

        let win_def = self.get_window_definition();
        for row in 0..rows {
            for col in 0..cols {
                if let Some(cur_cell) = grid.get_cell(row, col) {
                    cur_cell.get_rect_mesh().map(|mesh| {
                        let scaling_vec = Vec2::new(
                            WindowDefinition::len_to_px(1.),
                            WindowDefinition::len_to_px(1.),
                        );
                        let scale_and_pos = DrawParam::default()
                            .dest(win_def.coord_to_px(&cur_cell.get_top_left()))
                            .scale(scaling_vec);

                        canvas.draw(mesh, scale_and_pos);
                    });
                }
            }
        }

        Ok(())
    }
}
