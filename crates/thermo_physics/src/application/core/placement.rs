// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Collision check for placing many balls at once (#245).
//!
//! [`SimulationWorld::is_ball_colliding`] compares a candidate with every
//! ball, so seeding n balls costs O(n²). [`PlacementIndex`] gives the same
//! answer from the cells within reach of the candidate, plus the few balls the cell
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
//! The lookup is exact because every ball the candidate can collide with
//! lies in the cells searched: a collision needs a centre distance below the
//! pair's contact distance plus the margin (the "reach";
//! `BallPhysics::contact_distance` follows pair overrides), and a ball `n + 1`
//! or more cells away along an axis is at least `n` cell sizes away along it.
//! When the reach does not fit in a cell (large balls among small ones, whose
//! cells stay small), more cells are searched (`CellGrid::reach`). A candidate
//! outside the grid falls back to the full scan.

use super::SimulationWorld;
use crate::objects2d::ball_impl::Ball2D;

/// Keep the reach this far below a whole number of cells, so float rounding
/// in the cell index can never move a reachable ball out of the cells searched.
const CELL_SLACK: f32 = 1e-4;

/// Built once per batch of placements; see the module docs.
pub(crate) struct PlacementIndex {
    /// Balls not found in the grid cell of their position (should not happen, defensive).
    orphans: Vec<usize>,
}

impl PlacementIndex {
    /// Index the world as it is; O(balls + cells).
    pub(crate) fn new(world: &SimulationWorld) -> Self {
        let grid = &world.cache.cell_grid;
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
        Self { orphans }
    }

    /// Record ball `ball_idx`, just added through
    /// `SimulationWorld::add_ball_and_register`.
    pub(crate) fn note_added(&mut self, world: &SimulationWorld, ball_idx: usize) {
        let ball = &world.balls[ball_idx];
        // `add_ball_and_register` puts a ball into the cell of its position
        // when there is one; otherwise the grid does not hold it.
        if world
            .cache
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
        self.is_ball_colliding_near(candidate, margin, index.orphans.iter().copied())
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
        let outside = self.cache.cell_grid.outside_balls().iter();
        self.is_ball_colliding_near(candidate, margin, outside.map(|&b| b as usize))
    }

    /// The cells within reach of `candidate` plus `orphans` (balls the grid
    /// does not hold at their position); the full scan outside the grid.
    fn is_ball_colliding_near(
        &self,
        candidate: &Ball2D,
        margin: f32,
        mut orphans: impl Iterator<Item = usize>,
    ) -> bool {
        let grid = &self.cache.cell_grid;
        let reach = self
            .cache
            .species_present
            .iter()
            .map(|s| self.physics.contact_distance(candidate.species(), s))
            .fold(0.0, f32::max)
            + margin;
        // A candidate outside the grid takes the slow full scan.
        let Some((row, col)) = grid.get_cell_indices(candidate.get_position()) else {
            return self.is_ball_colliding(candidate, margin);
        };
        let cells = grid.reach(reach / (1.0 - CELL_SLACK));

        let collides = |&ball_idx: &usize| {
            self.physics
                .is_colliding(candidate, &self.balls[ball_idx], margin)
        };
        let mut hit = false;
        //check neighbouring cells for collision
        grid.for_each_neighbor(row, col, cells, |balls| {
            hit = hit || balls.iter().map(|&b| b as usize).any(|b| collides(&b));
        });
        //check orphans for collision (balls not on grid)
        hit || orphans.any(|b| collides(&b))
    }
}
