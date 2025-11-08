#![allow(unused_variables)]

use crate::frame::AppResult;
use crate::frame::application_master::BallCell;
use crate::frame::renderer::{DrawingBackend, PlotContent, Renderer};
use crate::frame::window::window_definition::WindowDefinition;
use crate::objects2d::Rect;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics};
use std::cell::{Ref, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

pub struct DummyRenderer {
    pub(crate) window_def: Rc<RefCell<WindowDefinition>>,
}

impl Renderer for DummyRenderer {
    fn plot_type(&self) -> PlotContent {
        todo!()
    }

    fn cycle_plot_content(&mut self) {
        todo!()
    }

    fn add_vkin_to_line(&mut self, v_kin: &VecDeque<f32>) -> AppResult<()> {
        todo!()
    }

    fn add_vpot_to_line(&mut self, v_pot: &VecDeque<f32>) -> AppResult<()> {
        todo!()
    }

    fn add_vtot_to_line(&mut self, v_kin: &VecDeque<f32>, v_pot: &VecDeque<f32>) -> AppResult<()> {
        todo!()
    }
    fn draw_balls(
        &mut self,
        backend: &mut dyn DrawingBackend,
        balls: Vec<BallCell>,
    ) -> AppResult<()> {
        todo!()
    }

    fn draw_neighbour_lines(
        &self,
        backend: &mut dyn DrawingBackend,
        ball: &Ref<Ball2D>,
        balls: &[BallCell],
    ) -> AppResult {
        todo!()
    }

    fn draw_cursor(&self, backend: &mut dyn DrawingBackend, size_idx: usize) -> AppResult {
        todo!()
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
        todo!()
    }

    fn finish_frame(&mut self, backend: &mut dyn DrawingBackend) -> AppResult<()> {
        todo!()
    }

    fn disable_drawing_cursor(&mut self) {
        todo!()
    }

    fn enable_drawing_cursor(&mut self, repeated: bool) {
        todo!()
    }

    fn resize(&mut self, width: f32, height: f32) {
        self.window_def.borrow_mut().resize(width, height);
    }

    fn get_world_region(&self) -> Rect {
        self.window_def.borrow().compute_world_bounds_rect()
    }

    fn draw_single_ball(
        &self,
        backend: &mut dyn DrawingBackend,
        ball_cell: &BallCell,
    ) -> AppResult {
        todo!()
    }

    fn set_physics_engine(&mut self, engine: BallPhysics) {
        todo!()
    }
}
