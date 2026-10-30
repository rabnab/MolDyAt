// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Buffers the simulation step reuses instead of allocating them every
//! step (#228).
//!
//! A step function borrows the buffers it needs through
//! [`SimulationWorld::with_scratch`], which takes them out of
//! [`StepScratch`] (no allocation), runs the function and always puts them
//! back, also on an early return. So their capacity carries over to the next
//! step. Their contents are only meaningful inside the function that filled
//! them; between steps they hold stale data, and no other code may read them.
//!
//! Allocations that remain on purpose:
//! - `Verlet::rebuild` builds each rebuilt ball's new neighbour list (it is
//!   installed into the ball) and its small added/removed lists.
//! - Parallel passes with `map_init(Vec::new, ..)` allocate one candidate
//!   buffer per rayon split.
//! - For balls with neighbour-line verbosity, the force pass fills a new
//!   `ForceResult::neighbor_forces` every step; it is dropped when the
//!   buffer is refilled the next step.
//! - Emission and placement collect the walls with `collect_solid_obstacles`;
//!   they only run when a sphere emits or a ball is added.

use super::SimulationWorld;
use super::line_segments::{Obstacle, OverlapPass};
use super::simulation_engine::ApplyStats;
use crate::objects2d::ball_impl::ForceResult;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::verlet::HoodRebuild;

/// A ball the neighbour rebuild looked at: (ball, cell row, cell column,
/// rebuild, verbose). `None` for balls outside the grid or with nothing to do.
pub(crate) type CheckedBall = Option<(usize, usize, usize, bool, bool)>;

/// Neighbour statistics of one ball: (ball, count, nearest, farthest, toggle
/// the "other neighbour" verbosity).
pub(crate) type NeighbourStats = (usize, usize, f32, f32, bool);

#[derive(Default)]
pub(crate) struct StepScratch {
    /// Buffers of `SimulationEngine::update_sim`.
    pub(crate) update: UpdateScratch,
    /// Buffers of the neighbour-list rebuild.
    pub(crate) rebuild: RebuildScratch,
    /// Neighbour statistics of each ball.
    pub(crate) neighbour_stats: Vec<NeighbourStats>,
}

#[derive(Default)]
pub(crate) struct UpdateScratch {
    /// Solid walls of the step (contact sweep and overlap pass).
    pub(crate) obstacles: Vec<Obstacle>,
    /// Wall contacts of each ball in the position update.
    pub(crate) bounce_infos: Vec<BounceInfo>,
    /// Wall distance checks of each ball in the contact sweep.
    pub(crate) sweep_checks: Vec<u64>,
    /// Overlap-pass result of each ball, summed serially in ball order.
    pub(crate) overlap_passes: Vec<OverlapPass>,
    /// Force results of this step. Swapped with
    /// `SimulationWorld::cached_force_results`, so the previous step's
    /// results become the next step's buffer.
    pub(crate) force_results: Vec<ForceResult>,
    /// Apply-pass stats of each chunk of balls, combined in chunk order.
    pub(crate) apply_stats: Vec<ApplyStats>,
}

#[derive(Default)]
pub(crate) struct RebuildScratch {
    /// What each ball needs (see [`CheckedBall`]).
    pub(crate) checked: Vec<CheckedBall>,
    /// (ball, cell row, cell column) of the rebuilding balls.
    pub(crate) to_rebuild: Vec<(usize, usize, usize)>,
    /// The new lists, installed in ball order.
    pub(crate) rebuilds: Vec<HoodRebuild>,
}

impl SimulationWorld {
    /// Run `f` with a scratch buffer taken out of `self.scratch`; the buffer
    /// is put back afterwards, also when `f` returns early.
    pub(crate) fn with_scratch<T: Default, R>(
        &mut self,
        field: fn(&mut StepScratch) -> &mut T,
        f: impl FnOnce(&mut Self, &mut T) -> R,
    ) -> R {
        let mut buffer = std::mem::take(field(&mut self.scratch));
        let result = f(self, &mut buffer);
        *field(&mut self.scratch) = buffer;
        result
    }
}
