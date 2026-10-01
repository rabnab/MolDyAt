use crate::logging;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::{BallPhysics, Named, UPDATE_HOOD_CNT};
use crate::objects2d::vec_2d::Vec2D;

const UPDATE_FREQ: usize = 100000; //basically deactivate update by frequency

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
    /// Creates a new Verlet integration system with default parameters
    pub fn new() -> Self {
        Self::default()
    }

    /// Get slice of neighbor ball indices.
    ///
    /// # Returns
    /// Slice of indices into the main ball vector
    pub fn get_neighbour_indices(&self) -> &[usize] {
        &self.neighbours
    }

    pub(crate) fn get_number_of_neighbors(&self) -> usize {
        self.num_neighbours
    }

    /// Retrieve the names of all valid neighbours from the ball vector.
    pub fn get_neighbour_names(&self, balls: &[Ball2D]) -> Vec<String> {
        self.neighbours
            .iter()
            .filter_map(|&idx| balls.get(idx))
            .map(|ball| ball.get_name())
            .collect()
    }

    /// Add a neighbor to this ball's Verlet list, keeping it sorted by index.
    ///
    /// # Arguments
    /// * `neighbour_idx` - Index of ball to add as neighbor
    ///
    /// # Returns
    /// `true` if ball was already a neighbor, `false` if newly added
    pub fn add_neighbour(&mut self, neighbour_idx: usize) -> bool {
        match self.neighbours.binary_search(&neighbour_idx) {
            Ok(_) => true,
            Err(pos) => {
                self.neighbours.insert(pos, neighbour_idx);
                false
            }
        }
    }

    pub fn clear_neighbours(&mut self) {
        self.neighbours.clear();
        self.num_neighbours = 0;
    }

    /// Check if a ball index is in this neighbor list.
    ///
    /// # Arguments
    /// * `idx` - Ball index to check
    ///
    /// # Returns
    /// `true` if ball is a neighbor
    pub fn is_a_neighbour_idx(&self, idx: usize) -> bool {
        self.neighbours.binary_search(&idx).is_ok()
    }

    /// Remove a neighbor from this Verlet list.
    ///
    /// # Arguments
    /// * `remove_idx` - Index of ball to remove
    ///
    /// # Returns
    /// `true` if ball was removed, `false` if not in list
    pub fn remove(&mut self, remove_idx: usize) -> bool {
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
    pub fn has_ball_moved_enough(ball: &Ball2D, dt: f32, min_movement_threshold: f32) -> bool {
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
    pub fn needs_update_hood(&self, ball: &Ball2D, dt: f32, min_movement_threshold: f32) -> bool {
        if self.updates_since_refresh >= self.updates_frequency {
            return true;
        }
        Self::has_ball_moved_enough(ball, dt, min_movement_threshold)
    }

    pub fn update_hood(
        ball_idx: usize,
        dt: f32,
        candidates: &[usize],
        cutoff_and_skin: f32,
        min_movement_threshold: f32,
        balls: &mut [Ball2D],
    ) {
        let dist_limit_by_v = 0.;

        // The search radius (Verlet radius) is cutoff + skin
        // Note: Since physics uses edge-to-edge distance, we don't need to add ball radii here
        // if 'cutoff' is the edge-distance cutoff.
        let effective_cutoff = cutoff_and_skin;
        let dist_limit_by_closest = effective_cutoff;
        let dist_limit = f32::max(dist_limit_by_closest, dist_limit_by_v);

        let this_ball_needs_update = {
            let ball = &balls[ball_idx];
            if ball.verbose.console {
                logging::log_update_hood_start(ball, cutoff_and_skin);
            }
            ball.hood
                .needs_update_hood(ball, dt, min_movement_threshold)
        };

        {
            let ball = &mut balls[ball_idx];
            ball.hood.updates_since_refresh += 1;
            ball.decrement_update_counter();
        }

        if !this_ball_needs_update {
            return;
        }

        unsafe {
            UPDATE_HOOD_CNT += 1;
        }

        // Get ball position and ID for distance calculations
        {
            let ball = &mut balls[ball_idx];
            ball.last_update_hood_pos = *ball.get_position();
            ball.reset_update_counter();
        };

        // The new list holds the candidates from the 3x3 cells within range.
        // Each ball sits in exactly one cell, so the candidates have no
        // duplicates, and the cell size guarantees that every ball in range is
        // among them (see `CellGrid::new`).
        let mut new_hood: Vec<(usize, f32)> = candidates
            .iter()
            .filter(|&&neighbour_idx| neighbour_idx != ball_idx)
            .map(|&neighbour_idx| {
                let dist = BallPhysics::distance_of_edges(&balls[ball_idx], &balls[neighbour_idx]);
                (neighbour_idx, dist)
            })
            .filter(|&(_, dist)| dist < dist_limit)
            .collect();
        new_hood.sort_unstable_by_key(|&(neighbour_idx, _)| neighbour_idx);

        // Both lists are sorted, so one merge walk finds the added and removed
        // pairs; only those touch the partner's list (which holds this ball
        // exactly when this ball holds the partner).
        let old_hood = std::mem::take(&mut balls[ball_idx].hood.neighbours);
        let mut old_iter = old_hood.iter().copied().peekable();
        for &(neighbour_idx, dist) in &new_hood {
            while let Some(removed_idx) = old_iter.next_if(|&old_idx| old_idx < neighbour_idx) {
                Self::unlink_partner(balls, removed_idx, ball_idx);
            }
            if old_iter.next_if_eq(&neighbour_idx).is_some() {
                continue; // kept
            }
            logging::log_neighbour_add(
                &balls[ball_idx],
                &balls[neighbour_idx],
                dist * dist,
                dist_limit * dist_limit,
            );
            let partner_hood = &mut balls[neighbour_idx].hood;
            partner_hood.add_neighbour(ball_idx);
            partner_hood.hood_dirty = true;
        }
        for removed_idx in old_iter {
            Self::unlink_partner(balls, removed_idx, ball_idx);
        }

        let mut neighbours = old_hood;
        neighbours.clear();
        neighbours.extend(new_hood.iter().map(|&(neighbour_idx, _)| neighbour_idx));
        balls[ball_idx].hood.neighbours = neighbours;

        balls[ball_idx].hood.updates_since_refresh = 0;
        // Mark the hood as dirty since neighbors were updated
        balls[ball_idx].hood.hood_dirty = true;
    }

    /// Remove `ball_idx` from the list of its former neighbour `partner_idx`.
    fn unlink_partner(balls: &mut [Ball2D], partner_idx: usize, ball_idx: usize) {
        let partner_hood = &mut balls[partner_idx].hood;
        partner_hood.remove(ball_idx);
        partner_hood.hood_dirty = true;
    }

    /// Compute neighbor statistics from a list of indices and ball positions.
    /// Returns (num_neighbours, nearest, farthest) as a tuple.
    pub fn compute_neighbor_stats(
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

    pub fn update_closeness_stats(&mut self, ball_pos: &Vec2D, balls: &[Ball2D]) {
        let (num_neighbours, nearest, farthest) =
            Self::compute_neighbor_stats(ball_pos, &self.neighbours, balls);

        self.num_neighbours = num_neighbours;
        self.nearest = nearest;
        self.farthest = farthest;
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
