use crate::frame::application_log;
use crate::objects2d::ball_cell::cell::BallCell;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::{BallPhysics, Named, UPDATE_HOOD_CNT};
use crate::objects2d::vec_2d::Vec2D;
use std::collections::HashSet;

const UPDATE_FREQ: usize = 100000; //basically deactivate update by frequency

pub(crate) struct Verlet {
    neighbours: Vec<usize>,
    pub nearest: f32,
    pub farthest: f32,
    updates_frequency: usize,
    updates_since_refresh: usize,
    num_neighbours: usize,
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
        }
    }
}

impl Verlet {
    /// Creates a new Verlet integration system with default parameters
    pub fn new() -> Self {
        Self::default()
    }

    // /// Creates a new Verlet integration system with custom parameters
    // #[allow(unused_variables)]
    // pub fn new_with_params(
    //     interaction_radius: f32,
    //     max_neighbors: usize,
    //     update_frequency: usize,
    //     min_movement_threshold: f32,
    // ) -> Self {
    //     Verlet::default()
    // }

    pub fn get_neighbour_indices(&self) -> &[usize] {
        &self.neighbours
    }

    pub(crate) fn get_number_of_neighbors(&self) -> usize {
        self.num_neighbours
    }

    /// Retrieve the names of all valid neighbours from the ball vector.
    pub fn get_neighbour_names(&self, balls: &[BallCell]) -> Vec<String> {
        self.neighbours
            .iter()
            .filter_map(|&idx| balls.get(idx))
            .map(|ball_rc| ball_rc.borrow().get_name())
            .collect()
    }

    pub fn add_neighbour(&mut self, neighbour_idx: usize) -> bool {
        let was_neighbour = self.neighbours.contains(&neighbour_idx);
        if !was_neighbour {
            self.neighbours.push(neighbour_idx);
        }
        was_neighbour
    }

    fn clear(&mut self) {
        self.neighbours.clear();
        self.num_neighbours = 0;
    }

    pub fn is_a_neighbour_idx(&self, idx: usize) -> bool {
        self.neighbours.contains(&idx)
    }

    pub fn remove(&mut self, remove_idx: usize) -> bool {
        if self.is_a_neighbour_idx(remove_idx) {
            self.neighbours.retain(|&idx| idx != remove_idx);
            return true;
        }
        false
    }
    /// Returns true if the ball has moved more than the min_movement_threshold since last update.
    pub fn has_ball_moved_enough(ball: &Ball2D, dt: f32, min_movement_threshold: f32) -> bool {
        let travel_since_last_upd_sq = ball
            .get_position()
            .add_scaled(&ball.v, dt)
            .dist(&ball.last_update_hood_pos)
            .norm_sq();
        min_movement_threshold * min_movement_threshold < travel_since_last_upd_sq
    }

    pub fn needs_update_hood(&self, ball: &Ball2D, dt: f32, min_movement_threshold: f32) -> bool {
        if self.updates_since_refresh >= self.updates_frequency {
            return true;
        }
        Self::has_ball_moved_enough(ball, dt, min_movement_threshold)
    }

    pub fn update_hood<'neigh>(
        ball_idx: usize,
        dt: f32,
        neighbours: impl IntoIterator<Item = &'neigh BallCell>,
        cutoff: f32,
        min_movement_threshold: f32,
        balls: &mut [BallCell],
    ) {
        let dist_limit_by_v = 0.;

        let dist_limit_by_closest = 4. * cutoff;
        let dist_limit = f32::max(dist_limit_by_closest, dist_limit_by_v);

        let this_ball_needs_update = {
            let ball = balls[ball_idx].borrow();
            if ball.verbose.console {
                application_log::log_update_hood_start(&ball, cutoff);
            }
            ball.hood
                .needs_update_hood(&ball, dt, min_movement_threshold)
        };

        balls[ball_idx].borrow_mut().hood.updates_since_refresh += 1;

        if !this_ball_needs_update {
            return;
        }

        unsafe {
            UPDATE_HOOD_CNT += 1;
        }

        // Get ball position and ID for distance calculations
        let ball_id = {
            let mut ball = balls[ball_idx].borrow_mut();
            ball.last_update_hood_pos = ball.get_position().clone();
            ball.get_id()
        };

        // Collect all candidate neighbours from the cell neighbors parameter
        let mut candidates_from_cells = Vec::new();
        for cur_rc in neighbours {
            let cur_ref = cur_rc.borrow();
            let cur_id = cur_ref.get_id();

            // Skip self
            if cur_id == ball_id {
                continue;
            }

            // Find the index of this ball in the vector
            if let Some(neighbour_idx) = balls.iter().position(|b| b.borrow().get_id() == cur_id) {
                candidates_from_cells.push(neighbour_idx);
            }
        }

        // Get the current neighbour list - these need to be checked too
        let old_neighbours: Vec<usize> = balls[ball_idx].borrow().hood.neighbours.clone();

        // Merge candidates: existing neighbours + new candidates from cells using HashSet for deduplication
        let mut all_candidates: HashSet<usize> = old_neighbours.iter().copied().collect();
        all_candidates.extend(candidates_from_cells);
        
        // Convert back to Vec for iteration
        let all_candidates: Vec<usize> = all_candidates.into_iter().collect();

        // Now check distance for all candidates and update accordingly
        for &neighbour_idx in &all_candidates {
            // Explicitly skip self-reference to avoid double borrow panic
            if ball_idx == neighbour_idx {
                continue;
            }
            let dist = BallPhysics::distance_of_edges(
                &balls[ball_idx].borrow(),
                &balls[neighbour_idx].borrow(),
            );

            if dist < dist_limit {
                // Should be a neighbour
                let was_already_neighbour = balls[ball_idx]
                    .borrow_mut()
                    .hood
                    .add_neighbour(neighbour_idx);

                if !was_already_neighbour {
                    application_log::log_neighbour_add(
                        &balls[ball_idx].borrow(),
                        &balls[neighbour_idx].borrow(),
                        dist * dist,
                        dist_limit * dist_limit,
                    );

                    // Update the inverse relationship: add ball_idx to the neighbour's list
                    balls[neighbour_idx]
                        .borrow_mut()
                        .hood
                        .add_neighbour(ball_idx);
                }
            } else {
                // Should NOT be a neighbour - remove if present
                let was_removed = balls[ball_idx].borrow_mut().hood.remove(neighbour_idx);
                if was_removed {
                    // Also remove the inverse relationship
                    balls[neighbour_idx].borrow_mut().hood.remove(ball_idx);
                }
            }
        }

        balls[ball_idx].borrow_mut().hood.updates_since_refresh = 0;
    }

    pub fn update_closeness_stats(&mut self, ball_pos: &Vec2D, balls: &[BallCell]) {
        self.num_neighbours = self.neighbours.len();
        let (nearest_sq, farthest_sq) = self
            .neighbours
            .iter()
            .filter_map(|&idx| balls.get(idx))
            .map(|ball_rc| {
                let d = ball_pos.dist(&ball_rc.borrow().get_position()).norm_sq();
                d
            })
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), val| {
                (min.min(val), max.max(val))
            });

        self.nearest = if nearest_sq.is_finite() {
            f32::sqrt(nearest_sq)
        } else {
            f32::NAN
        };
        self.farthest = if nearest_sq.is_finite() {
            f32::sqrt(farthest_sq)
        } else {
            f32::NAN
        };
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
    use std::cell::RefCell;
    use std::rc::Rc;

    fn make_ball(id: usize, pos: Vec2D) -> Rc<RefCell<Ball2D>> {
        let b = Ball2D::new(id, pos, (0, 1.0));
        Rc::new(RefCell::new(b))
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
        v.update_closeness_stats(&balls[0].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 0);

        // Add a
        v.add_neighbour(0);
        v.update_closeness_stats(&balls[0].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 1);

        // Add b
        v.add_neighbour(1);
        v.update_closeness_stats(&balls[1].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 2);

        // Add c
        v.add_neighbour(2);
        v.update_closeness_stats(&balls[2].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 3);

        // Add b again (should not increment)
        v.add_neighbour(1);
        v.update_closeness_stats(&balls[2].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 3);

        // Remove b
        v.remove(1);
        v.update_closeness_stats(&balls[2].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 2);

        // Remove a
        v.remove(0);
        v.update_closeness_stats(&balls[2].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 1);

        // Remove c
        v.remove(2);
        v.update_closeness_stats(&balls[2].borrow().get_position(), &balls);
        assert_eq!(v.num_neighbours, 0);

        // Remove non-existent (should not go negative)
        v.remove(0);
        v.update_closeness_stats(&balls[2].borrow().get_position(), &balls);
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
        v.update_closeness_stats(&balls[0].borrow().get_position(), &balls);
        // num_neighbours should match number of valid neighbours
        let count = v.neighbours.len();
        assert_eq!(v.num_neighbours, count);

        // Remove one
        v.remove(0);
        v.update_closeness_stats(&balls[0].borrow().get_position(), &balls);
        let count = v.neighbours.len();
        assert_eq!(v.num_neighbours, count);
    }
}
