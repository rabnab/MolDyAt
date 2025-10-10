use crate::frame::AppResult;
use crate::frame::ggez::ggez_renderer::GgezRenderer;
use crate::frame::renderer::DrawingBackend;
use crate::frame::window::window_definition::WindowDefinition;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use ggez::Context;
use ggez::glam::Vec2;
use ggez::graphics::{
    Canvas, Color, DrawMode, DrawParam, Drawable, Mesh, MeshBuilder, Rect, Text, TextFragment,
};
use ggez::mint::Point2;

pub struct GgezBackend<'a> {
    ctx: &'a mut Context,
    canvas: Option<Canvas>,
    cache: GgezMeshCache,
}

impl<'a> GgezBackend<'a> {
    pub fn new(ctx: &'a mut Context, ggez_mesh_cache: GgezMeshCache) -> Self {
        let canvas = Some(Canvas::from_frame(ctx, Color::BLACK));

        Self {
            ctx,
            canvas,
            cache: ggez_mesh_cache,
        }
    }

    fn draw_text_on_ball(&mut self, id: usize, ball_pos_px: [f32; 2]) -> AppResult {
        let txt_name = Text::new(TextFragment::new(format!("{id}")).scale(15.));
        //TODO move this text to ball for performance

        self.canvas.as_mut().unwrap().draw(
            &txt_name,
            DrawParam::default().dest(ball_pos_px).offset([0.5, 0.5]), //for text to be centered
        );
        Ok(())
    }

    // Return the owned cache back to the caller after rendering
    pub fn into_cache(self) -> GgezMeshCache {
        self.cache
    }
}

impl<'a> DrawingBackend for GgezBackend<'a> {
    /// Draws the cell grid as faint colored rectangles in the background.
    fn draw_cell_grid_background(
        &mut self,
        grid: &CellGrid,
        win_def: &WindowDefinition,
    ) -> AppResult {
        {
            if let Some(mesh) = self.cache.cell_mesh.as_ref() {
                self.canvas
                    .as_mut()
                    .unwrap()
                    .draw(&mesh.1, DrawParam::default());
                return Ok(());
            }
        }

        let mut mesh_builder = MeshBuilder::new();

        let (rows, cols) = grid.get_grid_size();
        let field_size = WindowDefinition::len_to_px(1.0);

        for row in 0..rows {
            for col in 0..cols {
                if let Some(cur_cell) = grid.get_cell(row, col) {
                    // Alternate colors based on row + col
                    let color = GgezRenderer::get_cell_color(row, col);

                    let tl_px = win_def.coord_to_px(&cur_cell.get_top_left());
                    let br_px = win_def.coord_to_px(&cur_cell.get_bottom_right());

                    let rect = Rect::new(tl_px.x, tl_px.y, br_px.x - tl_px.x, br_px.y - tl_px.y);
                    mesh_builder.rectangle(DrawMode::fill(), rect, color)?;
                }
            }
        }

        let m = Mesh::from_data(self.ctx, mesh_builder.build());
        let s = grid.get_cell_size();

        self.cache.cell_mesh = Some(((s.0, s.1), m));

        Ok(())
    }

    fn draw_boundary_frame(&mut self, margin_px: (f32, f32), region_px: (f32, f32)) -> AppResult {
        let rect = Mesh::new_rectangle(
            self.ctx,
            DrawMode::stroke(3.0),
            GgezRenderer::rect_of_tuple(margin_px, region_px),
            Color::BLUE,
        )?;
        self.canvas
            .as_mut()
            .unwrap()
            .draw(&rect, Vec2::new(0.0, 0.0));
        Ok(())
    }

    fn dimensions_of_text(&self, text: String, scale: f32) -> Option<crate::objects2d::Rect> {
        let txt_mesh = Text::new(TextFragment::new(text).scale(scale));
        let ggez_rect_opt = txt_mesh.dimensions(self.ctx);

        if let Some(ggez_rect) = ggez_rect_opt {
            return Some(crate::objects2d::Rect {
                x: ggez_rect.x,
                y: ggez_rect.y,
                w: ggez_rect.w,
                h: ggez_rect.h,
            });
        };
        None
    }

    fn draw_text(
        &mut self,
        text: String,
        pos: [f32; 2],
        scale: f32,
        align_right: bool,
    ) -> AppResult {
        let txt_mesh = Text::new(TextFragment::new(text).scale(scale));
        let par = if align_right {
            DrawParam::default().dest(pos).offset(Vec2::new(1.0, 1.0))
        } else {
            DrawParam::default().dest(pos)
        };

        self.canvas.as_mut().unwrap().draw(&txt_mesh, par);
        Ok(())
    }

    fn draw_ball_mesh(
        &mut self,
        id: usize,
        size_idx: usize,
        pos: [f32; 2],
        white: bool,
    ) -> AppResult {
        let ball_mesh = &self.cache.ball_mesh;

        let draw_param = DrawParam::default()
            .dest(pos)
            //.scale(Vec2::new(radius, radius))
            ;
        self.canvas.as_mut().unwrap().draw(
            ball_mesh.get(size_idx).unwrap_or(&ball_mesh[0]),
            if white {
                draw_param.color(Color::WHITE)
            } else {
                draw_param
            },
        );

        self.draw_text_on_ball(id, pos)?;

        Ok(())
    }

    fn draw_cross_hair(&mut self, cursor_pos_px: [f32; 2]) -> AppResult {
        self.canvas.as_mut().unwrap().draw(
            self.cache
                .crosshair_mesh
                .as_ref()
                .expect("no crosshair set but already drawing"),
            cursor_pos_px,
        );
        Ok(())
    }

    fn draw_line(
        &mut self,
        points: &[Point2<f32>; 2],
        width: f32,
        color: (u8, u8, u8, u8),
    ) -> AppResult {
        let ggez_color = Color::from(color);
        let line = Mesh::new_line(self.ctx, points, width, ggez_color)?;
        self.canvas
            .as_mut()
            .unwrap()
            .draw(&line, DrawParam::default());
        Ok(())
    }

    fn finish_frame(&mut self) -> AppResult {
        let c = self.canvas.take().expect("canvas already taken");
        Ok(c.finish(self.ctx)?)
    }
}

pub struct GgezMeshCache {
    pub cell_mesh: Option<((f32, f32), Mesh)>,

    pub line_vkin_builder: Option<MeshBuilder>,
    pub line_vpot_builder: Option<MeshBuilder>,
    pub line_vtot_builder: Option<MeshBuilder>,
    pub(super) crosshair_mesh: Option<Mesh>,
    pub(super) ball_mesh: Vec<Mesh>,
}

impl Default for GgezMeshCache {
    fn default() -> Self {
        Self {
            cell_mesh: None,
            line_vkin_builder: None,
            line_vpot_builder: None,
            line_vtot_builder: None,
            crosshair_mesh: None,
            ball_mesh: vec![],
        }
    }
}
