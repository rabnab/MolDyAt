use crate::logging;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::{BallPhysics, Named, UPDATE_HOOD_CNT};
use crate::objects2d::vec_2d::Vec2D;

const UPDATE_FREQ: usize = 100000; //basically deactivate update by frequency

pub(crate) struct Verlet {
    pub(crate) neighbours: Vec<usize>,
    force_magnitudes: Vec<f32>,
    pot_energy: Vec<f32>,
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
            force_magnitudes: Vec::new(),
            pot_energy: Vec::new(),
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

    /// Add a neighbor to this ball's Verlet list.
    ///
    /// # Arguments
    /// * `neighbour_idx` - Index of ball to add as neighbor
    ///
    /// # Returns
    /// `true` if ball was already a neighbor, `false` if newly added
    pub fn add_neighbour(&mut self, neighbour_idx: usize) -> bool {
        let was_neighbour = self.neighbours.contains(&neighbour_idx);
        if !was_neighbour {
            self.neighbours.push(neighbour_idx);
        }
        was_neighbour
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
        self.neighbours.contains(&idx)
    }

    /// Remove a neighbor from this Verlet list.
    ///
    /// # Arguments
    /// * `remove_idx` - Index of ball to remove
    ///
    /// # Returns
    /// `true` if ball was removed, `false` if not in list
    pub fn remove(&mut self, remove_idx: usize) -> bool {
        if self.is_a_neighbour_idx(remove_idx) {
            self.neighbours.retain(|&idx| idx != remove_idx);
            return true;
        }
        false
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
        neighbours: &[usize],
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

        // Collect all candidate neighbours from the cell neighbors parameter
        let mut candidates_from_cells = Vec::new();
        for &neighbour_idx in neighbours {
            // Skip self
            if neighbour_idx == ball_idx {
                continue;
            }
            candidates_from_cells.push(neighbour_idx);
        }

        // Get the current neighbour list - these need to be checked too
        let old_neighbours = &balls[ball_idx].hood.neighbours;

        // Merge candidates: existing neighbours + new candidates from cells using HashSet for deduplication
        let mut all_candidates = old_neighbours.clone();
        all_candidates.extend(&candidates_from_cells);
        all_candidates.sort_unstable();
        all_candidates.dedup();

        // Now check distance for all candidates and update accordingly
        for &neighbour_idx in &all_candidates {
            // Explicitly skip self-reference to avoid issues
            if ball_idx == neighbour_idx {
                continue;
            }

            // Calculate distance using split_at to avoid unnecessary mutability
            let dist = if ball_idx < neighbour_idx {
                let (left, right) = balls.split_at(neighbour_idx);
                BallPhysics::distance_of_edges(&left[ball_idx], &right[0])
            } else {
                let (left, right) = balls.split_at(ball_idx);
                BallPhysics::distance_of_edges(&right[0], &left[neighbour_idx])
            };

            if dist < dist_limit {
                // Should be a neighbour
                let was_already_neighbour = balls[ball_idx].hood.add_neighbour(neighbour_idx);

                if !was_already_neighbour {
                    // Log using split_at to get immutable refs
                    if ball_idx < neighbour_idx {
                        let (left, right) = balls.split_at(neighbour_idx);
                        logging::log_neighbour_add(
                            &left[ball_idx],
                            &right[0],
                            dist * dist,
                            dist_limit * dist_limit,
                        );
                    } else {
                        let (left, right) = balls.split_at(ball_idx);
                        logging::log_neighbour_add(
                            &right[0],
                            &left[neighbour_idx],
                            dist * dist,
                            dist_limit * dist_limit,
                        );
                    }

                    // Update the inverse relationship: add ball_idx to the neighbour's list
                    balls[neighbour_idx].hood.add_neighbour(ball_idx);
                    // Mark the neighbour's hood as dirty too
                    balls[neighbour_idx].hood.hood_dirty = true;
                }
            } else {
                // Should NOT be a neighbour - remove if present
                let was_removed = balls[ball_idx].hood.remove(neighbour_idx);
                if was_removed {
                    // Also remove the inverse relationship
                    balls[neighbour_idx].hood.remove(ball_idx);
                    // Mark the neighbour's hood as dirty too
                    balls[neighbour_idx].hood.hood_dirty = true;
                }
            }
        }

        balls[ball_idx].hood.updates_since_refresh = 0;
        // Mark the hood as dirty since neighbors were updated
        balls[ball_idx].hood.hood_dirty = true;
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
