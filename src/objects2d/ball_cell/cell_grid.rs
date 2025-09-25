use super::cell::{BallCell, Cell, CellBounds};
use crate::objects2d::Vec2D;
use ggez::graphics::Rect;

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

impl<'a> CellGrid<'a> {
    pub(crate) fn default() -> CellGrid<'a> {
        Self::new(Rect::new(0.0, 0.0, 100.0, 100.0), 10.0)
    }
}

impl<'a> CellGrid<'a> {
    pub fn new(world_rect: Rect, cutoff: f32) -> Self {
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
                cell_row.push(Cell::new(bounds));
            }
            cells.push(cell_row);
        }

        Self {
            cells,
            cell_size_x,
            cell_size_y,
            grid_width,
            grid_height,
            sim_width,
            sim_height,
            grid_origin: (top_left_x, top_left_y),
        }
    }

    pub(crate) fn get_grid_size(&self) -> (usize, usize) {
        (self.grid_height, self.grid_width)
    }

    pub fn get_cell_indices(&self, position: &Vec2D) -> Option<(usize, usize)> {
        let col = ((position.x - self.grid_origin.0) / self.cell_size_x) as usize;
        let row = ((position.y - self.grid_origin.1) / self.cell_size_y) as usize;

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

        for di in -1..=1 {
            for dj in -1..=1 {
                let ni = i as isize + di;
                let nj = j as isize + dj;

                if ni >= 0 && nj >= 0 {
                    if let Some(cell) = self.get_cell(ni as usize, nj as usize) {
                        balls.extend(cell.get_balls().cloned());
                    }
                }
            }
        }

        balls
    }

    pub(crate) fn resize(old_grid: &mut CellGrid<'a>, new_world: Rect, cutoff: f32) -> Self {
        let mut new_grid = CellGrid::new(new_world, cutoff);

        for row in &mut old_grid.cells {
            for cell in row {
                for ball in cell.drain_balls() {
                    new_grid.add_ball(ball.clone());
                }
            }
        }

        new_grid
    }
}
