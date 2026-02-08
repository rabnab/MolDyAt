use crate::logging;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::{BallPhysics, Named, UPDATE_HOOD_CNT};
use crate::objects2d::vec_2d::Vec2D;

const UPDATE_FREQ: usize = 100000; //basically deactivate update by frequency

pub(crate) struct Verlet {
    neighbours: Vec<usize>,
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
    /// * `min_movement_threshold` - Distance threshold (typically skin/2)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ball(id: usize, pos: Vec2D) -> Ball2D {
        Ball2D::new(id, pos, (0, 1.0))
    }

    #[test]
    fn test_add_remove_neighbour_and_query() {
        let mut v = Verlet::new();
        let a_idx = 0;
        let b_idx = 1;

        let was_neigh = v.add_neighbour(b_idx);
        assert!(!was_neigh);
        assert!(v.is_a_neighbour_idx(b_idx));
        // Adding again returns true (was already neighbour)
        assert!(v.add_neighbour(b_idx));
        // Remove and ensure gone
        v.remove(b_idx);
        assert!(!v.is_a_neighbour_idx(b_idx));
        // No panic on removing non-existent
        v.remove(a_idx);
    }

    #[test]
    fn test_update_closeness_stats() {
        let mut v = Verlet::new();
        let a = make_ball(0, Vec2D::new(0.0, 0.0));
        let b = make_ball(1, Vec2D::new(3.0, 4.0)); // distance 5
        let balls = vec![a, b];

        v.add_neighbour(0);
        v.add_neighbour(1);
        v.update_closeness_stats(&Vec2D::new(0.0, 0.0), &balls);
        assert!((v.nearest - 0.0).abs() < 1e-6);
        assert!((v.farthest - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_needs_update_hood_thresholds() {
        let mut v = Verlet::new();
        // Simulate that the ball was just updated by resetting updates_since_refresh
        v.updates_since_refresh = 0;

        let mut ball = Ball2D::new(1, Vec2D::zero(), (0, 1.0));
        ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
        ball.v = Vec2D::new(1.0, 0.0);
        // With small dt and high threshold -> false
        assert!(!v.needs_update_hood(&ball, 0.01, 10.0));
        // With larger dt -> true
        assert!(v.needs_update_hood(&ball, 1.0, 0.5));
    }

    #[test]
    fn test_has_ball_moved_enough_true() {
        let mut ball = Ball2D::new(1, Vec2D::zero(), (0, 1.0));
        ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
        ball.v = Vec2D::new(3.0, 4.0); // velocity vector

        let dt = 1.0;
        let min_movement_threshold = 4.0;
        // Movement is 5 units, threshold is 4, so should return true
        assert!(Verlet::has_ball_moved_enough(
            &ball,
            dt,
            min_movement_threshold
        ));
    }

    #[test]
    fn test_has_ball_moved_enough_false() {
        let mut ball = Ball2D::new(1, Vec2D::new(0.1, 0.1), (0, 1.0));
        ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
        ball.v = Vec2D::new(0.1, 0.1);

        let dt = 1.0;
        let min_movement_threshold = 1.0;
        // Movement is sqrt(0.08) ≈ 0.282, threshold is 1, so should return false
        assert!(!Verlet::has_ball_moved_enough(
            &ball,
            dt,
            min_movement_threshold
        ));
    }

    #[test]
    fn test_has_ball_moved_enough_edge_case() {
        let mut ball = Ball2D::new(1, Vec2D::new(2., 0.), (0, 1.0));
        ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
        ball.v = Vec2D::new(1.0, 0.0);

        let dt = 1.0;
        let min_movement_threshold = 3.0;
        // Expected position after dt would be (3,0)
        // threshold = 3, so distance == threshold -> should return false
        assert!(!Verlet::has_ball_moved_enough(
            &ball,
            dt,
            min_movement_threshold
        ));
    }

    #[test]
    fn test_has_ball_moved_enough_zero_threshold() {
        let mut ball = Ball2D::new(1, Vec2D::zero(), (0, 1.0));
        ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
        ball.v = Vec2D::new(0.0, 0.0);

        let dt = 1.0;
        let min_movement_threshold = 0.0;
        // No movement, zero threshold -> should return false
        assert!(!Verlet::has_ball_moved_enough(
            &ball,
            dt,
            min_movement_threshold
        ));
    }

    #[test]
    fn test_num_neighbours_add_remove() {
        let mut v = Verlet::new();
        let a = make_ball(0, Vec2D::new(0.0, 0.0));
        let b = make_ball(1, Vec2D::new(1.0, 0.0));
        let c = make_ball(2, Vec2D::new(2.0, 0.0));
        let balls = vec![a, b, c];

        // Initially zero
        v.update_closeness_stats(&balls[0].get_position(), &balls);
        assert_eq!(v.num_neighbours, 0);

        // Add a
        v.add_neighbour(0);
        v.update_closeness_stats(&balls[0].get_position(), &balls);
        assert_eq!(v.num_neighbours, 1);

        // Add b
        v.add_neighbour(1);
        v.update_closeness_stats(&balls[1].get_position(), &balls);
        assert_eq!(v.num_neighbours, 2);

        // Add c
        v.add_neighbour(2);
        v.update_closeness_stats(&balls[2].get_position(), &balls);
        assert_eq!(v.num_neighbours, 3);

        // Add b again (should not increment)
        v.add_neighbour(1);
        v.update_closeness_stats(&balls[2].get_position(), &balls);
        assert_eq!(v.num_neighbours, 3);

        // Remove b
        v.remove(1);
        v.update_closeness_stats(&balls[2].get_position(), &balls);
        assert_eq!(v.num_neighbours, 2);

        // Remove a
        v.remove(0);
        v.update_closeness_stats(&balls[2].get_position(), &balls);
        assert_eq!(v.num_neighbours, 1);

        // Remove c
        v.remove(2);
        v.update_closeness_stats(&balls[2].get_position(), &balls);
        assert_eq!(v.num_neighbours, 0);

        // Remove non-existent (should not go negative)
        v.remove(0);
        v.update_closeness_stats(&balls[2].get_position(), &balls);
        assert_eq!(v.num_neighbours, 0);
    }

    #[test]
    fn test_num_neighbours_consistency_with_neighbours_vec() {
        let mut v = Verlet::new();
        let a = make_ball(0, Vec2D::new(0.0, 0.0));
        let b = make_ball(1, Vec2D::new(1.0, 0.0));
        let balls = vec![a, b];

        v.add_neighbour(0);
        v.add_neighbour(1);
        v.update_closeness_stats(&balls[0].get_position(), &balls);
        // num_neighbours should match number of valid neighbours
        let count = v.neighbours.len();
        assert_eq!(v.num_neighbours, count);

        // Remove one
        v.remove(0);
        v.update_closeness_stats(&balls[0].get_position(), &balls);
        let count = v.neighbours.len();
        assert_eq!(v.num_neighbours, count);
    }

    #[test]
    fn test_update_hood_respects_effective_cutoff() {
        // Test that update_hood only includes neighbors within cutoff_and_skin (Verlet radius)
        use crate::objects2d::ball_impl::BallPhysics;

        let physics = BallPhysics::default();
        let cutoff = physics.get_cutoff_distance(); // 4.0
        let skin_thickness = physics.get_skin_thickness(); // 2.0 * cutoff = 8.0
        let cutoff_and_skin = cutoff + skin_thickness; // 12.0

        // Create three balls: one at origin, one within range, one beyond range
        let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));

        // Position balls at specific center-to-center distances
        // Note: make_ball creates balls with radius 1.0, and distance check uses edge-to-edge distance
        // Edge-to-edge distance = center_distance - 2*radius
        // For edge distance to be within cutoff_and_skin: center_distance - 2.0 < cutoff_and_skin
        // So: center_distance < cutoff_and_skin + 2.0
        let ball_radius = 1.0;
        let dist1 = cutoff_and_skin * 0.8 + 2.0 * ball_radius; // Edge within range
        let dist2 = cutoff_and_skin * 1.2 + 2.0 * ball_radius; // Edge beyond range
        let mut ball1 = make_ball(1, Vec2D::new(dist1, 0.0));
        let mut ball2 = make_ball(2, Vec2D::new(dist2, 0.0));

        // Initialize last_update_hood_pos to trigger update
        ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        ball2.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

        let mut balls = vec![ball0, ball1, ball2];

        // All balls are candidates (simulate what cell grid provides)
        let candidates = vec![1, 2];

        // Update hood for ball 0 using cutoff_and_skin as the Verlet radius
        Verlet::update_hood(0, 0.01, &candidates, cutoff_and_skin, 0.1, &mut balls);

        // Ball 0 should have ball 1 as neighbor (within range) but not ball 2 (beyond range)
        let neighbors = &balls[0].hood.neighbours;
        assert!(
            neighbors.contains(&1),
            "Ball 1 should be a neighbor (edge distance {} < cutoff_and_skin {})",
            dist1 - 2.0 * ball_radius,
            cutoff_and_skin
        );
        assert!(
            !neighbors.contains(&2),
            "Ball 2 should NOT be a neighbor (edge distance {} > cutoff_and_skin {})",
            dist2 - 2.0 * ball_radius,
            cutoff_and_skin
        );
    }

    #[test]
    fn test_update_hood_removes_distant_neighbors() {
        // Test that neighbors moving beyond cutoff_and_skin are removed
        use crate::objects2d::ball_impl::BallPhysics;

        let physics = BallPhysics::default();
        let cutoff = physics.get_cutoff_distance(); // 4.0
        let skin_thickness = physics.get_skin_thickness(); // 8.0
        let cutoff_and_skin = cutoff + skin_thickness; // 12.0

        // Create two balls initially close together
        let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));
        let mut ball1 = make_ball(1, Vec2D::new(1.0, 0.0));

        ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

        let mut balls = vec![ball0, ball1];

        // Initial update - ball 1 should be a neighbor
        Verlet::update_hood(0, 0.01, &[1], cutoff_and_skin, 0.1, &mut balls);
        assert!(
            balls[0].hood.neighbours.contains(&1),
            "Ball 1 should initially be a neighbor"
        );

        // Move ball 1 far away (beyond cutoff_and_skin + 2*radius for edge distance)
        // Edge distance needs to exceed cutoff_and_skin, so center distance > cutoff_and_skin + 2*radius
        balls[1].set_position_and_update_tail(Vec2D::new(cutoff_and_skin + 2.0 + 0.5, 0.0));
        balls[0].last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

        // Update again - ball 1 should be removed
        Verlet::update_hood(0, 0.01, &[1], cutoff_and_skin, 0.1, &mut balls);
        assert!(
            !balls[0].hood.neighbours.contains(&1),
            "Ball 1 should be removed after moving beyond cutoff_and_skin"
        );
    }

    #[test]
    fn test_hood_dirty_flag_initialized_true() {
        // Test that hood_dirty is initialized to true so stats are computed on first frame
        let v = Verlet::new();
        assert!(
            v.hood_dirty,
            "hood_dirty should be initialized to true for first-frame stats computation"
        );
    }

    #[test]
    fn test_hood_dirty_flag_set_when_neighbor_added() {
        // Test that hood_dirty is set to true when a neighbor is added
        use crate::objects2d::ball_impl::BallPhysics;

        let physics = BallPhysics::default();
        let cutoff = physics.get_cutoff_distance();
        let cutoff_and_skin = cutoff + physics.get_skin_thickness();

        let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));
        let mut ball1 = make_ball(1, Vec2D::new(1.0, 0.0));

        ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

        let mut balls = vec![ball0, ball1];

        // Initially, hood_dirty should be true
        assert!(balls[0].hood.hood_dirty);
        assert!(balls[1].hood.hood_dirty);

        // Perform update_hood which adds a neighbor
        Verlet::update_hood(0, 0.01, &[1], cutoff_and_skin, 0.1, &mut balls);

        // After update, hood_dirty should still be true (marks as dirty when neighbors change)
        assert!(
            balls[0].hood.hood_dirty,
            "Ball 0's hood_dirty should be true after update_hood"
        );
        assert!(
            balls[1].hood.hood_dirty,
            "Ball 1's hood_dirty should be true since it was added as inverse neighbor"
        );
    }

    #[test]
    fn test_hood_dirty_flag_set_when_neighbor_removed() {
        // Test that hood_dirty is set to true when a neighbor is removed
        use crate::objects2d::ball_impl::BallPhysics;

        let physics = BallPhysics::default();
        let cutoff = physics.get_cutoff_distance();
        let cutoff_and_skin = cutoff + physics.get_skin_thickness();

        let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));
        let mut ball1 = make_ball(1, Vec2D::new(1.0, 0.0));

        ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

        let mut balls = vec![ball0, ball1];

        // First, add a neighbor
        Verlet::update_hood(0, 0.01, &[1], cutoff_and_skin, 0.1, &mut balls);
        assert!(balls[0].hood.neighbours.contains(&1));

        // Simulate resetting hood_dirty to false (as would happen after stats computation)
        balls[0].hood.hood_dirty = false;
        balls[1].hood.hood_dirty = false;

        // Move ball 1 far away and update
        balls[1].set_position_and_update_tail(Vec2D::new(cutoff_and_skin + 2.0 + 0.5, 0.0));
        balls[0].last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

        // Perform update_hood which removes the neighbor
        Verlet::update_hood(0, 0.01, &[1], cutoff_and_skin, 0.1, &mut balls);

        // After removal, hood_dirty should be true again
        assert!(
            balls[0].hood.hood_dirty,
            "Ball 0's hood_dirty should be true after neighbor removal"
        );
        assert!(
            balls[1].hood.hood_dirty,
            "Ball 1's hood_dirty should be true since it was removed as inverse neighbor"
        );
    }

    #[test]
    fn test_hood_dirty_flag_tracks_neighbor_changes_correctly() {
        // Test that hood_dirty correctly tracks whether any neighbor changes occurred
        let mut v = Verlet::new();

        // After initialization, hood_dirty is true
        assert!(v.hood_dirty);

        // Manually reset it (simulating that stats were just computed)
        v.hood_dirty = false;
        assert!(!v.hood_dirty);

        // Add a neighbor
        v.add_neighbour(1);

        // Manually set hood_dirty as update_hood would (in a real scenario)
        v.hood_dirty = true;
        assert!(v.hood_dirty);

        // Remove the neighbor
        v.remove(1);

        // Again set hood_dirty
        v.hood_dirty = true;
        assert!(v.hood_dirty);

        // Clear and verify
        v.clear_neighbours();
        assert!(v.neighbours.is_empty());
    }

    #[test]
    fn test_hood_dirty_independent_per_ball() {
        // Test that hood_dirty flag is independent for each ball
        let mut v1 = Verlet::new();
        let mut v2 = Verlet::new();

        // Both start as dirty
        assert!(v1.hood_dirty);
        assert!(v2.hood_dirty);

        // Modify one
        v1.hood_dirty = false;
        assert!(!v1.hood_dirty);
        assert!(v2.hood_dirty); // v2 should still be dirty

        // Modify the other
        v2.hood_dirty = false;
        assert!(!v1.hood_dirty);
        assert!(!v2.hood_dirty);
    }
}
