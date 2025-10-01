use crate::frame::application_log;
use crate::objects2d::ball_cell::cell::BallCell;
use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::{BallId, BallPhysics, Named, UPDATE_HOOD_CNT};
use crate::objects2d::vec_2d::Vec2D;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

const UPDATE_FREQ: usize = 100;

pub(crate) struct Verlet<'a> {
    neighbours: Vec<Weak<RefCell<Ball2D<'a>>>>,
    pub nearest: f32,
    pub farthest: f32,
    updates_frequency: usize,
    updates_since_refresh: usize,
    num_neighbours: usize,
}

impl<'a> Default for Verlet<'a> {
    fn default() -> Verlet<'a> {
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

impl<'a> Verlet<'a> {
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

    pub fn get_neighbour_iter(&self) -> impl Iterator<Item = Rc<RefCell<Ball2D<'a>>>> {
        self.neighbours
            .iter()
            .filter_map(|weak| weak.upgrade().clone())
    }

    pub(crate) fn get_number_of_neighbors(&self) -> usize {
        self.num_neighbours
    }

    /// Retrieve the names of all valid neighbours.
    pub fn get_neighbour_names(&self) -> Vec<String> {
        self.neighbours
            .iter()
            .filter_map(|weak| weak.upgrade()) // Filter out dropped references
            .map(|rcrefcell| rcrefcell.borrow().get_name())
            .collect()
    }

    // fn add_neighbour_already_sure_was_no_neighbour(&self, ball_rc: Weak<RefCell<Ball2D<'a>>>) {
    //     self.neighbours.borrow_mut().push(ball_rc);
    // }
    pub fn add_neighbour(&mut self, ball_rc: &Rc<RefCell<Ball2D<'a>>>) -> bool {
        let was_neighbour = self.is_a_neighbour_id(ball_rc.borrow().get_id());
        if !was_neighbour {
            self.neighbours.push(Rc::downgrade(ball_rc));
            self.num_neighbours += 1;
        }
        was_neighbour
    }

    pub fn is_a_neighbour_id(&self, id_other: BallId) -> bool {
        self.neighbours
            .iter()
            .filter_map(|weak| weak.upgrade())
            .any(|rcrefcell| rcrefcell.borrow().id == id_other)
    }

    pub fn remove(&mut self, remove_id: BallId) -> bool {
        if self.is_a_neighbour_id(remove_id) {
            self.neighbours.retain(|weak| {
                weak.upgrade()
                    .is_some_and(|ball_rc| ball_rc.borrow().get_id() != remove_id)
            });
            self.num_neighbours -= 1;
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
        if self.updates_since_refresh > self.updates_frequency {
            return true;
        }
        Self::has_ball_moved_enough(ball, dt, min_movement_threshold)
    }

    pub fn update_hood<'b>(
        ball: &mut Ball2D<'a>,
        dt: f32,
        neighbours: impl IntoIterator<Item = &'b BallCell<'a>>,
        cutoff: f32,
        min_movement_threshold: f32,
    ) where
        'a: 'b,
    {
        let dist_limit_by_v = 0.;
        //we only check hood every ten updates --> factors 100 and 10
        //100. * v_sq * dt * dt + eq * eq + 2. * 10. * dt * eq * (self.v.x + self.v.y);
        let dist_limit_by_closest = 4. * cutoff;
        let dist_limit = f32::max(dist_limit_by_closest, dist_limit_by_v);
        if ball.verbose.console {
            application_log::log_update_hood_start(ball, cutoff);
        }
        ball.hood.updates_since_refresh += 1;
        let this_ball_needs_update = ball
            .hood
            .needs_update_hood(ball, dt, min_movement_threshold);
        if !this_ball_needs_update {
            return;
        }
        unsafe {
            UPDATE_HOOD_CNT += 1;
        }
        ball.last_update_hood_pos = ball.get_position().clone();
        for cur_rc in neighbours {
            let cur_ref = cur_rc.borrow();

            // let dist_sq = ball.get_position().dist(&cur_ref.get_position()).norm_sq();
            let dist = BallPhysics::distance_of_edges(&ball, &cur_ref);
            //println!("checking {}", cur_ref.get_name());
            if dist < dist_limit {
                ball.hood.add_neighbour(cur_rc);
                application_log::log_neighbour_add(
                    ball,
                    &*cur_ref,
                    dist * dist,
                    dist_limit * dist_limit,
                );
            } else {
                ball.hood.remove(cur_ref.get_id());
            }
        }
        ball.hood.updates_since_refresh = 0;
    }

    pub fn update_closeness_stats(&mut self, ball_pos: &Vec2D) {
        let (nearest_sq, farthest_sq) = self
            .neighbours
            .iter()
            .filter_map(|weak| weak.upgrade())
            .map(|val| {
                let d = ball_pos.dist(&val.borrow().get_position()).norm_sq();
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

impl<'a> core::fmt::Debug for Verlet<'a> {
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

    fn make_ball(id: usize, pos: Vec2D) -> Rc<RefCell<Ball2D<'static>>> {
        let b = Ball2D::new(id, pos, 1.0);
        Rc::new(RefCell::new(b))
    }

    #[test]
    fn test_add_remove_neighbour_and_query() {
        let mut v = Verlet::new();
        let a = make_ball(0, Vec2D::new(0.0, 0.0));
        let b = make_ball(1, Vec2D::new(1.0, 0.0));
        let was_neigh = v.add_neighbour(&b);
        assert!(!was_neigh);
        assert!(v.is_a_neighbour_id(b.borrow().get_id()));
        // Adding again returns true (was already neighbour)
        assert!(v.add_neighbour(&b));
        // Remove and ensure gone
        let mut v_mut = v;
        v_mut.remove(b.borrow().get_id());
        assert!(!v_mut.is_a_neighbour_id(b.borrow().get_id()));
        // No panic on removing non-existent
        v_mut.remove(a.borrow().get_id());
    }

    #[test]
    fn test_update_closeness_stats() {
        let mut v = Verlet::new();
        let a = make_ball(0, Vec2D::new(0.0, 0.0));
        let b = make_ball(1, Vec2D::new(3.0, 4.0)); // distance 5
        v.add_neighbour(&a);
        v.add_neighbour(&b);
        v.update_closeness_stats(&Vec2D::new(0.0, 0.0));
        assert!((v.nearest - 0.0).abs() < 1e-6);
        assert!((v.farthest - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_needs_update_hood_thresholds() {
        let v = Verlet::new();
        let mut ball = Ball2D::new(1, Vec2D::zero(), 1.0);
        ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
        ball.v = Vec2D::new(1.0, 0.0);
        // With small dt and high threshold -> false
        assert!(!v.needs_update_hood(&ball, 0.01, 10.0));
        // With larger dt -> true
        assert!(v.needs_update_hood(&ball, 1.0, 0.5));
    }

    #[test]
    fn test_has_ball_moved_enough_true() {
        let mut ball = Ball2D::new(1, Vec2D::zero(), 1.0);
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
        let mut ball = Ball2D::new(1, Vec2D::new(0.1, 0.1), 1.0);
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
        let mut ball = Ball2D::new(1, Vec2D::new(2., 0.), 1.0);
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
        let mut ball = Ball2D::new(1, Vec2D::zero(), 1.0);
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

        // Initially zero
        assert_eq!(v.num_neighbours, 0);

        // Add a
        v.add_neighbour(&a);
        assert_eq!(v.num_neighbours, 1);

        // Add b
        v.add_neighbour(&b);
        assert_eq!(v.num_neighbours, 2);

        // Add c
        v.add_neighbour(&c);
        assert_eq!(v.num_neighbours, 3);

        // Add b again (should not increment)
        v.add_neighbour(&b);
        assert_eq!(v.num_neighbours, 3);

        // Remove b
        v.remove(b.borrow().get_id());
        assert_eq!(v.num_neighbours, 2);

        // Remove a
        v.remove(a.borrow().get_id());
        assert_eq!(v.num_neighbours, 1);

        // Remove c
        v.remove(c.borrow().get_id());
        assert_eq!(v.num_neighbours, 0);

        // Remove non-existent (should not go negative)
        v.remove(a.borrow().get_id());
        assert_eq!(v.num_neighbours, 0);
    }

    #[test]
    fn test_num_neighbours_consistency_with_neighbours_vec() {
        let mut v = Verlet::new();
        let a = make_ball(0, Vec2D::new(0.0, 0.0));
        let b = make_ball(1, Vec2D::new(1.0, 0.0));

        v.add_neighbour(&a);
        v.add_neighbour(&b);
        // num_neighbours should match number of valid neighbours
        let count = v
            .neighbours
            .iter()
            .filter(|w| w.upgrade().is_some())
            .count();
        assert_eq!(v.num_neighbours, count);

        // Remove one
        v.remove(a.borrow().get_id());
        let count = v
            .neighbours
            .iter()
            .filter(|w| w.upgrade().is_some())
            .count();
        assert_eq!(v.num_neighbours, count);
    }
}
