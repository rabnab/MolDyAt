use crate::frame::renderer::PlotContent;

use crate::frame::application_master::WindowDefinitionRef;
use crate::frame::ggez::ggez_renderer::{
    CIRCLE_TOLERANCE, CROSSHAIR_SIZE, GgezRenderer, STROKE_THICKNESS,
};
use crate::frame::window::window_definition::WindowDefinition;
use crate::objects2d::Vec2D;
use ggez::Context;
use ggez::graphics::{Color, DrawMode, Mesh, MeshBuilder};
use ggez::mint::Point2;

pub struct GgezRendererBuilder {
    show_plot: PlotContent,
    crosshair_mesh: Option<Mesh>,
    window_def_ref: Option<WindowDefinitionRef>,
    ball_mesh: Option<Vec<Mesh>>,
    // cell_mesh: Option<((f32, f32), Mesh)>,
    // neighbour_force_and_pos: Option<
    //     fn(&crate::objects2d::ball_impl::ball_core::Ball2D) -> Vec<(crate::objects2d::Vec2D, f32)>,
    // >,
}

impl Default for GgezRendererBuilder {
    fn default() -> Self {
        Self {
            show_plot: PlotContent::NONE,
            crosshair_mesh: None,
            ball_mesh: None,
            window_def_ref: None,
            // neighbour_force_and_pos: None,
            // cell_mesh: None,
        }
    }
}

impl GgezRendererBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show_plot(mut self, plot: PlotContent) -> Self {
        self.show_plot = plot;
        self
    }

    pub fn context(mut self, ctx: &Context, sizes: Vec<(f32, f32)>) -> Self {
        self.ball_mesh = initialize_ball_meshes(ctx, sizes);
        self.crosshair_mesh = initialize_crosshair_mesh(ctx);
        // self.cell_mesh = create_cell_grid_mesh(ctx, (10., 10.));
        self
    }

    pub(crate) fn window_def_ref(mut self, window_def_ref: WindowDefinitionRef) -> Self {
        self.window_def_ref = Some(window_def_ref);
        self
    }

    pub fn build(self) -> GgezRenderer {
        GgezRenderer::new(
            self.show_plot,
            self.crosshair_mesh.expect("CrosshairMesh not set"),
            self.ball_mesh.expect("BallMesh not set"),
            self.window_def_ref.expect("WindowDefRef not set"),
            // self.neighbour_force_and_pos.expect("neighbour_force_and_pos not set"),
        )
    }
}

fn create_cell_grid_mesh(ctx: &Context, cell_size: (f32, f32)) -> Option<((f32, f32), Mesh)> {
    None
    // let rect_mesh = Mesh::new_rectangle(
    //     ctx,
    //     DrawMode::fill(),
    //     ggez::graphics::Rect::new(0.0, 0.0, cell_size.0, cell_size.1),
    //     Color::WHITE,
    // )
    // .unwrap();
    // let m = ((cell_size.0, cell_size.1), rect_mesh);
    // Some(m)
}

fn initialize_ball_meshes(ctx: &Context, radius_equi: Vec<(f32, f32)>) -> Option<Vec<Mesh>> {
    let ball_meshes: Vec<Mesh> = radius_equi
        .iter()
        .map(|&radius| {
            create_ball_mesh(ctx, radius.0, radius.1, CIRCLE_TOLERANCE, STROKE_THICKNESS)
        })
        .collect();
    Some(ball_meshes)
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

fn initialize_crosshair_mesh(ctx: &Context) -> Option<Mesh> {
    let mut builder = MeshBuilder::new();

    // Draw horizontal and vertical lines
    GgezRenderer::draw_line(
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
    GgezRenderer::draw_line(
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
