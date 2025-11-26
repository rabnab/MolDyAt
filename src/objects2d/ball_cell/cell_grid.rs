use super::cell::{Cell, CellBounds};
use crate::frame::AppResult;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};
use std::fmt::Debug;

/// A spatial grid to partition the simulation area into cells for efficient neighbor searching.
/// Each cell contains a list of balls currently located within its bounds.
pub struct CellGrid {
    cells: Vec<Vec<Cell>>,
    cell_size_x: f32,
    cell_size_y: f32,
    grid_width: usize,
    grid_height: usize,
    sim_width: f32,
    sim_height: f32,
    /// Origin of the grid in world coordinates (top-left corner)
    grid_origin: (f32, f32), // (top-left x, top-left y)
}

impl CellGrid {
    /// Returns the world rectangle that the grid covers.
    pub(crate) fn world_rect(&self) -> Rect {
        let sim_br = (
            self.grid_origin.0 + self.sim_width,
            self.grid_origin.1 + self.sim_height,
        );
        Rect::from_tl_br(self.grid_origin, sim_br)
    }
}

impl CellGrid {
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

impl CellGrid {
    /// Create a new CellGrid covering the specified world rectangle with cells sized based on the cutoff distance.
    pub fn new(world_rect: Rect, cutoff: f32) -> AppResult<Self> {
        let sim_width = world_rect.w;
        let sim_height = world_rect.h;
        let (top_left_x, top_left_y) = world_rect.get_tl();

        let cell_size = 2.5 * cutoff;
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

                cell_row.push(Cell::new(bounds, row, col, grid_width));
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

        Ok(grid)
    }

    pub(crate) fn get_cell_size(&self) -> (f32, f32) {
        (self.cell_size_x, self.cell_size_y)
    }

    /// Get the number of cells in the grid as (rows, columns).
    pub(crate) fn get_grid_size(&self) -> (usize, usize) {
        (self.grid_height, self.grid_width)
    }

    /// Given a position in world coordinates, return the corresponding cell indices (row, column).
    /// Returns None if the position is outside the grid bounds.
    pub fn get_cell_indices(&self, position: &Vec2D) -> Option<(usize, usize)> {
        let col = ((position.x - self.grid_origin.0) / self.cell_size_x) as usize;
        let row = ((self.grid_origin.1 - position.y) / self.cell_size_y) as usize;

        if col < self.grid_width && row < self.grid_height {
            Some((row, col))
        } else {
            None
        }
    }

    pub fn get_cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(row)?.get_mut(col)
    }

    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.get(row)?.get(col)
    }

    /// Move a ball from its old position to a new position, updating the cells accordingly.
    /// Returns Some((old_cell, new_cell)) if the ball changed cells, None otherwise.
    pub fn move_ball(
        &mut self,
        ball_idx: usize,
        balls: &mut [Ball2D],
        old_pos: &Vec2D,
        new_pos: &Vec2D,
    ) -> Option<((usize, usize), (usize, usize))> {
        if let (Some((old_row, old_col)), Some((new_row, new_col))) = (
            self.get_cell_indices(old_pos),
            self.get_cell_indices(new_pos),
        ) && (old_row != new_row || old_col != new_col)
        {
            // Remove from old cell
            if let Some(old_cell) = self.get_cell_mut(old_row, old_col) {
                old_cell.remove_ball(ball_idx);
            }

            // Add to new cell
            if let Some(new_cell) = self.get_cell_mut(new_row, new_col) {
                new_cell.add_ball(ball_idx);
            }

            // Update the ball's current_cell
            balls[ball_idx].current_cell = Some((new_row, new_col));

            return Some(((old_row, old_col), (new_row, new_col)));
        }
        None
    }

    /// Add a ball to the appropriate cell based on its current position.
    pub fn add_ball(&mut self, ball_idx: usize, balls: &mut [Ball2D]) {
        let position = balls[ball_idx].get_position().clone();
        if let Some((row, col)) = self.get_cell_indices(&position)
            && let Some(cell) = self.get_cell_mut(row, col)
        {
            cell.add_ball(ball_idx);
            // Update the ball's current_cell
            balls[ball_idx].current_cell = Some((row, col));
        }
    }

    pub fn clear_all_cells(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                cell.clear();
            }
        }
        // Note: caller is responsible for clearing ball.current_cell if needed
    }

    /// Iterate over neighbor cells (including self) without allocating.
    #[inline]
    pub fn for_each_neighbor<F>(&self, row: usize, col: usize, mut f: F)
    where
        F: FnMut(&Cell),
    {
        if self.grid_height == 0 || self.grid_width == 0 {
            return;
        }

        let r_min = row.saturating_sub(1);
        let c_min = col.saturating_sub(1);
        let r_max = (row + 1).min(self.grid_height - 1);
        let c_max = (col + 1).min(self.grid_width - 1);

        for r in r_min..=r_max {
            let row_ref = &self.cells[r];
            for cell in row_ref.iter().take(c_max + 1).skip(c_min) {
                f(cell);
            }
        }
    }

    pub(crate) fn to_grid_structure(&self) -> Self {
        let empty_cells = self
            .cells
            .iter()
            .map(|row| row.iter().map(|c| c.empty_clone()).collect())
            .collect();
        Self {
            cells: empty_cells,
            cell_size_x: self.cell_size_x,
            cell_size_y: self.cell_size_y,
            grid_width: self.grid_width,
            grid_height: self.grid_height,
            sim_width: self.sim_width,
            sim_height: self.sim_height,
            grid_origin: self.grid_origin,
        }
    }

    /// Collect all ball indices from the 3x3 neighborhood into the provided `out` buffer.
    /// Caller should `clear()` the buffer if they want fresh contents each time.
    pub(crate) fn collect_ball_indices_into(&self, i: usize, j: usize, out: &mut Vec<usize>) {
        self.for_each_neighbor(i, j, |cell| {
            out.extend_from_slice(cell.get_ball_indices());
        });
    }

    /// Resize the grid to cover a new world rectangle, reassigning all existing balls to their new cells.
    pub(crate) fn resize(
        old_grid: &mut CellGrid,
        new_world: Rect,
        cutoff: f32,
        balls: &mut [Ball2D],
    ) -> AppResult<Self> {
        let mut new_grid = CellGrid::new(new_world, cutoff)?;

        for row in &mut old_grid.cells {
            for cell in row {
                for ball_idx in cell.drain_balls() {
                    new_grid.add_ball(ball_idx, balls);
                }
            }
        }

        Ok(new_grid)
    }
}

impl Debug for CellGrid {
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

impl Clone for CellGrid {
    fn clone(&self) -> Self {
        let nu_cells = self.cells.iter().map(|row| row.to_vec()).collect();
        Self {
            cells: nu_cells,
            cell_size_x: self.cell_size_x,
            cell_size_y: self.cell_size_y,
            grid_width: self.grid_width,
            grid_height: self.grid_height,
            sim_width: self.sim_width,
            sim_height: self.sim_height,
            grid_origin: self.grid_origin,
        }
    }
}
