use crate::application::AppResult;
use crate::graphics::renderer::dummy_renderer::DummyRenderer;
use crate::graphics::window::window_definition::WindowDefinition;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::BallPhysics;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotContent {
    None,
    Ekin,
    Epot,
    Etot,
    All,
}

pub fn create_dummy_renderer(win_def: WindowDefinition) -> impl Renderer {
    DummyRenderer {
        window_def: std::rc::Rc::new(std::cell::RefCell::new(win_def)),
    }
}

pub trait DrawingBackend {
    /// Draws the cell grid as faint colored rectangles in the background.
    fn draw_cell_grid_background(
        &mut self,
        grid_structure: &CellGrid,
        windef: &WindowDefinition,
    ) -> AppResult;
    fn draw_boundary_frame(&mut self, margin_px: (f32, f32), region_px: (f32, f32)) -> AppResult;
    fn dimensions_of_text(&mut self, text: String, scale: f32) -> Option<crate::objects2d::Rect>;

    fn draw_text(
        &mut self,
        text: String,
        pos: [f32; 2],
        scale: f32,
        align_right: bool,
        align_bottom: bool,
    ) -> AppResult;
    fn draw_ball_mesh(
        &mut self,
        id: usize,
        size_idx: usize,
        pos: [f32; 2],
        white: bool,
    ) -> AppResult;
    fn draw_cross_hair(&mut self, pos: [f32; 2]) -> AppResult;

    fn draw_line(
        &mut self,
        points: &[[f32; 2]; 2],
        width: f32,
        color: (u8, u8, u8, u8),
    ) -> AppResult;

    /// Begin batching lines for efficient rendering (clears previous batch)
    fn begin_line_batch(&mut self);

    /// Add a line to the current batch
    fn add_line_to_batch(
        &mut self,
        points: &[[f32; 2]; 2],
        width: f32,
        color: (u8, u8, u8, u8),
    ) -> AppResult;

    /// Draw all batched lines as a single mesh
    fn draw_line_batch(&mut self) -> AppResult;

    fn finish_frame(&mut self) -> AppResult;
}

pub trait Renderer {
    fn plot_type(&self) -> PlotContent;
    fn cycle_plot_content(&mut self);
    /// Adds the kinetic energy data points to the existing line mesh builder and creates the builder if it doesn't exist.
    fn add_vkin_to_line(&mut self, v_kin: &VecDeque<f32>) -> AppResult;
    fn add_vpot_to_line(&mut self, v_pot: &VecDeque<f32>) -> AppResult;
    fn add_vtot_to_line(&mut self, v_kin: &VecDeque<f32>, v_pot: &VecDeque<f32>) -> AppResult;

    /// Draw balls from snapshot (new threaded architecture)
    fn draw_balls_from_snapshot(
        &mut self,
        backend: &mut dyn DrawingBackend,
        snapshot: &crate::application::renderer_snapshot::RendererSnapshot,
    ) -> AppResult<()> {
        // Draw ball symbols and neighbor lines from snapshot
        self.draw_balls_from_snapshot_data(backend, snapshot)?;
        Ok(())
    }

    /// Helper to draw ball symbols from snapshot data
    fn draw_balls_from_snapshot_data(
        &self,
        backend: &mut dyn DrawingBackend,
        snapshot: &crate::application::renderer_snapshot::RendererSnapshot,
    ) -> AppResult;

    fn draw_cursor(&self, backend: &mut dyn DrawingBackend, size_idx: usize) -> AppResult;
    fn draw_plot(&mut self) -> AppResult;
    fn draw_frame(
        &mut self,
        backend: &mut dyn DrawingBackend,
        cell_grid: &CellGrid,
        status_text: Option<String>,
    ) -> AppResult;
    fn finish_frame(&mut self, backend: &mut dyn DrawingBackend) -> AppResult;
    fn disable_drawing_cursor(&mut self);
    fn enable_drawing_cursor(&mut self, repeated: bool);

    fn resize(&mut self, width: f32, height: f32);
    fn get_world_region(&self) -> crate::objects2d::Rect;

    fn set_physics_engine(&mut self, engine: BallPhysics);
}

pub(crate) fn is_number_continue(ch: char) -> bool {
    // Accept digits, decimal point, exponent letters and sign (+/-) (simple permissive DFA)
    matches!(ch, '0'..='9' | '.' | 'e' | 'E' | '+' | '-')
}

pub(crate) fn is_number_start(ch: char, next_opt: Option<char>) -> bool {
    // Start numbers for: '-' followed by digit or '.', a digit, or '.' followed by digit
    if ch == '-' {
        if let Some(n) = next_opt {
            return n.is_ascii_digit();
        }
        return false;
    }
    if ch == '.' {
        if let Some(n) = next_opt {
            return n.is_ascii_digit();
        }
        return false;
    }
    ch.is_ascii_digit()
}
