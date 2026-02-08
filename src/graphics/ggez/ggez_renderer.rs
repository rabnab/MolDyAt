use crate::application::AppResult;
use crate::graphics::ggez::WindowDefinitionRef;
use crate::graphics::ggez::batching::TailSegment;
use crate::graphics::ggez::ggez_renderer_backend::{DrawingBackend, GgezMeshCache};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::batch_renderer::{
    BatchRenderer, LineSegment, collect_neighbor_lines,
};
use crate::graphics::renderer::state_layers::{CacheStats, FrameState};
use crate::graphics::window::window_definition::WindowDefinition;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics};
use ggez::Context;
use ggez::graphics::{Color, DrawMode, Mesh, MeshBuilder};
use ggez::mint::Point2;
use std::cell::Ref;
use std::collections::VecDeque;

// Re-export PlotContent from state_layers so existing callers that
// imported it from ggez_renderer still work.
pub use crate::graphics::renderer::state_layers::PlotContent;

/// Ball visual types for rendering
pub enum BallType {
    Cursor,
    Normal,
    HoodUpdated,
}

pub(crate) const CIRCLE_TOLERANCE: f32 = 0.1; // Tolerance for circle drawing
pub(crate) const STROKE_THICKNESS: f32 = 2.0; // Border thickness for stroke drawing
pub(super) const CROSSHAIR_SIZE: f32 = 10.0;
const LINE_THICKNESS: f32 = 2.0;
const CROSSHAIR_COLOR: Color = Color::WHITE;

pub struct GgezRenderer {
    /// Per-frame transient state.
    frame: FrameState,
    /// GPU-agnostic cache metrics.
    cache_stats: CacheStats,

    // ── Backend-specific resources ──────────────────────────────────
    pub(crate) window_def: WindowDefinitionRef,
    cache: GgezMeshCache,
}

impl GgezRenderer {
    /// Creates a new GgezRenderer instance
    /// # Arguments
    /// * `plot_type` - The type of plot to display
    /// * `crosshair_mesh` - Pre-created mesh for the crosshair
    /// * `ball_meshes` - Pre-created meshes for balls (normal, cursor, hood updated)
    /// * `window_def` - Reference to the window definition used for rendering
    pub(crate) fn new(
        _plot_type: PlotContent,
        crosshair_mesh: Mesh,
        ball_meshes: (Vec<Mesh>, Vec<Mesh>, Vec<Mesh>),
        window_def: WindowDefinitionRef,
    ) -> Self {
        let cache = GgezMeshCache {
            crosshair_mesh: Some(crosshair_mesh),
            ball_mesh: ball_meshes.0,
            cursor_mesh: ball_meshes.1,
            hood_updated_mesh: ball_meshes.2,
            ..Default::default()
        };

        let ball_text_pos = {
            let windef = window_def.borrow();
            [windef.margin_px.0 * 2.0 + windef.region_px.0, 0.0]
        };

        let frame = FrameState {
            ball_status_text_pos: Some(ball_text_pos),
            ..FrameState::default()
        };

        Self {
            frame,
            cache_stats: CacheStats::default(),
            window_def,
            cache,
        }
    }

    /// Get current cache statistics.
    pub fn cache_stats(&self) -> CacheStats {
        self.cache_stats
    }

    /// Adds the kinetic energy data points to the existing line mesh builder and creates the builder if it doesn't exist.
    pub fn add_vkin_to_line(&mut self, v_kin: &VecDeque<f32>) -> AppResult<()> {
        let win_def = self.window_def.borrow();

        let builder = self
            .cache
            .line_vkin_builder
            .get_or_insert_with(MeshBuilder::new);
        GgezRenderer::add_data_to_line(win_def, builder, Color::RED, v_kin)?;
        Ok(())
    }

    /// Add potential energy data points to the line plot.
    pub fn add_vpot_to_line(&mut self, v_pot: &VecDeque<f32>) -> AppResult<()> {
        let win_def = self.window_def.borrow();
        let builder = self
            .cache
            .line_vpot_builder
            .get_or_insert_with(MeshBuilder::new);
        Self::add_data_to_line(win_def, builder, Color::BLUE, v_pot)?;
        Ok(())
    }

    /// Add total energy (kinetic + potential) data points to the line plot.
    pub fn add_vtot_to_line(
        &mut self,
        v_kin: &VecDeque<f32>,
        v_pot: &VecDeque<f32>,
    ) -> AppResult<()> {
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

    pub fn draw_cursor(
        &self,
        backend: &mut dyn DrawingBackend,
        viewport: &ViewportTransform,
        draw_cursor: bool,
        size_idx: usize,
    ) -> AppResult {
        if draw_cursor {
            // Cursor should also scale with zoom
            let zoom_scale = viewport.zoom();
            backend.draw_ball_mesh(
                0,
                size_idx,
                self.frame.cursor_pos_px,
                BallType::Cursor,
                zoom_scale,
            )?;
        } else {
            backend.draw_cross_hair(self.frame.cursor_pos_px)?;
        }

        let win_def = self.window_def.borrow();

        // Draw cursor position text in bottom-right
        let txt_cursor_pos = [
            win_def.margin_px.0 * 2. + win_def.region_px.0,
            win_def.margin_px.1 * 2. + win_def.region_px.1,
        ];
        backend.draw_text(
            self.frame.cursor_text.clone(),
            txt_cursor_pos,
            20.,
            true,
            true,
        )?;

        // Draw zoom level indicator just above cursor position text
        let zoom = viewport.zoom();
        let zoom_text = if zoom >= 10.0 {
            format!("Zoom: {:.0}x", zoom)
        } else {
            format!("Zoom: {:.1}x", zoom)
        };
        let zoom_pos = [
            win_def.margin_px.0 * 2. + win_def.region_px.0,
            win_def.margin_px.1 * 2. + win_def.region_px.1 - 25., // 25px above cursor text
        ];
        backend.draw_text(zoom_text, zoom_pos, 20., true, true)?;

        Ok(())
    }

    /// Draw the simulation frame (grid background and boundary).
    ///
    /// # Arguments
    /// * `backend` - Drawing backend to render with
    /// * `viewport` - Viewport transformation for coordinate conversion
    /// * `grid_spec` - Cell grid specification for background
    pub fn draw_frame(
        &mut self,
        backend: &mut dyn DrawingBackend,
        viewport: &ViewportTransform,
        grid_spec: &crate::application::renderer_snapshot::CellGridSpec,
    ) -> AppResult<()> {
        let win_def = self.window_def.borrow();

        // 1) Draw cell grid and rectangle (short-lived &mut Canvas)
        backend.draw_cell_grid_background(grid_spec, &win_def, viewport)?;
        backend.draw_boundary_frame(win_def.margin_px, win_def.region_px)?;

        Ok(())
    }

    pub fn finish_frame(&mut self, backend: &mut dyn DrawingBackend) -> AppResult<()> {
        backend.finish_frame()
    }

    /// Resize renderer-specific state (window definition, caches, text positions).
    ///
    /// # Returns
    /// The updated world bounds rectangle after resize.
    pub fn resize(&mut self, width: f32, height: f32) -> crate::objects2d::Rect {
        self.cache.cell_mesh = None;
        let mut win_def = self.window_def.borrow_mut();
        win_def.resize(width, height);
        self.frame.ball_status_text_pos =
            Some([win_def.margin_px.0 * 2.0 + win_def.region_px.0, 0.0]);
        let world_bounds = win_def.compute_world_bounds_rect();
        drop(win_def);
        world_bounds
    }

    pub fn get_world_region(&self) -> crate::objects2d::Rect {
        self.window_def.borrow().compute_world_bounds_rect()
    }

    /// Reset frame state at the start of each render frame.
    ///
    /// Increments the frame counter. Call this once per frame before
    /// any drawing operations.
    pub fn reset_frame(&mut self) {
        self.frame.reset();
    }

    /// Draw balls from snapshot (new threaded architecture)
    pub fn draw_balls_from_snapshot(
        &mut self,
        backend: &mut dyn DrawingBackend,
        viewport: &ViewportTransform,
        snapshot: &crate::application::renderer_snapshot::RendererSnapshot,
    ) -> AppResult<()> {
        // Draw ball symbols and neighbor lines from snapshot
        self.draw_balls_from_snapshot_data(backend, viewport, snapshot)?;
        Ok(())
    }

    /// Helper to draw ball symbols from snapshot data.
    /// Renders balls, trails, and neighbor lines using batched drawing.
    fn draw_balls_from_snapshot_data(
        &mut self,
        backend: &mut dyn DrawingBackend,
        viewport: &ViewportTransform,
        snapshot: &crate::application::renderer_snapshot::RendererSnapshot,
    ) -> AppResult {
        // Batch all tail lines for efficient rendering.
        // Skip tail batching entirely if no ball has a tail.
        let has_any_tails = snapshot.balls.iter().any(|b| !b.tail.is_empty());
        if has_any_tails {
            let mut tail_batch: BatchRenderer<TailSegment> = BatchRenderer::new();
            for ball in &snapshot.balls {
                let tail_len = ball.tail.len();
                if tail_len > 1 {
                    // Limit the number of tail segments per ball to avoid excessive per-frame work.
                    // This takes only the most recent segments near the end of the tail.
                    let max_segments = 200usize.min(tail_len - 1);
                    let start_index = tail_len - (max_segments + 1);
                    for window in ball.tail[start_index..].windows(2) {
                        let start_pos = &window[0];
                        let end_pos = &window[1];
                        // Use unified viewport transformation
                        let start_px = viewport.world_to_px(start_pos);
                        let end_px = viewport.world_to_px(end_pos);
                        tail_batch.push(TailSegment {
                            points: [start_px, end_px],
                            width: 1.0,
                        });
                    }
                }
            }
            backend.flush_tail_batch(&mut tail_batch)?;
        }

        // Batch all neighbor lines for efficient rendering
        let mut line_batch: BatchRenderer<LineSegment> = BatchRenderer::new();
        collect_neighbor_lines(&snapshot.balls, &mut line_batch);
        backend.flush_line_batch(&mut line_batch, viewport)?;

        // Then, draw balls from snapshot data (on top of lines) with frustum culling
        for ball in &snapshot.balls {
            // Frustum culling: only render balls that are at least partially visible
            // Use ball radius for proper visibility testing
            let ball_radius = ball.radius;
            if !viewport.is_circle_visible(&ball.position, ball_radius) {
                continue; // Skip balls outside viewport
            }

            // Use unified viewport transformation
            let ball_coord_px = viewport.world_to_px(&ball.position);

            // Scale balls with zoom level
            let zoom_scale = viewport.zoom();

            backend.draw_ball_mesh(
                ball.id,
                ball.size_idx,
                ball_coord_px,
                if ball.update_counter > 0 {
                    BallType::HoodUpdated
                } else {
                    BallType::Normal
                },
                zoom_scale,
            )?;
        }

        Ok(())
    }
}

impl GgezRenderer {
    // Move the cache out without cloning (leaves an empty/default cache behind).
    // Ensure GgezMeshCache implements Default (or create a proper empty value).
    pub fn take_cache(&mut self) -> GgezMeshCache {
        self.update_cache_stats();
        std::mem::take(&mut self.cache)
    }

    // Put cache back after use
    pub fn put_cache(&mut self, cache: GgezMeshCache) {
        self.cache = cache;
        self.update_cache_stats();
    }

    /// Sync [`CacheStats`] from the current [`GgezMeshCache`].
    fn update_cache_stats(&mut self) {
        self.cache_stats = CacheStats {
            string_entries: self.cache.text_cache.len(),
            glyph_entries: self.cache.glyph_cache.len(),
            ball_label_entries: self.cache.ball_label_cache.len(),
        };
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

    /// Update cursor text display based on current cursor position.
    ///
    /// This is ggez-specific for rendering cursor coordinates.
    /// The actual cursor position tracking is handled by UnifiedRenderState.
    pub(crate) fn update_cursor_text(
        &mut self,
        viewport: &ViewportTransform,
        x_px: f32,
        y_px: f32,
    ) {
        let win_def = self.window_def.borrow();
        let clamped_px = win_def.clamp_to_bounds(&Vec2D::new(x_px, y_px));
        let curs_pos = clamped_px;
        self.frame.cursor_pos_px = [clamped_px.x, clamped_px.y];

        // Use unified viewport transformation (same as wgpu)
        let curs_pos_coord = viewport.px_to_world(&[clamped_px.x, clamped_px.y]);

        let origin = Vec2D::new(
            self.frame.cursor_text_origin_px[0],
            self.frame.cursor_text_origin_px[1],
        );
        let curs_moved_sq = curs_pos.dist(&origin).norm_sq();
        if curs_moved_sq > (f32::min(win_def.region_px.0, win_def.region_px.1) * 0.1) {
            self.frame.cursor_text_origin_px = [curs_pos.x, curs_pos.y];
            self.frame.cursor_text = format!(
                "x: {}, y: {} pos ({:.3},{:.3})",
                clamped_px.x, clamped_px.y, curs_pos_coord.x, curs_pos_coord.y,
            );
        }
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
