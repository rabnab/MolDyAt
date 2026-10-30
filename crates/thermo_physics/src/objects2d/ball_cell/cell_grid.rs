// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The cell grid as flat counting-sort arrays (P4, #345).
//!
//! Each step [`CellGrid::rebuild`] computes every ball's cell (in parallel)
//! and sorts the ball indices by cell with one counting sort: `sorted` holds
//! the indices cell by cell, ascending within a cell, and `cell_start[c]..
//! cell_start[c + 1]` is the range of cell `c`. There is no heap `Vec` per
//! cell and no per-ball move bookkeeping.
//!
//! Balls added between two rebuilds (placement, seeding, source spheres)
//! go into short per-cell lists (`late`) that every lookup also reads, so the
//! grid holds them right away; the next rebuild folds them into the sorted
//! arrays.

use crate::error::AppResult;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};
use rayon::prelude::*;
use std::fmt::Debug;

/// `cell_of` value of a ball outside the grid.
const NO_CELL: u32 = u32::MAX;

/// Where the cells are; `Copy`, so the parallel cell computation can use it
/// while the grid's arrays are written.
#[derive(Clone, Copy, Debug)]
struct GridGeometry {
    cell_size_x: f32,
    cell_size_y: f32,
    grid_width: usize,
    grid_height: usize,
    /// Top-left corner of the grid in world coordinates.
    origin: (f32, f32),
}

impl GridGeometry {
    fn cell_indices(&self, position: &Vec2D) -> Option<(usize, usize)> {
        let col = ((position.x - self.origin.0) / self.cell_size_x) as usize;
        let row = ((self.origin.1 - position.y) / self.cell_size_y) as usize;
        (col < self.grid_width && row < self.grid_height).then_some((row, col))
    }

    fn flat_cell(&self, position: &Vec2D) -> u32 {
        self.cell_indices(position)
            .map_or(NO_CELL, |(row, col)| (row * self.grid_width + col) as u32)
    }

    fn cell_count(&self) -> usize {
        self.grid_width * self.grid_height
    }
}

/// A spatial grid that partitions the simulation area into cells for
/// neighbour searches; see the module docs for its layout.
#[derive(Clone)]
pub struct CellGrid {
    geometry: GridGeometry,
    sim_width: f32,
    sim_height: f32,
    /// Neighbour-list radius (centre distance) the cells were sized for.
    list_radius: f32,
    /// Flat cell (`row * grid_width + col`) of each ball when it was last
    /// put into the grid, `NO_CELL` outside it.
    cell_of: Vec<u32>,
    /// `cell_start[c]..cell_start[c + 1]` is cell `c`'s range in `sorted`.
    cell_start: Vec<u32>,
    /// Ball indices sorted by cell, ascending within a cell.
    sorted: Vec<u32>,
    /// Scratch of the counting sort: the next free slot of each cell.
    fill: Vec<u32>,
    /// Balls added since the last rebuild, per cell. Empty lists do not
    /// allocate.
    late: Vec<Vec<u32>>,
    /// Cells with a non-empty `late` list, so a rebuild clears only those.
    late_cells: Vec<u32>,
}

impl CellGrid {
    /// Returns the world rectangle that the grid covers.
    pub fn world_rect(&self) -> Rect {
        let origin = self.geometry.origin;
        let sim_br = (origin.0 + self.sim_width, origin.1 + self.sim_height);
        Rect::from_tl_br(origin, sim_br)
    }

    /// Create an empty grid covering `world_rect` with cells at least
    /// `list_radius` wide: the largest centre distance at which two balls
    /// are neighbours (pair cutoff plus skin).
    pub(crate) fn new(world_rect: Rect, list_radius: f32) -> AppResult<Self> {
        let sim_width = world_rect.w;
        let sim_height = world_rect.h;

        // Cells at least as wide as the list radius make the 3x3 cells around
        // a ball cover every possible neighbour. Rounding the cell count down
        // keeps every cell at least this large.
        let cell_size = list_radius;
        let grid_width = ((sim_width.abs() / cell_size).floor() as usize).max(1);
        let grid_height = ((sim_height.abs() / cell_size).floor() as usize).max(1);

        let geometry = GridGeometry {
            cell_size_x: sim_width.abs() / grid_width as f32,
            cell_size_y: sim_height.abs() / grid_height as f32,
            grid_width,
            grid_height,
            origin: world_rect.get_tl(),
        };
        let cells = geometry.cell_count();
        Ok(Self {
            geometry,
            sim_width,
            sim_height,
            list_radius,
            cell_of: Vec::new(),
            cell_start: vec![0; cells + 1],
            sorted: Vec::new(),
            fill: Vec::new(),
            late: vec![Vec::new(); cells],
            late_cells: Vec::new(),
        })
    }

    /// Neighbour-list radius the cells were sized for.
    pub(crate) fn list_radius(&self) -> f32 {
        self.list_radius
    }

    pub fn get_cell_size(&self) -> (f32, f32) {
        (self.geometry.cell_size_x, self.geometry.cell_size_y)
    }

    /// Get the number of cells in the grid as (rows, columns).
    pub fn get_grid_size(&self) -> (usize, usize) {
        (self.geometry.grid_height, self.geometry.grid_width)
    }

    /// The cell (row, column) of a position, `None` outside the grid.
    pub(crate) fn get_cell_indices(&self, position: &Vec2D) -> Option<(usize, usize)> {
        self.geometry.cell_indices(position)
    }

    /// The cell the grid holds ball `ball_idx` in, `None` if it holds it in
    /// none (outside the grid, or not added since the last rebuild).
    pub(crate) fn cell_of_ball(&self, ball_idx: usize) -> Option<(usize, usize)> {
        let flat = *self.cell_of.get(ball_idx)?;
        let width = self.geometry.grid_width;
        (flat != NO_CELL).then(|| (flat as usize / width, flat as usize % width))
    }

    /// The cells of all balls, flat (`row * columns + col`), `u32::MAX` for
    /// a ball outside the grid. Indexed by ball.
    pub(crate) fn flat_cells(&self) -> &[u32] {
        &self.cell_of
    }

    /// Ball indices sorted by cell (ascending within a cell) as of the last
    /// rebuild; balls outside the grid are not in it.
    pub(crate) fn sorted_balls(&self) -> &[u32] {
        &self.sorted
    }

    /// Put every ball into the cell of its position, from scratch: the cells
    /// in parallel, then one counting sort in ball order. O(balls + cells).
    pub(crate) fn rebuild(&mut self, balls: &[Ball2D]) {
        let geometry = self.geometry;
        balls
            .par_iter()
            .map(|ball| geometry.flat_cell(ball.get_position()))
            .collect_into_vec(&mut self.cell_of);

        let cells = geometry.cell_count();
        self.cell_start.clear();
        self.cell_start.resize(cells + 1, 0);
        for &cell in &self.cell_of {
            if cell != NO_CELL {
                self.cell_start[cell as usize + 1] += 1;
            }
        }
        for c in 0..cells {
            self.cell_start[c + 1] += self.cell_start[c];
        }

        self.fill.clear();
        self.fill.extend_from_slice(&self.cell_start[..cells]);
        self.sorted.clear();
        self.sorted.resize(self.cell_start[cells] as usize, 0);
        for (ball_idx, &cell) in self.cell_of.iter().enumerate() {
            if cell != NO_CELL {
                let slot = &mut self.fill[cell as usize];
                self.sorted[*slot as usize] = ball_idx as u32;
                *slot += 1;
            }
        }

        for &cell in &self.late_cells {
            self.late[cell as usize].clear();
        }
        self.late_cells.clear();
    }

    /// Add ball `ball_idx`, just appended to the balls, to the cell of
    /// `position` without a rebuild.
    pub(crate) fn add_ball(&mut self, ball_idx: usize, position: &Vec2D) {
        let cell = self.geometry.flat_cell(position);
        if self.cell_of.len() <= ball_idx {
            self.cell_of.resize(ball_idx + 1, NO_CELL);
        }
        self.cell_of[ball_idx] = cell;
        if cell != NO_CELL {
            let late = &mut self.late[cell as usize];
            if late.is_empty() {
                self.late_cells.push(cell);
            }
            late.push(ball_idx as u32);
        }
    }

    /// Calls `f` with the ball indices of every cell in the 3x3
    /// neighbourhood around (row, col), the cell itself included, clamped to
    /// the grid. A cell may be passed in two slices (sorted, then added
    /// since the last rebuild).
    #[inline]
    pub(crate) fn for_each_neighbor<F>(&self, row: usize, col: usize, mut f: F)
    where
        F: FnMut(&[u32]),
    {
        let (height, width) = (self.geometry.grid_height, self.geometry.grid_width);
        if height == 0 || width == 0 {
            return;
        }
        let r_max = (row + 1).min(height - 1);
        let c_min = col.saturating_sub(1);
        let c_max = (col + 1).min(width - 1);
        for r in row.saturating_sub(1)..=r_max {
            // The cells of one row are contiguous in `sorted`.
            let first = r * width + c_min;
            let last = r * width + c_max;
            let (start, end) = (self.cell_start[first], self.cell_start[last + 1]);
            f(&self.sorted[start as usize..end as usize]);
            if !self.late_cells.is_empty() {
                for cell in first..=last {
                    let late = &self.late[cell];
                    if !late.is_empty() {
                        f(late);
                    }
                }
            }
        }
    }

    /// Append the ball indices of the 3x3 neighbourhood of (row, col) to
    /// `out`. Does not clear `out` first.
    pub(crate) fn collect_ball_indices_into(&self, row: usize, col: usize, out: &mut Vec<usize>) {
        self.for_each_neighbor(row, col, |balls| {
            out.extend(balls.iter().map(|&b| b as usize));
        });
    }

    /// The ball indices of one cell, sorted ones first.
    #[cfg(test)]
    pub(crate) fn balls_in_cell(&self, row: usize, col: usize) -> impl Iterator<Item = usize> + '_ {
        let cell = row * self.geometry.grid_width + col;
        let (start, end) = (self.cell_start[cell], self.cell_start[cell + 1]);
        self.sorted[start as usize..end as usize]
            .iter()
            .chain(&self.late[cell])
            .map(|&b| b as usize)
    }
}

#[cfg(test)]
impl CellGrid {
    /// Address and capacity of the arrays a rebuild reuses.
    pub(crate) fn buffers(&self) -> [(usize, usize); 4] {
        let buffer = |p: *const u32, capacity: usize| (p as usize, capacity);
        [
            buffer(self.cell_of.as_ptr(), self.cell_of.capacity()),
            buffer(self.cell_start.as_ptr(), self.cell_start.capacity()),
            buffer(self.sorted.as_ptr(), self.sorted.capacity()),
            buffer(self.fill.as_ptr(), self.fill.capacity()),
        ]
    }
}

impl Debug for CellGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = &self.geometry;
        write!(
            f,
            "CellGrid: {}x{} cells, cell size: ({:.2}, {:.2}), sim size: ({:.2}, {:.2}), origin: ({:.2}, {:.2}), {} balls sorted, {} cells with added balls",
            g.grid_height,
            g.grid_width,
            g.cell_size_x,
            g.cell_size_y,
            self.sim_width,
            self.sim_height,
            g.origin.0,
            g.origin.1,
            self.sorted.len(),
            self.late_cells.len()
        )
    }
}
