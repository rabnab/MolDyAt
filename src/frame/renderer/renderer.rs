use crate::frame::AppResult;
use crate::frame::application_master::BallCell;
use crate::frame::renderer::dummy_renderer::DummyRenderer;
use crate::frame::window::window_definition::WindowDefinition;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics};
use ggez::mint::Point2;
use std::cell::Ref;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotContent {
    NONE,
    EKIN,
    EPOT,
    ETOT,
    ALL,
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
        grid: &CellGrid,
        windef: &WindowDefinition,
    ) -> AppResult;
    fn draw_boundary_frame(&mut self, margin_px: (f32, f32), region_px: (f32, f32)) -> AppResult;
    fn dimensions_of_text(&self, text: String, scale: f32) -> Option<crate::objects2d::Rect>;

    fn draw_text(
        &mut self,
        text: String,
        pos: [f32; 2],
        scale: f32,
        align_right: bool,
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
        points: &[Point2<f32>; 2],
        width: f32,
        color: (u8, u8, u8, u8),
    ) -> AppResult;

    fn finish_frame(&mut self) -> AppResult;
}

pub trait Renderer {
    fn plot_type(&self) -> PlotContent;
    fn cycle_plot_content(&mut self);
    /// Adds the kinetic energy data points to the existing line mesh builder and creates the builder if it doesn't exist.
    fn add_vkin_to_line(&mut self, v_kin: &VecDeque<f32>) -> AppResult;
    fn add_vpot_to_line(&mut self, v_pot: &VecDeque<f32>) -> AppResult;
    fn add_vtot_to_line(&mut self, v_kin: &VecDeque<f32>, v_pot: &VecDeque<f32>) -> AppResult;
    fn draw_balls(
        &mut self,
        backend: &mut dyn DrawingBackend,
        balls: Vec<BallCell>,
    ) -> AppResult<()> {
        let verbose_balls = get_verbose_balls(&balls);
        self.draw_verbose_balls(backend, &verbose_balls)?;
        // self.draw_all_ball_tails(backend, &balls)?; todo!()
        self.draw_all_balls_symbol(backend, &balls)?;

        Ok(())
    }

    fn draw_verbose_balls(
        &self,
        backend: &mut dyn DrawingBackend,
        verbose_balls: &[BallCell],
    ) -> AppResult {
        for ball_ref in verbose_balls.iter() {
            let ball = ball_ref.borrow();
            self.draw_neighbour_lines(backend, &ball)?;
        }
        Ok(())
    }
    fn draw_neighbour_lines(
        &self,
        backend: &mut dyn DrawingBackend,
        ball: &Ref<Ball2D>,
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
    fn draw_single_ball(&self, backend: &mut dyn DrawingBackend, ball_cell: &BallCell)
    -> AppResult;
    fn draw_all_balls_symbol(
        &self,
        backend: &mut dyn DrawingBackend,
        balls: &Vec<BallCell>,
    ) -> AppResult {
        for ball_cell in balls {
            self.draw_single_ball(backend, ball_cell)?;
        }
        Ok(())
    }

    fn set_physics_engine(&mut self, engine: BallPhysics);
}

fn get_verbose_balls(balls: &Vec<BallCell>) -> Vec<BallCell> {
    balls
        .iter()
        .filter(|ball| ball.borrow().verbose.lines())
        .cloned()
        .collect()
}
