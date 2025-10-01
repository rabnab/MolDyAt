use super::cell::{BallCell, Cell, CellBounds};
use crate::objects2d::Vec2D;
use ggez::Context;
use ggez::graphics::{Color, Rect};
use std::fmt::Debug;

pub struct CellGrid<'a> {
    cells: Vec<Vec<Cell<'a>>>,
    cell_size_x: f32,
    cell_size_y: f32,
    grid_width: usize,
    grid_height: usize,
    sim_width: f32,
    sim_height: f32,
    grid_origin: (f32, f32), // (top-left x, top-left y)
}

impl Debug for CellGrid<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "CellGrid: {}x{} cells, cell size: ({:.2}, {:.2}), sim size: ({:.2}, {:.2}), origin: ({:.2}, {:.2})",
            self.grid_height,
            self.grid_width,
            self.cell_size_x,
            self.cell_size_y,
            self.sim_width,
            self.sim_height,
            self.grid_origin.0,
            self.grid_origin.1
        )?;
        for (row_idx, row) in self.cells.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                writeln!(f, "Cell ({}, {}): {:?}", row_idx, col_idx, cell)?;
            }
        }
        Ok(())
    }
}

impl<'a> CellGrid<'a> {
    pub(crate) fn default() -> Self {
        Self {
            cells: Vec::new(),
            cell_size_x: 0.0,
            cell_size_y: 0.0,
            grid_width: 0,
            grid_height: 0,
            sim_width: 0.0,
            sim_height: 0.0,
            grid_origin: (0.0, 0.0),
        }
    }
}

impl<'a> CellGrid<'a> {
    pub fn new(world_rect: Rect, cutoff: f32, ctx: Option<&Context>) -> Self {
        let sim_width = world_rect.w;
        let sim_height = world_rect.h;
        let top_left_x = world_rect.x;
        let top_left_y = world_rect.y;

        let cell_size = 4.0 * cutoff;
        let grid_width = (sim_width.abs() / cell_size).ceil() as usize;
        let grid_height = (sim_height.abs() / cell_size).ceil() as usize;

        let cell_size_x = sim_width.abs() / grid_width as f32;
        let cell_size_y = sim_height.abs() / grid_height as f32;

        let mut cells = Vec::with_capacity(grid_height);
        for row in 0..grid_height {
            let mut cell_row = Vec::with_capacity(grid_width);
            for col in 0..grid_width {
                let bounds = CellBounds {
                    min_x: top_left_x + col as f32 * cell_size_x,
                    max_x: top_left_x + (col + 1) as f32 * cell_size_x,
                    min_y: top_left_y - row as f32 * cell_size_y,
                    max_y: top_left_y - (row + 1) as f32 * cell_size_y,
                };

                let color = Self::get_cell_color(row, col);
                cell_row.push(Cell::new(bounds, color, ctx));
            }
            cells.push(cell_row);
        }

        let grid = Self {
            cells,
            cell_size_x,
            cell_size_y,
            grid_width,
            grid_height,
            sim_width,
            sim_height,
            grid_origin: (top_left_x, top_left_y),
        };

        grid
    }

    pub(crate) fn get_grid_size(&self) -> (usize, usize) {
        (self.grid_height, self.grid_width)
    }

    pub fn get_cell_indices(&self, position: &Vec2D) -> Option<(usize, usize)> {
        let col = ((position.x - self.grid_origin.0) / self.cell_size_x) as usize;
        let row = ((self.grid_origin.1 - position.y) / self.cell_size_y) as usize;

        if col < self.grid_width && row < self.grid_height {
            Some((row, col))
        } else {
            None
        }
    }

    pub fn get_cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell<'a>> {
        self.cells.get_mut(row)?.get_mut(col)
    }

    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell<'a>> {
        self.cells.get(row)?.get(col)
    }

    pub fn move_ball(&mut self, ball: BallCell<'a>, old_pos: &Vec2D, new_pos: &Vec2D) {
        if let (Some((old_row, old_col)), Some((new_row, new_col))) = (
            self.get_cell_indices(old_pos),
            self.get_cell_indices(new_pos),
        ) {
            if old_row != new_row || old_col != new_col {
                let ball_id = ball.borrow().get_id();

                // Remove from old cell
                if let Some(old_cell) = self.get_cell_mut(old_row, old_col) {
                    old_cell.remove_ball(ball_id);
                }

                // Add to new cell
                if let Some(new_cell) = self.get_cell_mut(new_row, new_col) {
                    new_cell.add_ball(ball);
                }
            }
        }
    }

    pub fn add_ball(&mut self, ball: BallCell<'a>) {
        let position = &ball.borrow().get_position().clone();
        if let Some((row, col)) = self.get_cell_indices(position) {
            if let Some(cell) = self.get_cell_mut(row, col) {
                cell.add_ball(ball);
            }
        }
    }

    pub fn get_neighboring_cells(&self, row: usize, col: usize) -> Vec<&Cell<'a>> {
        let mut neighbors = Vec::new();

        for dr in -1..=1 {
            for dc in -1..=1 {
                let r = row as i32 + dr;
                let c = col as i32 + dc;

                if r >= 0 && c >= 0 {
                    if let Some(cell) = self.get_cell(r as usize, c as usize) {
                        neighbors.push(cell);
                    }
                }
            }
        }

        neighbors
    }

    pub fn clear_all_cells(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                cell.clear();
            }
        }
    }

    pub(crate) fn get_balls_in_neighborhood(&self, i: usize, j: usize) -> Vec<BallCell<'a>> {
        let mut balls = Vec::new();

        self.get_neighboring_cells(i, j)
            .into_iter()
            .for_each(|cell| {
                balls.extend(cell.get_balls().cloned());
            });
        balls
    }

    pub(crate) fn resize(
        old_grid: &mut CellGrid<'a>,
        new_world: Rect,
        cutoff: f32,
        ctx: Option<&Context>,
    ) -> Self {
        let mut new_grid = CellGrid::new(new_world, cutoff, ctx);

        for row in &mut old_grid.cells {
            for cell in row {
                for ball in cell.drain_balls() {
                    new_grid.add_ball(ball.clone());
                }
            }
        }

        new_grid
    }

    /// Returns a faint background color for a cell based on its row and column.
    pub fn get_cell_color(row: usize, col: usize) -> Color {
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
}
