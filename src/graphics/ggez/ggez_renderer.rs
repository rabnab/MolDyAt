use crate::application::AppResult;
use crate::graphics::ggez::WindowDefinitionRef;
use crate::graphics::ggez::ggez_renderer_backend::GgezMeshCache;
use crate::graphics::renderer::{DrawingBackend, PlotContent, Renderer};
use crate::graphics::window::window_definition::WindowDefinition;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics};
use ggez::Context;
use ggez::graphics::{Color, DrawMode, Mesh, MeshBuilder, Shader};
use ggez::mint::Point2;
use std::cell::Ref;
use std::collections::VecDeque;

pub(crate) const CIRCLE_TOLERANCE: f32 = 0.1; // Tolerance for circle drawing
pub(crate) const STROKE_THICKNESS: f32 = 2.0; // Border thickness for stroke drawing
pub(super) const CROSSHAIR_SIZE: f32 = 10.0;
const LINE_THICKNESS: f32 = 2.0;
const CROSSHAIR_COLOR: Color = Color::WHITE;

pub struct GgezRenderer {
    show_plot: PlotContent,
    shader: Option<Shader>,
    ball_status_text_pos: Option<[f32; 2]>,
    last_status_text: String,
    txt_cursor: String,
    curs_pos_of_txt: Vec2D,
    window_def: WindowDefinitionRef,
    draw_cursor: bool,
    cursor_pos_px: [f32; 2],
    cache: GgezMeshCache,
    physics: Option<BallPhysics>,
    string_elems: usize,
    glyph_elems: usize,
    ball_elems: usize,
}

impl GgezRenderer {
    pub(crate) fn new(
        plot_type: PlotContent,
        crosshair_mesh: Mesh,
        ball_mesh: Vec<Mesh>,
        window_def: WindowDefinitionRef,
    ) -> Self {
        let cache = GgezMeshCache {
            crosshair_mesh: Some(crosshair_mesh),
            ball_mesh,
            ..Default::default()
        };

        let ball_text_pos = {
            let windef = window_def.borrow();
            [windef.margin_px.0 * 2.0 + windef.region_px.0, 0.0]
        };
        Self {
            show_plot: plot_type,
            window_def,
            shader: None,
            ball_status_text_pos: Some(ball_text_pos),
            last_status_text: "".to_string(),
            curs_pos_of_txt: Vec2D::zero(),
            draw_cursor: false,
            cursor_pos_px: [0., 0.],
            txt_cursor: format!("x: {}, y: {} pos ({:.3},{:.3})", 0., 0., 0., 0.,),
            cache,
            physics: None,
            string_elems: 0,
            glyph_elems: 0,
            ball_elems: 0,
        }
    }
}

impl Renderer for GgezRenderer {
    fn set_physics_engine(&mut self, engine: BallPhysics) {
        self.physics = Some(engine);
    }

    fn plot_type(&self) -> PlotContent {
        self.show_plot
    }
    fn cycle_plot_content(&mut self) {
        self.show_plot = match self.show_plot {
            PlotContent::None => PlotContent::Ekin,
            PlotContent::Ekin => PlotContent::Epot,
            PlotContent::Epot => PlotContent::Etot,
            PlotContent::Etot => PlotContent::All,
            PlotContent::All => PlotContent::None,
        };
    }

    /// Adds the kinetic energy data points to the existing line mesh builder and creates the builder if it doesn't exist.
    fn add_vkin_to_line(&mut self, v_kin: &VecDeque<f32>) -> AppResult<()> {
        let win_def = self.window_def.borrow();

        let builder = self
            .cache
            .line_vkin_builder
            .get_or_insert_with(MeshBuilder::new);
        GgezRenderer::add_data_to_line(win_def, builder, Color::RED, v_kin)?;
        Ok(())
    }

    fn add_vpot_to_line(&mut self, v_pot: &VecDeque<f32>) -> AppResult<()> {
        let win_def = self.window_def.borrow();
        let builder = self
            .cache
            .line_vpot_builder
            .get_or_insert_with(MeshBuilder::new);
        Self::add_data_to_line(win_def, builder, Color::BLUE, v_pot)?;
        Ok(())
    }

    fn add_vtot_to_line(&mut self, v_kin: &VecDeque<f32>, v_pot: &VecDeque<f32>) -> AppResult<()> {
        let data = v_kin
            .iter()
            .zip(v_pot.iter())
            .map(|(k, p)| -> f32 { k + p })
            .collect();
        let win_def = self.window_def.borrow();
        let builder = self
            .cache
            .line_vtot_builder
            .get_or_insert_with(MeshBuilder::new);
        Self::add_data_to_line(win_def, builder, Color::YELLOW, &data)?;
        Ok(())
    }

    fn draw_neighbour_lines(
        &self,
        backend: &mut dyn DrawingBackend,
        ball: &Ball2D,
        balls: &[Ball2D],
    ) -> AppResult {
        let ball_coord_px = self.window_def.borrow().coord_to_px(ball.get_position());

        let neighbour_info = match &self.physics {
            Some(physics) => physics.get_neighbour_force_and_pos(ball, balls),
            None => vec![],
        };
        let start: [f32; 2] = [ball_coord_px.x, ball_coord_px.y];
        let max_force = neighbour_info
            .iter()
            .map(|info| info.1)
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
            let other_pos_px = self.window_def.borrow().coord_to_px(other_pos);
            let end: [f32; 2] = [other_pos_px.x, other_pos_px.y];
            // Create a line mesh
            backend.draw_line(&[start, end], 2.0, color_line.to_rgba())?;
        }
        Ok(())
    }

    // todo!()
    // fn draw_all_ball_tails(
    //     &self,
    //     ctx: &mut Context,
    //     canvas: &mut Canvas,
    //     balls: &Vec<Rc<RefCell<Ball2D>>>,
    // ) -> Result<(), GameError> {
    //     let mut mesh_builder = MeshBuilder::new();
    //     let to_screen_coords =
    //         |pos: &Vec2D| -> Point2<f32> { self.get_window_definition().coord_to_px(pos).into() };
    //
    //     for ball_ref in balls {
    //         let ball = ball_ref.borrow();
    //         if let Some(tail_positions) = ball.get_tail_positions() {
    //             let points: Vec<Point2<f32>> =
    //                 tail_positions.iter().map(to_screen_coords).collect();
    //             if points.len() > 1 {
    //                 mesh_builder.polyline(DrawMode::stroke(1.0), &points, ggez::graphics::Color::WHITE)?;
    //             }
    //         }
    //     }
    //
    //     let tail_mesh = Mesh::from_data(ctx, mesh_builder.build());
    //     canvas.draw(&tail_mesh, DrawParam::default());
    //     Ok(())
    // }

    fn draw_cursor(&self, backend: &mut dyn DrawingBackend, size_idx: usize) -> AppResult {
        if self.draw_cursor {
            backend.draw_ball_mesh(0, size_idx, self.cursor_pos_px, true)?;
        } else {
            backend.draw_cross_hair(self.cursor_pos_px)?;
        }

        let win_def = self.window_def.borrow();
        let txt_cursor_pos = [
            win_def.margin_px.0 * 2. + win_def.region_px.0,
            win_def.margin_px.1 * 2. + win_def.region_px.1,
        ];
        backend.draw_text(self.txt_cursor.clone(), txt_cursor_pos, 20., true, true)?;

        Ok(())
    }

    fn draw_plot(&mut self) -> AppResult<()> {
        todo!()
    }

    fn draw_frame(
        &mut self,
        backend: &mut dyn DrawingBackend,
        cell_grid: &CellGrid,
        status_text: Option<String>,
    ) -> AppResult<()> {
        let win_def = self.window_def.borrow();

        // 1) Draw cell grid and rectangle (short-lived &mut Canvas)
        backend.draw_cell_grid_background(cell_grid, &win_def)?;
        backend.draw_boundary_frame(win_def.margin_px, win_def.region_px)?;

        // 2) draw upper right text
        if let Some(text) = status_text {
            let pos = self.ball_status_text_pos.unwrap_or([0., 0.]);

            let full_text = format!(
                "{}\n[Cache: strings {}, glyphs {}, ball labels {}]",
                text, self.string_elems, self.glyph_elems, self.ball_elems
            );
            backend.draw_text(full_text, pos, 30., true, false)?;
        }

        Ok(())
    }

    fn finish_frame(&mut self, backend: &mut dyn DrawingBackend) -> AppResult<()> {
        backend.finish_frame()
    }

    fn disable_drawing_cursor(&mut self) {
        self.draw_cursor = false;
    }

    fn enable_drawing_cursor(&mut self, repeated: bool) {
        if !repeated {
            self.draw_cursor = true;
        }
    }

    fn resize(&mut self, width: f32, height: f32) {
        self.cache.cell_mesh = None;
        let mut win_def = self.window_def.borrow_mut();
        win_def.resize(width, height);
        self.ball_status_text_pos = Some([win_def.margin_px.0 * 2.0 + win_def.region_px.0, 0.0]);
    }

    fn get_world_region(&self) -> crate::objects2d::Rect {
        self.window_def.borrow().compute_world_bounds_rect()
    }

    fn draw_single_ball(&self, backend: &mut dyn DrawingBackend, ball: &Ball2D) -> AppResult {
        let ball_coord_px = self.window_def.borrow().coord_to_px(ball.get_position());

        backend.draw_ball_mesh(
            ball.id,
            ball.size_idx,
            [ball_coord_px.x, ball_coord_px.y],
            false,
        )?;

        Ok(())
    }

    fn draw_balls_from_snapshot_data(
        &self,
        backend: &mut dyn DrawingBackend,
        snapshot: &crate::application::renderer_snapshot::RendererSnapshot,
    ) -> AppResult {
        // Draw balls from snapshot data
        for ball in &snapshot.balls {
            let ball_coord_px = self.window_def.borrow().coord_to_px(&ball.position);

            backend.draw_ball_mesh(
                ball.id,
                ball.size_idx,
                [ball_coord_px.x, ball_coord_px.y],
                false,
            )?;
        }

        Ok(())
    }
}

impl GgezRenderer {
    // Move the cache out without cloning (leaves an empty/default cache behind).
    // Ensure GgezMeshCache implements Default (or create a proper empty value).
    pub fn take_cache(&mut self) -> GgezMeshCache {
        self.string_elems = self.cache.text_cache.len();
        self.glyph_elems = self.cache.glyph_cache.len();
        self.ball_elems = self.cache.ball_label_cache.len();
        std::mem::take(&mut self.cache)
    }

    // Put cache back after use
    pub fn put_cache(&mut self, cache: GgezMeshCache) {
        self.cache = cache;
        self.string_elems = self.cache.text_cache.len();
        self.glyph_elems = self.cache.glyph_cache.len();
        self.ball_elems = self.cache.ball_label_cache.len();
    }

    fn create_ball_meshes(
        &self,
        ctx: &Context,
        ball_size: Vec<f32>,
        ball_physics: &BallPhysics,
    ) -> Vec<Mesh> {
        ball_size
            .iter()
            .map(|&size| {
                let eq_radius = ball_physics.equilibrium_of_radius(size);
                Self::create_ball_mesh(ctx, size, eq_radius, CIRCLE_TOLERANCE, STROKE_THICKNESS)
            })
            .collect()
    }

    /// Helper function to add data points to a line mesh
    fn add_data_to_line(
        win_def: Ref<WindowDefinition>,
        mesh_builder: &mut MeshBuilder,
        color: Color,
        data: &VecDeque<f32>,
    ) -> AppResult<()> {
        let coord_to_px = |x, y| win_def.coord_to_px(&Vec2D::new(x, y)).into();
        let mut points: Vec<Point2<f32>> = Vec::with_capacity(data.len());
        for (i, &value) in data.iter().enumerate() {
            points.push(coord_to_px(i as f32, value));
        }
        mesh_builder.polyline(DrawMode::stroke(2.0), &points, color)?;
        Ok(())
    }

    /// Returns a faint background color for a cell based on its row and column.
    pub(super) fn get_cell_color(row: usize, col: usize) -> Color {
        // 6 faint pastel colors
        const COLORS: [Color; 6] = [
            Color::new(0.8, 0.8, 1.0, 0.15), // faint blue
            Color::new(0.8, 1.0, 0.8, 0.15), // faint green
            Color::new(1.0, 0.8, 0.8, 0.15), // faint red
            Color::new(1.0, 1.0, 0.8, 0.15), // faint yellow
            Color::new(0.8, 1.0, 1.0, 0.15), // faint cyan
            Color::new(1.0, 0.8, 1.0, 0.15), // faint magenta
        ];
        let idx = (row + col) % COLORS.len();
        COLORS[idx]
    }

    /// Helper function to create the mesh for a ball
    fn create_ball_mesh(
        ctx: &Context,
        ball_size: f32,
        ball_eq_radius: f32,
        tolerance: f32,
        stroke_thickness: f32,
    ) -> Mesh {
        let mut builder = MeshBuilder::new();

        // Inner filled circle
        builder
            .circle(
                DrawMode::fill(),
                Vec2D::new(0.0, 0.0),
                WindowDefinition::len_to_px(ball_size),
                tolerance,
                Color::RED,
            )
            .unwrap();

        // Outer stroked circle
        builder
            .circle(
                DrawMode::stroke(stroke_thickness),
                Vec2D::new(0.0, 0.0),
                WindowDefinition::len_to_px(ball_eq_radius),
                tolerance,
                Color::MAGENTA,
            )
            .unwrap();

        // Outer stroked circle

        let cutoff = 4_f32;
        builder
            .circle(
                DrawMode::stroke(stroke_thickness),
                Vec2D::new(0.0, 0.0),
                WindowDefinition::len_to_px(ball_eq_radius + cutoff),
                tolerance,
                Color::from_rgb(110, 110, 110),
            )
            .unwrap();

        let ball_mesh_data = builder.build();

        Mesh::from_data(ctx, ball_mesh_data)
    }

    fn ball_color(&self, ball: &Ball2D) -> Option<Color> {
        if ball.verbose.console || ball.verbose.lines() {
            return Some(Color::BLUE);
        }
        if ball.braking {
            return Some(Color::YELLOW);
        }

        None
    }

    pub(crate) fn update_cursor_and_text(&mut self, x_px: f32, y_px: f32) -> Vec2D {
        let win_def = self.window_def.borrow();
        let clamped_px = win_def.clamp_to_bounds(&Vec2D::new(x_px, y_px));
        let curs_pos = clamped_px.clone();
        self.cursor_pos_px = [clamped_px.x, clamped_px.y];

        let curs_pos_coord = win_def.px_to_coord(clamped_px.x, clamped_px.y);

        let curs_moved_sq = curs_pos.dist(&self.curs_pos_of_txt).norm_sq();
        if curs_moved_sq > (f32::min(win_def.region_px.0, win_def.region_px.1) * 0.1) {
            self.curs_pos_of_txt = curs_pos.clone();
            self.txt_cursor = format!(
                "x: {}, y: {} pos ({:.3},{:.3})",
                clamped_px.x, clamped_px.y, curs_pos_coord.x, curs_pos_coord.y,
            );
        }
        curs_pos_coord
    }

    pub(crate) fn draw_line(
        builder: &mut MeshBuilder,
        start: Point2<f32>,
        end: Point2<f32>,
    ) -> &mut MeshBuilder {
        builder
            .line(&[start, end], LINE_THICKNESS, CROSSHAIR_COLOR)
            .unwrap()
    }

    pub(crate) fn rect_of_tuple(top_left: (f32, f32), format: (f32, f32)) -> ggez::graphics::Rect {
        ggez::graphics::Rect::new(top_left.0, top_left.1, format.0, format.1)
    }
}
