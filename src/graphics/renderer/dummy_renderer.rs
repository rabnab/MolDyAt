#![allow(unused_variables)]

use crate::application::AppResult;
use crate::graphics::renderer::viewport::ViewportState;
use crate::graphics::renderer::{DrawingBackend, PlotContent, Renderer};
use crate::graphics::window::window_definition::WindowDefinition;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::BallPhysics;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

pub struct DummyRenderer {
    pub(crate) window_def: Rc<RefCell<WindowDefinition>>,
    pub(crate) viewport: ViewportState,
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

    fn draw_cursor(&self, backend: &mut dyn DrawingBackend, size_idx: usize) -> AppResult {
        todo!()
    }

    fn draw_plot(&mut self) -> AppResult<()> {
        todo!()
    }

    fn draw_frame(
        &mut self,
        backend: &mut dyn DrawingBackend,
        grid_spec: &crate::application::renderer_snapshot::CellGridSpec,
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
        let world_bounds = self.window_def.borrow().compute_world_bounds_rect();
        self.viewport.set_world_bounds(world_bounds);
    }

    fn get_world_region(&self) -> Rect {
        self.window_def.borrow().compute_world_bounds_rect()
    }

    fn draw_balls_from_snapshot_data(
        &self,
        backend: &mut dyn DrawingBackend,
        snapshot: &crate::application::renderer_snapshot::RendererSnapshot,
    ) -> AppResult {
        // Dummy implementation - does nothing
        Ok(())
    }

    fn set_physics_engine(&mut self, engine: BallPhysics) {
        todo!()
    }

    fn viewport_mut(&mut self) -> &mut ViewportState {
        &mut self.viewport
    }

    fn viewport(&self) -> &ViewportState {
        &self.viewport
    }
}
