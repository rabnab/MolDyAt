use crate::frame::ggez::ggez_renderer::GgezRenderer;
use crate::frame::renderer::{is_number_continue, is_number_start, DrawingBackend};
use crate::frame::window::window_definition::WindowDefinition;
use crate::frame::AppResult;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use ggez::glam::Vec2;
use ggez::graphics::{
    Canvas, Color, DrawMode, DrawParam, Drawable, Mesh, MeshBuilder, Rect, Text, TextFragment,
};
use ggez::mint::Point2;
use ggez::Context;
use lru::LruCache;
use std::num::NonZeroUsize;

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
        // use lookup in ball_label_cache.
        let txt_mesh = if let Some(t) = self.cache.ball_label_cache.get(&id) {
            t.clone()
        } else {
            //create a new one and cache it
            let t = Text::new(TextFragment::new(format!("{id}")).scale(15.0));
            self.cache.ball_label_cache.push(id, t.clone());
            t
        };

        self.canvas.as_mut().unwrap().draw(
            &txt_mesh,
            DrawParam::default().dest(ball_pos_px).offset([0.5, 0.5]), //for text to be centered
        );
        Ok(())
    }

    /// Get cached single-character glyph Text for given char and scale
    fn get_cached_glyph(&mut self, ch: char, scale: f32) -> Text {
        let scale_key = scale.to_bits();

        let key = (ch, scale_key);

        if let Some(tt) = self.cache.glyph_cache.get(&key) {
            tt.clone()
        } else {
            let t = Text::new(TextFragment::new(ch.to_string()).scale(scale));
            self.cache.glyph_cache.push(key, t.clone());
            t
        }
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
        grid_structure: &CellGrid,
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

        let (rows, cols) = grid_structure.get_grid_size();

        for row in 0..rows {
            for col in 0..cols {
                if let Some(cur_cell) = grid_structure.get_cell(row, col) {
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
        let s = grid_structure.get_cell_size();

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

    fn dimensions_of_text(&mut self, text: String, scale: f32) -> Option<crate::objects2d::Rect> {
        // Use a cache keyed by (text, scale_bits) to avoid re-creating Text
        let key = (text.clone(), scale.to_bits());
        if let Some(cached) = self.cache.text_cache.get(&key) {
            if let Some(ggez_rect) = cached.dimensions(self.ctx) {
                return Some(crate::objects2d::Rect {
                    x: ggez_rect.x,
                    y: ggez_rect.y,
                    w: ggez_rect.w,
                    h: ggez_rect.h,
                });
            }
            return None;
        }
        //if not found in cache, create and insert

        let txt_mesh = Text::new(TextFragment::new(text.clone()).scale(scale));
        let ggez_rect_opt = txt_mesh.dimensions(self.ctx);

        if let Some(ggez_rect) = ggez_rect_opt {
            // store in cache for reuse
            self.cache
                .text_cache
                .push((text, scale.to_bits()), txt_mesh.clone());
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
        align_bottom: bool,
    ) -> AppResult {
        // Tokenize per-line into text tokens and numeric tokens.
        // Text tokens are rendered using cached Text (text_cache).
        // Numeric tokens are rendered by composing cached single-character Text objects (glyph_cache).
        let scale_key = scale.to_bits();

        let lines: Vec<&str> = text.split('\n').collect();

        // Compute per-line widths first to handle alignment
        let mut lines_metrics: Vec<(f32, Vec<(bool, String)>)> = Vec::new();
        let sample = Text::new(TextFragment::new("M").scale(scale));
        let line_h = sample
            .dimensions(self.ctx)
            .map(|r| r.h)
            .unwrap_or(scale * 1.2);

        for line in &lines {
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0usize;
            let mut line_w = 0.0f32;
            let mut tokens_for_line: Vec<(bool, String)> = Vec::new(); // (is_text, token_str)

            while i < chars.len() {
                let c = chars[i];
                let next = if i + 1 < chars.len() {
                    Some(chars[i + 1])
                } else {
                    None
                };

                if is_number_start(c, next) {
                    // numeric token
                    let mut s = String::new();
                    s.push(c);
                    i += 1;
                    while i < chars.len() {
                        let nc = chars[i];
                        if is_number_continue(nc) {
                            s.push(nc);
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    // measure numeric token by summing per-char widths
                    for ch in s.chars() {
                        let char_txt = self.get_cached_glyph(ch, scale);
                        if let Some(r) = char_txt.dimensions(self.ctx) {
                            line_w += r.w;
                        }
                    }
                    tokens_for_line.push((false, s));
                } else {
                    // text token - accumulate until numeric start
                    let mut s = String::new();
                    s.push(c);
                    i += 1;
                    while i < chars.len() {
                        if is_number_start(
                            chars[i],
                            if i + 1 < chars.len() {
                                Some(chars[i + 1])
                            } else {
                                None
                            },
                        ) {
                            break;
                        }
                        s.push(chars[i]);
                        i += 1;
                    }
                    // measure text token using cached Texts keyed by (string,scale)
                    let key = (s.clone(), scale_key);
                    if let Some(t) = self.cache.text_cache.get(&key) {
                        if let Some(r) = t.dimensions(self.ctx) {
                            line_w += r.w;
                        }
                    } else {
                        let t = Text::new(TextFragment::new(s.clone()).scale(scale));
                        // Cache reasonably-sized text tokens to avoid unbounded cache growth
                        if s.len() <= 64 {
                            self.cache.text_cache.push(key, t.clone());
                        }
                        if let Some(r) = t.dimensions(self.ctx) {
                            line_w += r.w;
                        }
                    }
                    tokens_for_line.push((true, s));
                }
            }

            lines_metrics.push((line_w, tokens_for_line));
        }

        let total_h = line_h * lines.len() as f32;

        // Now draw each line with correct alignment
        for (li, (line_w, tokens)) in lines_metrics.iter().enumerate() {
            // initial x depends on alignment direction
            let mut x = if align_right { pos[0] - line_w } else { pos[0] };
            // If align_bottom interpret pos.y as bottom coordinate.
            // So top y should be pos.y - total_h. Otherwise treat pos.y as top.
            let y = if align_bottom {
                pos[1] - total_h + li as f32 * line_h
            } else {
                pos[1] + li as f32 * line_h
            };

            for (is_text, token) in tokens {
                if *is_text {
                    // draw whole token as Text
                    let key = (token.clone(), scale_key);
                    let txt = if let Some(t) = self.cache.text_cache.get(&key) {
                        t.clone()
                    } else {
                        let t = Text::new(TextFragment::new(token.clone()).scale(scale));
                        // Cache reasonably-sized tokens
                        if token.len() <= 64 {
                            self.cache.text_cache.push(key, t.clone());
                        }
                        t
                    };
                    let dim = txt
                        .dimensions(self.ctx)
                        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                    self.canvas
                        .as_mut()
                        .unwrap()
                        .draw(&txt, DrawParam::default().dest([x, y]));
                    x += dim.w;
                } else {
                    // numeric token: draw per-char cached Texts
                    for ch in token.chars() {
                        let char_txt = self.get_cached_glyph(ch, scale);
                        let dim = char_txt
                            .dimensions(self.ctx)
                            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                        self.canvas
                            .as_mut()
                            .unwrap()
                            .draw(&char_txt, DrawParam::default().dest([x, y]));
                        x += dim.w;
                    }
                }
            }
        }

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
        points: &[[f32; 2]; 2],
        width: f32,
        color: (u8, u8, u8, u8),
    ) -> AppResult {
        let ggez_color = Color::from(color);
        // Convert [f32; 2] to Point2<f32> for ggez
        let ggez_points: [Point2<f32>; 2] = [
            Point2 {
                x: points[0][0],
                y: points[0][1],
            },
            Point2 {
                x: points[1][0],
                y: points[1][1],
            },
        ];
        let line = Mesh::new_line(self.ctx, &ggez_points, width, ggez_color)?;
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

impl<'a> GgezBackend<'a> {}

pub struct GgezMeshCache {
    pub cell_mesh: Option<((f32, f32), Mesh)>,

    pub line_vkin_builder: Option<MeshBuilder>,
    pub line_vpot_builder: Option<MeshBuilder>,
    pub line_vtot_builder: Option<MeshBuilder>,
    pub(super) crosshair_mesh: Option<Mesh>,
    pub(super) ball_mesh: Vec<Mesh>,
    // caches for text to avoid re-allocating Text each frame
    // use u32 for scale key via f32::to_bits() to avoid f32 hashing issues
    pub text_cache: LruCache<(String, u32), Text>,
    // convenience cache for small per-ball labels keyed by id (scale is fixed in draw_text_on_ball)
    pub ball_label_cache: LruCache<usize, Text>,
    // glyph cache for single characters keyed by (char, scale_bits)
    pub glyph_cache: LruCache<(char, u32), Text>,
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
            text_cache: LruCache::new(NonZeroUsize::new(4096).unwrap()),
            ball_label_cache: LruCache::new(NonZeroUsize::new(500000).unwrap()),
            glyph_cache: LruCache::new(NonZeroUsize::new(200).unwrap()),
        }
    }
}
