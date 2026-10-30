// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::logging;
#[cfg(test)]
use crate::objects2d::ball_impl::Named;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::{BallPhysics, UPDATE_HOOD_CNT};
use crate::objects2d::vec_2d::Vec2D;
use std::sync::atomic::Ordering;

const UPDATE_FREQ: usize = 100000; //basically deactivate update by frequency

/// A ball's rebuilt neighbour list (sorted by index) and the partners added
/// or removed against its previous list. Produced by `Verlet::rebuild`,
/// installed by `Verlet::apply_rebuild`.
pub(crate) struct HoodRebuild {
    ball_idx: usize,
    neighbours: Vec<usize>,
    /// New partners.
    added: Vec<usize>,
    removed: Vec<usize>,
}

pub(crate) struct Verlet {
    pub(crate) neighbours: Vec<usize>,
    pub nearest: f32,
    pub farthest: f32,
    updates_frequency: usize,
    pub(crate) updates_since_refresh: usize,
    pub(crate) num_neighbours: usize,
    pub(crate) hood_dirty: bool,
}

impl Default for Verlet {
    fn default() -> Verlet {
        Verlet {
            neighbours: Vec::new(),
            nearest: f32::NAN,
            farthest: f32::NAN,
            updates_frequency: UPDATE_FREQ,
            updates_since_refresh: UPDATE_FREQ, //initialize with same for instant refresh on creation
            num_neighbours: 0,
            hood_dirty: true, // Initialize as dirty so stats are computed on first frame
        }
    }
}

impl Verlet {
    /// Get slice of neighbor ball indices.
    ///
    /// # Returns
    /// Slice of indices into the main ball vector
    pub(crate) fn get_neighbour_indices(&self) -> &[usize] {
        &self.neighbours
    }

    pub(crate) fn get_number_of_neighbors(&self) -> usize {
        self.num_neighbours
    }

    /// Add a neighbor to this ball's Verlet list, keeping it sorted by index.
    ///
    /// # Arguments
    /// * `neighbour_idx` - Index of ball to add as neighbor
    ///
    /// # Returns
    /// `true` if ball was already a neighbor, `false` if newly added
    pub(crate) fn add_neighbour(&mut self, neighbour_idx: usize) -> bool {
        match self.neighbours.binary_search(&neighbour_idx) {
            Ok(_) => true,
            Err(pos) => {
                self.neighbours.insert(pos, neighbour_idx);
                false
            }
        }
    }

    /// Renumber the partners after the balls were permuted: ball `old` is
    /// now ball `new_index[old]`. The list stays sorted.
    pub(crate) fn remap(&mut self, new_index: &[usize]) {
        for idx in &mut self.neighbours {
            *idx = new_index[*idx];
        }
        self.neighbours.sort_unstable();
    }

    pub(crate) fn clear_neighbours(&mut self) {
        self.neighbours.clear();
        self.num_neighbours = 0;
    }

    /// Remove a neighbor from this Verlet list.
    ///
    /// # Arguments
    /// * `remove_idx` - Index of ball to remove
    ///
    /// # Returns
    /// `true` if ball was removed, `false` if not in list
    pub(crate) fn remove(&mut self, remove_idx: usize) -> bool {
        match self.neighbours.binary_search(&remove_idx) {
            Ok(pos) => {
                self.neighbours.remove(pos);
                true
            }
            Err(_) => false,
        }
    }
    /// Check if ball has moved enough to require neighbor list update.
    /// Uses skin thickness threshold to prevent missing interactions.
    ///
    /// # Arguments
    /// * `ball` - Ball to check
    /// * `dt` - Time step (for velocity extrapolation)
    /// * `min_movement_threshold` - Distance threshold (skin/3, see `get_min_movement_for_update_hood`)
    ///
    /// # Returns
    /// `true` if ball moved beyond threshold since last update
    pub(crate) fn has_ball_moved_enough(
        ball: &Ball2D,
        dt: f32,
        min_movement_threshold: f32,
    ) -> bool {
        let travel_since_last_upd_sq = ball
            .get_position()
            .add_scaled(&ball.v, dt)
            .dist(&ball.last_update_hood_pos)
            .norm_sq();
        min_movement_threshold * min_movement_threshold < travel_since_last_upd_sq
    }

    /// Check if this ball's neighbor list needs rebuilding.
    /// Triggers update if refresh frequency reached or ball moved too far.
    ///
    /// # Arguments
    /// * `ball` - Ball to check
    /// * `dt` - Time step
    /// * `min_movement_threshold` - Movement threshold for update
    ///
    /// # Returns
    /// `true` if neighbor list should be rebuilt
    pub(crate) fn needs_update_hood(
        &self,
        ball: &Ball2D,
        dt: f32,
        min_movement_threshold: f32,
    ) -> bool {
        if self.updates_since_refresh >= self.updates_frequency {
            return true;
        }
        Self::has_ball_moved_enough(ball, dt, min_movement_threshold)
    }

    /// Per-step bookkeeping for one ball. Returns whether its list must be
    /// rebuilt; if so, records the rebuild position. Touches only `ball`, so
    /// it can run for all balls in parallel; callers log verbose balls.
    pub(crate) fn begin_update(ball: &mut Ball2D, dt: f32, min_movement_threshold: f32) -> bool {
        let needs_update = ball
            .cold
            .hood
            .needs_update_hood(ball, dt, min_movement_threshold);
        ball.cold.hood.updates_since_refresh += 1;
        ball.decrement_update_counter();
        if !needs_update {
            return false;
        }

        UPDATE_HOOD_CNT.fetch_add(1, Ordering::Relaxed);
        ball.last_update_hood_pos = *ball.get_position();
        ball.reset_update_counter();
        true
    }

    /// Build the new neighbour list of `ball_idx` and its changes against the
    /// current one. Reads `balls` only, so rebuilds of different balls can run
    /// in parallel.
    ///
    /// # Arguments
    /// * `candidates` - Balls in the 3x3 cells around the ball's cell
    /// * `physics` - Gives each pair's list radius (cutoff + skin), compared
    ///   with the centre distance
    pub(crate) fn rebuild(
        ball_idx: usize,
        candidates: &[usize],
        physics: &BallPhysics,
        balls: &[Ball2D],
    ) -> HoodRebuild {
        // The new list holds the candidates from the 3x3 cells within range.
        // Each ball sits in exactly one cell, so the candidates have no
        // duplicates, and the cell size guarantees that every ball in range is
        // among them (see `CellGrid::new`).
        let ball = &balls[ball_idx];
        let species = ball.species();
        let mut neighbours: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&neighbour_idx| {
                let other = &balls[neighbour_idx];
                let radius = physics.list_radius(species, other.species());
                neighbour_idx != ball_idx
                    && ball.get_position().dist(other.get_position()).norm_sq() < radius * radius
            })
            .collect();
        neighbours.sort_unstable();

        // Both lists are sorted, so one merge walk finds the added and removed
        // partners.
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut old_iter = ball.cold.hood.neighbours.iter().copied().peekable();
        for &neighbour_idx in &neighbours {
            while let Some(removed_idx) = old_iter.next_if(|&old_idx| old_idx < neighbour_idx) {
                removed.push(removed_idx);
            }
            if old_iter.next_if_eq(&neighbour_idx).is_none() {
                added.push(neighbour_idx);
            }
        }
        removed.extend(old_iter);

        HoodRebuild {
            ball_idx,
            neighbours,
            added,
            removed,
        }
    }

    /// Install a rebuilt list and update the partners that were added or
    /// removed. Partner updates are idempotent (a partner holds this ball
    /// exactly when this ball holds the partner, and both sides compute the
    /// same distance), so applying the rebuilds of one pass in any order gives
    /// the same lists.
    pub(crate) fn apply_rebuild(rebuild: HoodRebuild, physics: &BallPhysics, balls: &mut [Ball2D]) {
        let HoodRebuild {
            ball_idx,
            neighbours,
            added,
            removed,
        } = rebuild;
        for removed_idx in removed {
            Self::unlink_partner(balls, removed_idx, ball_idx);
        }
        for neighbour_idx in added {
            let (ball, partner) = (&balls[ball_idx], &balls[neighbour_idx]);
            if ball.verbose.console || partner.verbose.console {
                let dist_sq = ball.get_position().dist(partner.get_position()).norm_sq();
                let radius = physics.list_radius(ball.species(), partner.species());
                logging::log_neighbour_add(ball, partner, dist_sq, radius * radius);
            }
            let partner_hood = &mut balls[neighbour_idx].cold.hood;
            partner_hood.add_neighbour(ball_idx);
            partner_hood.hood_dirty = true;
        }

        let hood = &mut balls[ball_idx].cold.hood;
        hood.neighbours = neighbours;
        hood.updates_since_refresh = 0;
        // Mark the hood as dirty since neighbors were updated
        hood.hood_dirty = true;
    }

    /// Remove `ball_idx` from the list of its former neighbour `partner_idx`.
    fn unlink_partner(balls: &mut [Ball2D], partner_idx: usize, ball_idx: usize) {
        let partner_hood = &mut balls[partner_idx].cold.hood;
        partner_hood.remove(ball_idx);
        partner_hood.hood_dirty = true;
    }

    /// Compute neighbor statistics from a list of indices and ball positions.
    /// Returns (num_neighbours, nearest, farthest) as a tuple.
    pub(crate) fn compute_neighbor_stats(
        ball_pos: &Vec2D,
        neighbour_indices: &[usize],
        balls: &[Ball2D],
    ) -> (usize, f32, f32) {
        let num_neighbours = neighbour_indices.len();
        let (nearest_sq, farthest_sq) = neighbour_indices
            .iter()
            .filter_map(|&idx| balls.get(idx))
            .map(|ball| ball_pos.dist(ball.get_position()).norm_sq())
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), val| {
                (min.min(val), max.max(val))
            });

        let nearest = if nearest_sq.is_finite() {
            f32::sqrt(nearest_sq)
        } else {
            f32::NAN
        };
        let farthest = if farthest_sq.is_finite() {
            f32::sqrt(farthest_sq)
        } else {
            f32::NAN
        };

        (num_neighbours, nearest, farthest)
    }

    #[cfg(test)]
    pub(crate) fn update_closeness_stats(&mut self, ball_pos: &Vec2D, balls: &[Ball2D]) {
        let (num_neighbours, nearest, farthest) =
            Self::compute_neighbor_stats(ball_pos, &self.neighbours, balls);

        self.num_neighbours = num_neighbours;
        self.nearest = nearest;
        self.farthest = farthest;
    }

    /// Creates a new Verlet integration system with default parameters
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Retrieve the names of all valid neighbours from the ball vector.
    #[cfg(test)]
    pub(crate) fn get_neighbour_names(&self, balls: &[Ball2D]) -> Vec<String> {
        self.neighbours
            .iter()
            .filter_map(|&idx| balls.get(idx))
            .map(|ball| ball.get_name())
            .collect()
    }

    /// Check if a ball index is in this neighbor list.
    ///
    /// # Arguments
    /// * `idx` - Ball index to check
    ///
    /// # Returns
    /// `true` if ball is a neighbor
    #[cfg(test)]
    pub(crate) fn is_a_neighbour_idx(&self, idx: usize) -> bool {
        self.neighbours.binary_search(&idx).is_ok()
    }

    /// Rebuild one ball's neighbour list if it moved far enough, and update
    /// its partners' lists. The simulation runs the same steps in parallel
    /// (`SimulationEngine::update_neighbor_hoods_for_occupied_cells`); this
    /// serial form is kept for single-ball callers and tests.
    #[cfg(test)]
    pub(crate) fn update_hood(
        ball_idx: usize,
        dt: f32,
        candidates: &[usize],
        physics: &BallPhysics,
        min_movement_threshold: f32,
        balls: &mut [Ball2D],
    ) {
        if balls[ball_idx].verbose.console {
            logging::log_update_hood_start(&balls[ball_idx], physics.get_skin_thickness());
        }
        if !Self::begin_update(&mut balls[ball_idx], dt, min_movement_threshold) {
            return;
        }
        let rebuild = Self::rebuild(ball_idx, candidates, physics, balls);
        Self::apply_rebuild(rebuild, physics, balls);
    }
}

impl core::fmt::Debug for Verlet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_fmt(format_args!(
            "nearest/farthest {:.3e}/{:.3e} num neighbours {}",
            self.nearest,
            self.farthest,
            self.neighbours.len()
        ))
    }
}
