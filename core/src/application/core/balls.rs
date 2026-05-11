use super::*;
use crate::{
    application::{AppError, AppResult, GameLogic},
    objects2d::ball_impl::Named,
};
use rand::rng;
use rand_distr::{Distribution, Normal};
use std::cmp::Ordering;

impl MainContent {
    /// Add ball to simulation without collision checks.
    /// Ball is added to both the balls vector and the cell grid.
    ///
    /// # Arguments
    /// * `candidate` - Ball to add to simulation
    pub fn add_candidate_to_vec(&mut self, candidate: Ball2D) {
        self.world.add_ball_and_register(candidate);
    }
    /// Remove all balls from simulation and reset ID counter.
    /// Clears both balls vector and all cells in the grid.
    /// Also clears all placed operators.
    pub(crate) fn clear_points(&mut self) {
        self.world.clear_balls();
        self.game_data.get_ball_factory().reset_id_counter();
        self.world.clear_all_operators();
    }

    // /// Add a new ball candidate at the given position with zero initial velocity.
    // pub(crate) fn add_cursor_candidate(&mut self, pos: Vec2D) {
    //     self.add_candidate(pos).ok();
    // }
    /// Add specified number of balls at random non-colliding positions.
    /// Attempts up to count × 4 placements, stopping early if target count is reached.
    /// Uses equilibrium_edge_distance as collision margin.
    /// Updates neighbor lists after all balls are added.
    ///
    /// # Arguments
    /// * `count` - Target number of balls to add
    pub(crate) fn add_random_candidates(&mut self, count: usize) {
        let time_step = self.game_data.get_time_step();
        let cur_radius = self.game_data.get_ball_factory().get_new_ball_radius();

        let world = self.world.world_region_inset(cur_radius);
        let min_pos = &(world.get_tl().into());
        let max_pos = &world.get_br().into();

        //let mut r = rng();
        let mut balls_added = 0;
        let max_attempts = count * 4;
        for _ in 0..max_attempts {
            let nu_ball = self
                .game_data
                .get_ball_factory()
                .create_random((min_pos, max_pos), (0.0, 0.0));
            let margin = self.world.physics.equilibrium_edge_distance();
            if self.add_candidate_ball(nu_ball, margin).is_ok() {
                balls_added += 1;
                if balls_added >= count {
                    break;
                }
            }
        }

        self.collect_neighbours_by_cell(time_step);

        #[cfg(debug_assertions)]
        self.output_ten_closest_balls();
    }

    #[cfg(debug_assertions)]
    pub(crate) fn output_ten_closest_balls(&self) {
        // Collect all neighbor pairs with their distances
        let mut pairs: Vec<(usize, usize, f32)> = Vec::new();

        for (i, ball) in self.world.balls.iter().enumerate() {
            for &neighbor_idx in ball.hood.get_neighbour_indices() {
                // Only count each pair once (i < neighbor_idx)
                if i < neighbor_idx {
                    let other = &self.world.balls[neighbor_idx];
                    let distance = ball.get_edge_distance(other);
                    pairs.push((i, neighbor_idx, distance));
                }
            }
        }

        // Sort by distance (closest first)
        pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));

        // Output the 10 closest pairs
        let count = pairs.len().min(10);
        log::info!("\n=== {} Closest Ball Pairs ===", count);
        for (idx, &(i, j, dist)) in pairs.iter().take(10).enumerate() {
            let ball_i = &self.world.balls[i];
            let ball_j = &self.world.balls[j];
            log::info!(
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
        log::info!("=================================\n");
    }

    pub(crate) fn enable_random_ball_speed(&mut self) {
        // !todo()
        // set a boolean which modifies add_random_candidate to use random speeds
    }

    pub(crate) fn add_candidate_at_pos(&mut self, pos: Vec2D) -> AppResult {
        let candidate = self
            .game_data
            .get_ball_factory()
            .create_massive_with_velocity(pos, Vec2D::zero());
        self.world.add_candidate_ball(candidate, 0.)
    }

    /// Add ball if it's within region and not colliding.
    /// Clears energy history after successful addition.
    ///
    /// # Arguments
    /// * `candidate` - Ball to add
    /// * `margin` - Additional collision margin beyond ball radius
    ///
    /// # Returns
    /// Ok if ball was added, Err if out of bounds or colliding
    pub(crate) fn add_candidate_ball(&mut self, candidate: Ball2D, margin: f32) -> AppResult {
        self.world.add_candidate_ball(candidate, margin)
    }

    pub(crate) fn randomize_speeds(&mut self) {
        let normal = self.calculate_normal_distribution_of_speed();
        let mut r = rng();

        for ball in &mut self.world.balls {
            ball.v.x = normal.sample(&mut r);
            ball.v.y = normal.sample(&mut r);
        }
    }
    pub(crate) fn calculate_normal_distribution_of_speed(&mut self) -> Normal<f32> {
        let (sx2, sy2) = self.world.balls.iter().fold((0., 0.), |(ax, ay), b| {
            (ax + b.v.x.powi(2), ay + b.v.y.powi(2))
        });
        let n = self.world.balls.len() as f32;
        let sig = f32::sqrt(f32::sqrt(sx2 / (n - 1.5) * sy2 / (n - 1.5)));
        Normal::new(0., f32::max(0.3, sig)).expect("Invalid distribution parameters")
    }
    pub(crate) fn remove_invalid_balls(&mut self) {
        self.world.remove_invalid_balls();
    }

    pub(crate) fn stop_all_balls(&mut self) {
        for ball in &mut self.world.balls {
            ball.v = Vec2D::new(0., 0.);
        }
    }

    pub(crate) fn next_ball_size(&mut self) {
        self.game_data.get_ball_factory().next_ball_size_idx();
    }
}

impl SimulationWorld {
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
        self.rebuild_cell_grid();
    }
}

impl SimulationWorld {
    pub(crate) fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let inner_margin = pt.radius;
        let region = self.world_rect;
        let tl = region.get_tl();
        let world = Rect::new(
            tl.0 + inner_margin,
            tl.1 - inner_margin,
            region.w - 2.0 * inner_margin,
            region.h + 2.0 * inner_margin,
        );
        let pos = pt.get_position();

        pos.x >= world.x
            && pos.x <= (world.x + world.w)
            && pos.y >= (world.y + world.h)
            && pos.y <= world.y
    }

    pub(crate) fn is_ball_colliding(&self, pt: &Ball2D, margin: f32) -> bool {
        self.balls
            .iter()
            .any(|ball| self.physics.is_colliding(pt, ball, margin))
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
        self.add_ball_and_register(candidate);
        self.sim.clear_energy();
        Ok(())
    }

    pub(crate) fn add_ball_and_register(&mut self, candidate: Ball2D) {
        let ball_idx = self.balls.len();
        self.balls.push(candidate);
        self.cell_grid.add_ball(ball_idx, &mut self.balls);
    }

    pub(crate) fn clear_balls(&mut self) {
        self.balls.clear();
        self.cell_grid.clear_all_cells();
    }

    pub(crate) fn rebuild_cell_grid(&mut self) {
        self.cell_grid.clear_all_cells();
        for i in 0..self.balls.len() {
            self.cell_grid.add_ball(i, &mut self.balls);
        }
    }

    pub(crate) fn move_ball_in_cell_grid(
        &mut self,
        ball_idx: usize,
        old_pos: &Vec2D,
        new_pos: &Vec2D,
    ) {
        self.cell_grid
            .move_ball(ball_idx, &mut self.balls, old_pos, new_pos);
    }

    pub(crate) fn resize_cell_grid(
        &mut self,
        world_rect: crate::objects2d::Rect,
        cutoff_and_skin: f32,
        max_radius: f32,
    ) {
        self.world_rect = world_rect;
        self.cell_grid = CellGrid::new(self.world_rect, cutoff_and_skin, max_radius)
            .expect("Failed to create cell grid on resize");
        self.rebuild_cell_grid();
    }
}
