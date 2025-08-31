use super::*;
use crate::frame::GameLogic;
use rand::{Rng, rng};
use rand_distr::{Distribution, Normal};

impl<'a> MainContent<'a> {
    pub fn add_candidate_to_vec(&mut self, candidate: Ball2D<'a>) {
        self.v_points.push(Rc::new(RefCell::new(candidate)));
        self.num_balls = self.v_points.len();
    }
    pub(crate) fn clear_points(&mut self) {
        self.v_points.clear();
        self.num_balls = 0;
    }
    pub(crate) fn get_number_of_balls(&self) -> usize {
        self.v_points.len()
    }
    pub(crate) fn get_ball_vec(&self) -> &Vec<Rc<RefCell<Ball2D<'a>>>> {
        &self.v_points
    }

    pub(crate) fn add_cursor_candidate(&mut self) {
        let Vec2D { x, y } = self.win_definition.curs_pos;
        self.add_candidate_px(x, y).ok();
    }
    pub(crate) fn add_random_candidates(&mut self, count: usize) {
        let WindowDefinition {
            margin_px: (x_min, y_min),
            region_px: (x_range, y_range),
            ..
        } = self.win_definition;
        let mut r = rng();
        for _ in 0..count {
            let x = x_min + r.random_range(0.0..x_range);
            let y = y_min + r.random_range(0.0..y_range);
            self.add_candidate_px(x, y).ok();
        }
    }
    pub(crate) fn enable_random_ball_speed(&mut self) {
        self.new_ball.rand_speed = true;
    }

    pub(crate) fn randomize_speeds(&mut self) {
        let normal = self.calculate_normal_distribution_of_speed();
        let mut r = rng();
        for ball in &self.v_points {
            let mut b = ball.borrow_mut();
            b.v.x = normal.sample(&mut r);
            b.v.y = normal.sample(&mut r);
        }
    }
    pub(crate) fn calculate_normal_distribution_of_speed(&mut self) -> Normal<f32> {
        let (sx2, sy2) = self
            .v_points
            .iter()
            .map(|b| b.borrow())
            .fold((0., 0.), |(ax, ay), b| {
                (ax + b.v.x.powi(2), ay + b.v.y.powi(2))
            });
        let n = self.v_points.len() as f32;
        let sig = f32::sqrt(f32::sqrt(sx2 / (n - 1.5) * sy2 / (n - 1.5)));
        Normal::new(0., f32::max(0.3, sig)).expect("Invalid distribution parameters")
    }
    pub(crate) fn remove_invalid_balls(&mut self) {
        self.v_points.retain(|cur| {
            let c = cur.borrow();
            let keep = !c.get_position().x.is_nan() && !c.get_position().y.is_nan();
            if !keep {
                println!("ball {:?} became invalid. removing", c);
            }
            keep
        });
        self.num_balls = self.v_points.len();
    }
}
