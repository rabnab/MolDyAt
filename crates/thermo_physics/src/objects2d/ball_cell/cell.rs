// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fmt::Debug;

#[derive(Clone)]
pub(crate) struct Cell {
    pub balls: Vec<usize>,
    pub bounds: CellBounds,
}

#[derive(Clone, Copy)]
pub(crate) struct CellBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Debug for CellBounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cell x {{ {:.2}, {:.2} }}, y {{ {:.2}, {:.2} }}",
            self.min_x, self.max_x, self.min_y, self.max_y
        )
    }
}

impl Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "balls {} bounds: {:?}", self.balls.len(), self.bounds)
    }
}

impl Cell {
    /// An empty cell with spatial bounds `bounds`.
    pub(crate) fn new(bounds: CellBounds) -> Self {
        Self {
            balls: Vec::new(),
            bounds,
        }
    }

    /// Add ball index to this cell's list.
    ///
    /// # Arguments
    /// * `ball_idx` - Index into main balls vector
    pub(crate) fn add_ball(&mut self, ball_idx: usize) {
        self.balls.push(ball_idx);
    }

    /// Remove ball index from this cell's list.
    /// Uses retain to filter out matching index.
    ///
    /// # Arguments
    /// * `ball_id` - Index to remove
    pub(crate) fn remove_ball(&mut self, ball_id: usize) {
        self.balls.retain(|&idx| idx != ball_id);
    }

    pub(crate) fn get_ball_indices(&self) -> &[usize] {
        &self.balls
    }

    pub(crate) fn clear(&mut self) {
        self.balls.clear();
    }
}
