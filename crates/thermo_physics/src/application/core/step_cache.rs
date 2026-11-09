// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the step derives from the world's balls or reuses between steps.
//! None of it is saved; all of it follows from `SimulationWorld::balls`.

use super::SimulationWorld;
use super::step_scratch::StepScratch;
use crate::application::AppResult;
use crate::application::phase_timings::PhaseTimings;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::simd_forces::KernelPoints;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, ForceResult, SpeciesSet};
use crate::objects2d::{Rect, Vec2D};

/// The step's derived and scratch data. Appending a ball updates it in place
/// (`SimulationWorld::add_ball_and_register`); every other change to the
/// ball list goes through [`SimulationWorld::balls_changed`]. The neighbour
/// lists are part of it too, but live in the balls (`Ball2D::cold`).
pub(crate) struct StepCache {
    pub(crate) cell_grid: CellGrid,
    /// Exactly the species of the balls in the world. The cell size follows
    /// the largest pair list radius among them.
    pub(crate) species_present: SpeciesSet,
    /// Per-ball position and species for the force kernel, refilled every step
    /// (same indices as `balls`). Valid only inside `update_sim`, from
    /// `update_old_positions` to `compute_forces`. Balls are removed and
    /// added between steps, so read `balls` everywhere else.
    pub(crate) kernel_points: KernelPoints,
    /// The last step's force results in ball order, for rendering; empty
    /// before the first step and after `balls_changed`.
    pub(crate) force_results: Vec<ForceResult>,
    /// Buffers the step reuses instead of allocating (see `StepScratch`).
    pub(crate) scratch: StepScratch,
    /// Time per step phase, for headless perf runs (#221).
    pub(crate) phase_timings: PhaseTimings,
}

impl StepCache {
    /// The cache of an empty world.
    pub(crate) fn new(world_rect: Rect, physics: &BallPhysics) -> AppResult<Self> {
        let species_present = SpeciesSet::default();
        Ok(Self {
            cell_grid: CellGrid::new(world_rect, physics.max_list_radius(species_present))?,
            species_present,
            kernel_points: Default::default(),
            force_results: Vec::new(),
            scratch: Default::default(),
            phase_timings: Default::default(),
        })
    }
}

impl SimulationWorld {
    /// Rebuild the cache after the ball list changed other than by appending
    /// a ball or sorting it (`reorder_balls_by_cell` renumbers the lists):
    /// every index is void, so all neighbour lists are cleared and rebuilt
    /// at the next neighbour pass.
    pub fn balls_changed(&mut self) {
        for ball in &mut self.balls {
            ball.cold.hood.clear_neighbours();
            ball.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        }
        self.cache.force_results.clear();
        self.rebuild_cell_grid();
    }

    /// Put every ball into the grid from scratch, with cells sized for the
    /// species now in the world.
    pub fn rebuild_cell_grid(&mut self) {
        let cache = &mut self.cache;
        cache.species_present = self.balls.iter().map(Ball2D::species).collect();
        let list_radius = self.physics.max_list_radius(cache.species_present);
        if list_radius != cache.cell_grid.list_radius() {
            cache.cell_grid = CellGrid::new(self.world_rect, list_radius)
                .expect("Failed to create cell grid for the species present");
        }
        cache.cell_grid.rebuild(&self.balls);
    }

    /// Rebuild the grid if the species added since the last rebuild need
    /// wider cells than it has, so the 3x3 cells around a ball still cover all
    /// its neighbours. Call only while every ball is registered in the cell of
    /// its position (not between a position update and the grid moves).
    pub(crate) fn fit_cell_grid(&mut self) {
        let cache = &self.cache;
        if self.physics.max_list_radius(cache.species_present) > cache.cell_grid.list_radius() {
            self.rebuild_cell_grid();
        }
    }
}
