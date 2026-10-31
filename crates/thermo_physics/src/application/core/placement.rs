// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Collision check for placing many balls at once (#245).
//!
//! [`SimulationWorld::is_ball_colliding`] compares a candidate with every
//! ball, so seeding n balls costs O(n²). [`PlacementIndex`] gives the same
//! answer from the 3x3 cells around the candidate, plus the few balls the cell
//! grid does not hold at their actual position ("orphans": outside the grid,
//! or a stale cell entry). The answer is the same boolean, so seeding places
//! exactly the same balls and draws the same random numbers.
//!
//! Source-sphere emission (#250) runs right after the step's grid rebuild,
//! when the grid holds every ball at its position. It needs no index: the
//! orphans are the grid's balls right of or below it (`outside_balls`;
//! left of or above it they clamp into the edge cells) and the reach comes from the
//! species present ([`SimulationWorld::is_ball_colliding_in_grid`]).
//!
//! The lookup is exact only when every ball the candidate can collide with
//! lies in those 3x3 cells: a collision needs a centre distance below the
//! pair's contact distance plus the margin (the "reach";
//! `BallPhysics::contact_distance` follows pair overrides), and a ball two or
//! more cells away along an axis is at least one cell size away along it. When the reach
//! does not fit in a cell, or the candidate is outside the grid, the check
//! falls back to the full scan.

use super::SimulationWorld;
use crate::objects2d::ball_impl::{Ball2D, SpeciesSet};

/// Keep the reach this far below the cell size, so float rounding in the cell
/// index can never move a reachable ball out of the 3x3 cells.
const CELL_SLACK: f32 = 1e-4;

/// Built once per batch of placements; see the module docs.
pub(crate) struct PlacementIndex {
    /// Balls not found in the grid cell of their position (should not happen, defensive).
    orphans: Vec<usize>,
    /// Species of the balls in the world.
    present: SpeciesSet,
}

impl PlacementIndex {
    /// Index the world as it is; O(balls + cells).
    pub(crate) fn new(world: &SimulationWorld) -> Self {
        let grid = &world.cell_grid;
        let orphans = world
            .balls
            .iter()
            .enumerate()
            .filter(|(i, ball)| {
                let cell = grid.get_cell_indices(ball.get_position());
                cell.is_none() || grid.cell_of_ball(*i) != cell
            })
            .map(|(i, _)| i)
            .collect();
        let present = world.balls.iter().map(Ball2D::species).collect();
        Self { orphans, present }
    }

    /// Record ball `ball_idx`, just added through
    /// `SimulationWorld::add_ball_and_register`.
    pub(crate) fn note_added(&mut self, world: &SimulationWorld, ball_idx: usize) {
        let ball = &world.balls[ball_idx];
        self.present.insert(ball.species());
        // `add_ball_and_register` puts a ball into the cell of its position
        // when there is one; otherwise the grid does not hold it.
        if world
            .cell_grid
            .get_cell_indices(ball.get_position())
            .is_none()
        {
            self.orphans.push(ball_idx);
        }
    }
}

impl SimulationWorld {
    /// Same result as [`Self::is_ball_colliding`], using `index`.
    pub(crate) fn is_ball_colliding_indexed(
        &self,
        candidate: &Ball2D,
        margin: f32,
        index: &PlacementIndex,
    ) -> bool {
        self.is_ball_colliding_near(
            candidate,
            margin,
            index.present,
            index.orphans.iter().copied(),
        )
    }

    /// Same result as [`Self::is_ball_colliding`], from the grid alone, with
    /// no O(balls) pass: the balls outside the grid are its orphans and the
    /// species present bound the reach (#250). Exact only while the grid
    /// holds every ball in the cell of its position, which holds right after
    /// `CellGrid::rebuild` and the adds that follow it (source-sphere
    /// emission in the `cells` phase).
    /// The caller checks that once per batch (`emit_from_source_spheres`
    /// asserts `CellGrid::matches_positions` in debug builds); the adds of
    /// the batch keep it true.
    pub(crate) fn is_ball_colliding_in_grid(&self, candidate: &Ball2D, margin: f32) -> bool {
        let outside = self.cell_grid.outside_balls().iter().map(|&b| b as usize);
        self.is_ball_colliding_near(candidate, margin, self.species_present, outside)
    }

    /// The 3x3 cells around `candidate` plus `orphans` (balls the grid does
    /// not hold at their position); the full scan where that is not exact.
    fn is_ball_colliding_near(
        &self,
        candidate: &Ball2D,
        margin: f32,
        present: SpeciesSet,
        mut orphans: impl Iterator<Item = usize>,
    ) -> bool {
        let grid = &self.cell_grid;
        let reach = present
            .iter()
            .map(|s| self.physics.contact_distance(candidate.species(), s))
            .fold(0.0, f32::max)
            + margin;
        let (num_rows, num_cols) = grid.get_grid_size();
        let (cellsize_x, cellsize_y) = grid.get_cell_size();
        // A 3x3 window spans a whole axis of at most two cells.
        let cell_covers_ball_reach_area = |cell_size: f32, num_cells_in_direction: usize| {
            num_cells_in_direction <= 2 || reach < cell_size.abs() * (1.0 - CELL_SLACK)
        };
        let cell = grid.get_cell_indices(candidate.get_position());

        //check if candidate position is in a cell, i.e ==Some((row,col)) and if not do the slow is_ball_colliding
        //we also do the slow search if balls reach farther than the cell size (covers==false)
        let (Some((row, col)), true) = (
            cell,
            cell_covers_ball_reach_area(cellsize_x, num_cols)
                && cell_covers_ball_reach_area(cellsize_y, num_rows),
        ) else {
            return self.is_ball_colliding(candidate, margin);
        };

        let collides = |&ball_idx: &usize| {
            self.physics
                .is_colliding(candidate, &self.balls[ball_idx], margin)
        };
        let mut hit = false;
        //check neighbouring cells for collision
        grid.for_each_neighbor(row, col, |balls| {
            hit = hit || balls.iter().map(|&b| b as usize).any(|b| collides(&b));
        });
        //check orphans for collision (balls not on grid)
        hit || orphans.any(|b| collides(&b))
    }
}
