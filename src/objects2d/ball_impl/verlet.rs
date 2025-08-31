use crate::objects2d::ball_impl::ball_core::Ball2D;
use crate::objects2d::ball_impl::{Named, UPDATE_HOOD_CNT};
use crate::objects2d::vec_2d::Vec2D;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

const UPDATE_FREQ: usize = 100;

pub(crate) struct Verlet<'a> {
    pub neighbours: RefCell<Vec<Weak<RefCell<Ball2D<'a>>>>>,
    pub nearest: f32,
    pub farthest: f32,
    updates_frequency: usize,
    updates_since_refresh: usize,
}

impl<'a> Default for Verlet<'a> {
    fn default() -> Verlet<'a> {
        Verlet {
            neighbours: RefCell::new(Vec::new()),
            nearest: f32::NAN,
            farthest: f32::NAN,
            updates_frequency: UPDATE_FREQ,
            updates_since_refresh: UPDATE_FREQ, //initialize with same for instant refresh on creation
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

    /// Retrieve the names of all valid neighbours.
    pub fn get_neighbour_names(&self) -> Vec<String> {
        self.neighbours
            .borrow()
            .iter()
            .filter_map(|weak| weak.upgrade()) // Filter out dropped references
            .map(|rcrefcell| rcrefcell.borrow().get_name())
            .collect()
    }

    // fn add_neighbour_already_sure_was_no_neighbour(&self, ball_rc: Weak<RefCell<Ball2D<'a>>>) {
    //     self.neighbours.borrow_mut().push(ball_rc);
    // }
    pub fn add_neighbour(&self, ball_rc: &Rc<RefCell<Ball2D<'a>>>) -> bool {
        let was_neighbour = self.is_a_neighbour(&ball_rc.borrow().get_name());
        if !was_neighbour {
            self.neighbours.borrow_mut().push(Rc::downgrade(ball_rc));
        }
        was_neighbour
    }

    pub fn is_a_neighbour(&self, name: &str) -> bool {
        self.neighbours
            .borrow()
            .iter()
            .filter_map(|weak| weak.upgrade())
            .any(|rcrefcell| rcrefcell.borrow().get_name() == name)
    }

    pub fn remove(&mut self, name: &str) {
        self.neighbours.borrow_mut().retain(|weak| {
            weak.upgrade()
                .is_some_and(|ball_rc| ball_rc.borrow().get_name() != name)
        });
    }
    pub fn needs_update_hood(&self, ball: &Ball2D, dt: f32, min_movement_threshold: f32) -> bool {
        if self.updates_since_refresh > self.updates_frequency {
            return true;
        }
        let travel_since_last_upd_sq = ball
            .last_update_hood_pos
            .add_scaled(&ball.v, dt)
            .dist(&ball.pos)
            .norm_sq();
        //return (self.radius + LJ_CUTOFF) * (self.radius + LJ_CUTOFF) < travel_since_last_upd_sq;
        min_movement_threshold * min_movement_threshold < travel_since_last_upd_sq
    }

    pub fn update_hood<'b>(
        ball: &mut Ball2D<'a>,
        dt: f32,
        // self_rc: &Rc<RefCell<Ball2D<'a>>>,
        neighbours: impl IntoIterator<Item = &'b Rc<RefCell<Ball2D<'a>>>>,
        cutoff: f32,
        min_movement_threshold: f32,
    ) where
        'a: 'b,
    {
        //let cutoff = self.radius + LJ_CUTOFF * 2. + self.v.norm() * dt;

        let dist_limit_by_v_sq = 0.;
        //we only check hood every ten updates --> factors 100 and 10
        //100. * v_sq * dt * dt + eq * eq + 2. * 10. * dt * eq * (self.v.x + self.v.y);
        let dist_limit_by_closest_sq = cutoff * cutoff;
        let dist_limit_sq = f32::max(dist_limit_by_closest_sq, dist_limit_by_v_sq);
        // let ball =  &mut *self_rc.borrow_mut();
        if ball.verbose.console {
            println!(
                "update_hood: {:?} dist_lim: {} ",
                ball,
                f32::sqrt(dist_limit_sq),
            );
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
        ball.last_update_hood_pos = ball.pos.clone();
        for cur_rc in neighbours {
            let cur_ref = cur_rc.borrow();

            let dist_sq = ball.pos.dist(&cur_ref.pos).norm_sq();
            //println!("checking {}", cur_ref.get_name());
            if dist_sq < dist_limit_sq {
                ball.hood.add_neighbour(cur_rc);

                if ball.verbose.console {
                    println!(
                        "\tneighbours {} at ({},{}) dist {}",
                        cur_ref.get_name(),
                        cur_ref.pos.x,
                        cur_ref.pos.y,
                        f32::sqrt(dist_sq)
                    );
                } else if cur_ref.verbose.console {
                    println!(
                        "\t {} is neighbour of {} since its dist is {} (dist_limit {})",
                        cur_ref.get_name(),
                        ball.get_name(),
                        f32::sqrt(dist_sq),
                        f32::sqrt(dist_limit_sq)
                    )
                }
            } else {
                ball.hood.remove(&cur_ref.get_name());
            }
        }
        ball.hood.updates_since_refresh = 0;
    }

    pub fn update_closeness_stats(&mut self, ball_pos: &Vec2D) {
        let actual_hood = self.neighbours.borrow();

        let (nearest_sq, farthest_sq) = actual_hood
            .iter()
            .filter_map(|weak| weak.upgrade())
            .map(|val| {
                let d = ball_pos.dist(&val.borrow().pos).norm_sq();
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
            self.neighbours.borrow().len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ball(id: usize, pos: Vec2D) -> Rc<RefCell<Ball2D<'static>>> {
        let mut b = Ball2D::default();
        b.pos = pos;
        b.pre_update_pos = b.pos.clone();
        b.id = id;
        Rc::new(RefCell::new(b))
    }

    #[test]
    fn test_add_remove_neighbour_and_query() {
        let v = Verlet::new();
        let a = make_ball(0, Vec2D::new(0.0, 0.0));
        let b = make_ball(1, Vec2D::new(1.0, 0.0));
        let was_neigh = v.add_neighbour(&b);
        assert!(!was_neigh);
        assert!(v.is_a_neighbour(&b.borrow().get_name()));
        // Adding again returns true (was already neighbour)
        assert!(v.add_neighbour(&b));
        // Remove and ensure gone
        let mut v_mut = v;
        v_mut.remove(&b.borrow().get_name());
        assert!(!v_mut.is_a_neighbour(&b.borrow().get_name()));
        // No panic on removing non-existent
        v_mut.remove(&a.borrow().get_name());
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
        let mut ball = Ball2D::default();
        ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
        ball.pos = Vec2D::new(0.0, 0.0);
        ball.v = Vec2D::new(1.0, 0.0);
        // With small dt and high threshold -> false
        assert!(!v.needs_update_hood(&ball, 0.01, 10.0));
        // With larger dt -> true
        assert!(v.needs_update_hood(&ball, 1.0, 0.5));
    }
}
