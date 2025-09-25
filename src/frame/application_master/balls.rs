use super::*;
use crate::frame::GameLogic;
use rand::{Rng, rng};
use rand_distr::{Distribution, Normal};

impl<'a> MainContent<'a> {
    pub fn add_candidate_to_vec(&mut self, candidate: Ball2D<'a>) {
        let ball_rc = Rc::new(RefCell::new(candidate));
        self.cell_grid.add_ball(ball_rc.clone());
        self.v_points.push(ball_rc);
        self.num_balls = self.v_points.len();
    }
    pub(crate) fn clear_points(&mut self) {
        self.v_points.clear();
        self.cell_grid.clear_all_cells();
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
        let mut valid_balls = Vec::new();
        self.cell_grid.clear_all_cells();
        
        for ball_rc in &self.v_points {
            let ball = ball_rc.borrow();
            let keep = !ball.get_position().x.is_nan() && !ball.get_position().y.is_nan();
            if !keep {
                println!("ball {:?} became invalid. removing", ball);
            } else {
                valid_balls.push(ball_rc.clone());
            }
        }
        
        // Re-add valid balls to grid
        for ball_rc in &valid_balls {
            self.cell_grid.add_ball(ball_rc.clone());
        }
        
        self.v_points = valid_balls;
        self.num_balls = self.v_points.len();
    }
}
