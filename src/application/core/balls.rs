use super::*;
use crate::{
    application::{AppError, AppResult, GameLogic},
    objects2d::ball_impl::Named,
};
use rand::rng;
use rand_distr::{Distribution, Normal};
use std::cmp::Ordering;

impl MainContent {
    pub fn add_candidate_to_vec(&mut self, candidate: Ball2D) {
        let ball_idx = self.balls.len();
        self.balls.push(candidate);
        self.cell_grid.add_ball(ball_idx, &mut self.balls);
        self.num_balls = self.balls.len();
    }
    pub(crate) fn clear_points(&mut self) {
        self.balls.clear();
        self.cell_grid.clear_all_cells();
        self.get_ball_factory().reset_id_counter();
        self.num_balls = 0;
    }
    pub(crate) fn get_number_of_balls(&self) -> usize {
        self.balls.len()
    }
    pub(crate) fn get_balls(&self) -> &[Ball2D] {
        &self.balls
    }

    pub(crate) fn get_balls_mut(&mut self) -> &mut [Ball2D] {
        &mut self.balls
    }

    // /// Add a new ball candidate at the given position with zero initial velocity.
    // pub(crate) fn add_cursor_candidate(&mut self, pos: Vec2D) {
    //     self.add_candidate(pos).ok();
    // }
    pub(crate) fn add_random_candidates(&mut self, count: usize) {
        let time_step = self.get_game_logic_data().get_time_step();
        let cur_radius = self
            .get_game_logic_data()
            .get_ball_factory()
            .get_new_ball_radius();

        let world = self.world_region_inset(cur_radius);
        let min_pos = &(world.get_tl().into());
        let max_pos = &world.get_br().into();

        //let mut r = rng();
        let mut balls_added = 0;
        let max_attempts = count * 4;
        for _ in 0..max_attempts {
            let nu_ball = self
                .get_game_logic_data()
                .get_ball_factory()
                .create_random((min_pos, max_pos), (0.0, 0.0));
            let margin = self.physics.equilibrium_edge_distance();
            if self.add_candidate_ball(nu_ball, margin).is_ok() {
                balls_added += 1;
                if balls_added >= count {
                    break;
                }
            }
        }

        self.collect_neighbours_by_cell(time_step);
        self.output_ten_closest_balls();
    }

    pub(crate) fn output_ten_closest_balls(&self) {
        // Collect all neighbor pairs with their distances
        let mut pairs: Vec<(usize, usize, f32)> = Vec::new();

        for (i, ball) in self.balls.iter().enumerate() {
            for &neighbor_idx in ball.hood.get_neighbour_indices() {
                // Only count each pair once (i < neighbor_idx)
                if i < neighbor_idx {
                    let other = &self.balls[neighbor_idx];
                    let distance = ball.get_edge_distance(other);
                    pairs.push((i, neighbor_idx, distance));
                }
            }
        }

        // Sort by distance (closest first)
        pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));

        // Output the 10 closest pairs
        let count = pairs.len().min(10);
        println!("\n=== {} Closest Ball Pairs ===", count);
        for (idx, &(i, j, dist)) in pairs.iter().take(10).enumerate() {
            let ball_i = &self.balls[i];
            let ball_j = &self.balls[j];
            println!(
                "{}. Ball {} (r={:.2}) at ({:.2}, {:.2}) <-> Ball {} (r={:.2}) at ({:.2}, {:.2}) | distance: {:.4}",
                idx + 1,
                ball_i.get_name(),
                ball_i.get_radius(),
                ball_i.get_position().x,
                ball_i.get_position().y,
                ball_j.get_name(),
                ball_j.get_radius(),
                ball_j.get_position().x,
                ball_j.get_position().y,
                dist,
            );
        }
        println!("=================================\n");
    }

    pub(crate) fn enable_random_ball_speed(&mut self) {
        // !todo()
        // set a boolean which modifies add_random_candidate to use random speeds
    }

    pub(crate) fn add_candidate_at_pos(&mut self, pos: Vec2D) -> AppResult {
        let candidate = {
            self.get_game_logic_data()
                .get_ball_factory()
                .create_massive_with_velocity(pos, Vec2D::zero())
        };
        self.add_candidate_ball(candidate, 0.)
    }

    pub(crate) fn add_candidate_ball(&mut self, candidate: Ball2D, margin: f32) -> AppResult {
        if !self.is_ball_in_region(&candidate) {
            return Err(AppError::CustomError(
                "new ball not within region".to_string(),
            ));
        }
        if self.is_ball_colliding(&candidate, margin) {
            return Err(AppError::CustomError(
                "new ball colliding with existing ball".to_string(),
            ));
        }
        self.add_candidate_to_vec(candidate);
        self.sim.clear_energy();
        Ok(())
    }

    pub(crate) fn randomize_speeds(&mut self) {
        let normal = self.calculate_normal_distribution_of_speed();
        let mut r = rng();
        for ball in &mut self.balls {
            ball.v.x = normal.sample(&mut r);
            ball.v.y = normal.sample(&mut r);
        }
    }
    pub(crate) fn calculate_normal_distribution_of_speed(&mut self) -> Normal<f32> {
        let (sx2, sy2) = self.balls.iter().fold((0., 0.), |(ax, ay), b| {
            (ax + b.v.x.powi(2), ay + b.v.y.powi(2))
        });
        let n = self.balls.len() as f32;
        let sig = f32::sqrt(f32::sqrt(sx2 / (n - 1.5) * sy2 / (n - 1.5)));
        Normal::new(0., f32::max(0.3, sig)).expect("Invalid distribution parameters")
    }
    pub(crate) fn remove_invalid_balls(&mut self) {
        let mut removed_some_balls = false;
        // Remove invalid balls and rebuild cell grid
        self.balls.retain(|ball| {
            let keep = !ball.get_position().x.is_nan() && !ball.get_position().y.is_nan();
            if !keep {
                removed_some_balls = true;
                println!("ball {:?} became invalid. removing", ball);
            }
            keep
        });

        if !removed_some_balls {
            return;
        }

        // After retain, all indices have changed. Clear neighbor lists and force update.
        // This prevents stale indices from causing incorrect lookups or panics.
        for ball in &mut self.balls {
            ball.hood.clear_neighbours();
            ball.current_cell = None;
            // Reset last_update_hood_pos to infinity to force update_hood on next check
            ball.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        }

        // Rebuild cell grid with valid balls
        self.cell_grid.clear_all_cells();
        for i in 0..self.balls.len() {
            self.cell_grid.add_ball(i, &mut self.balls);
        }

        self.num_balls = self.balls.len();
    }

    pub(crate) fn stop_all_balls(&mut self) {
        for ball in &mut self.balls {
            ball.v = Vec2D::new(0., 0.);
        }
    }

    pub(crate) fn next_ball_size(&mut self) {
        self.game_data.get_ball_factory().next_ball_size_idx();
    }
}
