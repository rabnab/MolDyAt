// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::line_segments::Obstacle;
use super::placement::PlacementIndex;
use super::*;
use crate::application::{AppError, AppResult};
use crate::objects2d::ball_impl::ball_core::gravity_acceleration;
use rand::rng;
use rand_distr::{Distribution, Normal};
use rayon::prelude::*;

impl MainContent {
    /// Add ball to simulation without collision checks.
    /// Ball is added to both the balls vector and the cell grid.
    ///
    /// # Arguments
    /// * `candidate` - Ball to add to simulation
    pub fn add_candidate_to_vec(&mut self, candidate: Ball2D) {
        self.world.add_ball_and_register(candidate);
    }

    /// Seed the simulation with up to `count` randomly placed balls.
    ///
    /// Placement obeys world bounds and collision constraints; actual inserted
    /// count may be lower in dense configurations.
    pub fn seed_random_balls(&mut self, count: usize) {
        self.seed_random_balls_with_speed(count, 0.0);
    }

    /// Like [`Self::seed_random_balls`], with speeds drawn uniformly from
    /// `0..=max_speed` in random directions. `max_speed` 0 seeds balls at rest
    /// and draws no extra random numbers, so it matches `seed_random_balls`.
    /// The centre-of-mass velocity is not removed.
    pub fn seed_random_balls_with_speed(&mut self, count: usize, max_speed: f32) {
        self.add_random_candidates_with_speed(count, max_speed, &[]);
    }

    /// Like [`Self::seed_random_balls_with_speed`], with a mixture of species:
    /// `mixture` lists species names of the world's table with their relative
    /// weights. Species are assigned in a fixed interleaved order (each new
    /// ball gets the species furthest behind its share), so no extra random
    /// numbers are drawn and a seeded run stays reproducible. An empty
    /// `mixture` seeds the species of new balls.
    pub fn seed_random_mixture(
        &mut self,
        count: usize,
        max_speed: f32,
        mixture: &[(String, u32)],
    ) -> AppResult {
        let mut resolved = Vec::with_capacity(mixture.len());
        for (name, weight) in mixture {
            let species = self.world.species();
            let id = species.by_name(name).ok_or_else(|| {
                let known: Vec<&str> = species.iter().map(|s| s.name.as_str()).collect();
                AppError::CustomError(format!(
                    "unknown species \"{name}\" (known: {})",
                    known.join(", ")
                ))
            })?;
            if *weight == 0 {
                return Err(AppError::CustomError(format!(
                    "species \"{name}\" needs a weight above 0"
                )));
            }
            resolved.push((id, *weight));
        }
        self.add_random_candidates_with_speed(count, max_speed, &resolved);
        Ok(())
    }
    /// Remove all balls from simulation and reset ID counter.
    /// Clears both balls vector and all cells in the grid.
    /// Also clears all placed operators.
    pub fn clear_points(&mut self) {
        self.world.clear_balls();
        self.step_config.get_ball_factory().reset_id_counter();
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
    pub fn add_random_candidates(&mut self, count: usize) {
        self.add_random_candidates_with_speed(count, 0.0, &[]);
    }

    /// `mixture`: species and weights (see [`Self::seed_random_mixture`]);
    /// empty seeds the species of new balls.
    fn add_random_candidates_with_speed(
        &mut self,
        count: usize,
        max_speed: f32,
        mixture: &[(SpeciesId, u32)],
    ) {
        let time_step = self.step_config.get_time_step();
        let new_ball_species = self.step_config.get_ball_factory().new_ball_species();
        let cur_radius = if mixture.is_empty() {
            self.step_config.get_ball_factory().get_new_ball_radius()
        } else {
            mixture
                .iter()
                .map(|&(id, _)| self.world.species().species(id).contact_radius())
                .fold(0.0, f32::max)
        };
        let total_weight: u64 = mixture.iter().map(|&(_, w)| u64::from(w)).sum();
        let mut placed_per_species = vec![0u64; mixture.len()];

        let world = self.world.world_region_inset(cur_radius);
        let min_pos = &(world.get_tl().into());
        let max_pos = &world.get_br().into();

        //let mut r = rng();
        let obstacles = self.world.collect_solid_obstacles();
        // One index for the whole batch: each candidate is checked against
        // its neighbourhood only (#245), with the same result as a full scan.
        let mut index = PlacementIndex::new(&self.world);
        let mut balls_added = 0;
        let max_attempts = count * 4;
        for _ in 0..max_attempts {
            // The species furthest behind its share of the balls placed so
            // far (plus this one); ties go to the first listed.
            let pick = (!mixture.is_empty()).then(|| {
                let placed = balls_added as u64 + 1;
                (0..mixture.len())
                    .max_by_key(|&i| {
                        let target = u64::from(mixture[i].1) * placed;
                        let have = placed_per_species[i] * total_weight;
                        (target as i128 - have as i128, std::cmp::Reverse(i))
                    })
                    .expect("mixture is not empty")
            });
            let species = pick.map_or(new_ball_species, |i| mixture[i].0);
            let nu_ball = self.step_config.get_ball_factory().create_random(
                species,
                (min_pos, max_pos),
                (0.0, max_speed),
            );
            let margin = self.world.equilibrium_gap(nu_ball.species());
            if self
                .world
                .add_candidate_ball_indexed(nu_ball, margin, &obstacles, &mut index)
                .is_ok()
            {
                if let Some(i) = pick {
                    placed_per_species[i] += 1;
                }
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
        use crate::objects2d::ball_impl::Named;
        use std::cmp::Ordering;

        // Collect all neighbor pairs with their distances
        let mut pairs: Vec<(usize, usize, f32)> = Vec::new();

        for (i, ball) in self.world.balls.iter().enumerate() {
            for &neighbor_idx in ball.hood.get_neighbour_indices() {
                // Only count each pair once (i < neighbor_idx)
                if i < neighbor_idx {
                    let other = &self.world.balls[neighbor_idx];
                    let distance = self.world.physics.edge_gap(ball, other);
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
                ball_i.get_contact_radius(),
                ball_i.get_position().x,
                ball_i.get_position().y,
                ball_j.get_name(),
                ball_j.get_contact_radius(),
                ball_j.get_position().x,
                ball_j.get_position().y,
                dist,
            );
        }
        log::info!("=================================\n");
    }

    pub fn enable_random_ball_speed(&mut self) {
        // !todo()
        // set a boolean which modifies add_random_candidate to use random speeds
    }

    pub fn add_candidate_at_pos(&mut self, pos: Vec2D) -> AppResult {
        let candidate = self
            .step_config
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
    pub fn add_candidate_ball(&mut self, candidate: Ball2D, margin: f32) -> AppResult {
        self.world.add_candidate_ball(candidate, margin)
    }

    pub fn randomize_speeds(&mut self) {
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

    pub fn stop_all_balls(&mut self) {
        for ball in &mut self.world.balls {
            ball.v = Vec2D::new(0., 0.);
        }
    }

    pub fn next_species(&mut self) {
        self.step_config.get_ball_factory().next_species();
    }

    pub fn previous_species(&mut self) {
        self.step_config.get_ball_factory().previous_species();
    }

    #[cfg(test)]
    pub(crate) fn remove_invalid_balls(&mut self) {
        self.world.remove_invalid_balls();
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
                log::warn!("ball {:?} became invalid. removing", ball);
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
            // Reset last_update_hood_pos to infinity to force update_hood on next check
            ball.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
        }

        // Rebuild cell grid with valid balls
        self.rebuild_cell_grid();
    }

    pub(crate) fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let inner_margin = self.physics.wall_contact_distance(pt.species());
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
        let obstacles = self.collect_solid_obstacles();
        self.add_candidate_ball_among(candidate, margin, &obstacles)
    }

    /// `add_candidate_ball` against a prebuilt wall obstacle list, so batch
    /// spawners flatten the walls once instead of once per candidate.
    pub(crate) fn add_candidate_ball_among(
        &mut self,
        candidate: Ball2D,
        margin: f32,
        obstacles: &[Obstacle],
    ) -> AppResult {
        self.place_candidate(candidate, margin, obstacles, None)
    }

    /// `add_candidate_ball_among` for batch placement: the collision check
    /// uses `index` (see `placement`), which this keeps up to date.
    pub(crate) fn add_candidate_ball_indexed(
        &mut self,
        candidate: Ball2D,
        margin: f32,
        obstacles: &[Obstacle],
        index: &mut PlacementIndex,
    ) -> AppResult {
        self.place_candidate(candidate, margin, obstacles, Some(index))
    }

    fn place_candidate(
        &mut self,
        candidate: Ball2D,
        margin: f32,
        obstacles: &[Obstacle],
        index: Option<&mut PlacementIndex>,
    ) -> AppResult {
        if !self.is_ball_in_region(&candidate) {
            return Err(AppError::CustomError(
                "new ball not within region".to_string(),
            ));
        }
        let colliding = match &index {
            Some(index) => self.is_ball_colliding_indexed(&candidate, margin, index),
            None => self.is_ball_colliding(&candidate, margin),
        };
        if colliding {
            return Err(AppError::CustomError(
                "new ball colliding with existing ball".to_string(),
            ));
        }
        let contact = self.physics.wall_contact_distance(candidate.species());
        if Self::overlaps_any_wall(obstacles, candidate.get_position(), contact) {
            return Err(AppError::CustomError("overlaps with wall".to_string()));
        }

        self.add_ball_and_register(candidate);
        if let Some(index) = index {
            index.note_added(self, self.balls.len() - 1);
        }
        self.sim.clear_energy();
        Ok(())
    }

    pub(crate) fn add_ball_and_register(&mut self, mut candidate: Ball2D) {
        candidate.accel.y += gravity_acceleration(self.physics.constants.gravity).y;
        // A new species may need wider cells; the grid is refitted before the
        // next neighbour rebuild (`fit_cell_grid`), when every ball is in its
        // cell.
        self.species_present.insert(candidate.species());
        let ball_idx = self.balls.len();
        let position = *candidate.get_position();
        self.balls.push(candidate);
        self.cell_grid.add_ball(ball_idx, &position);
    }

    pub(crate) fn clear_balls(&mut self) {
        self.balls.clear();
        self.rebuild_cell_grid();
    }

    /// Put every ball into the grid from scratch, with cells sized for the
    /// species now in the world.
    pub fn rebuild_cell_grid(&mut self) {
        self.species_present = self.balls.iter().map(Ball2D::species).collect();
        let list_radius = self.physics.max_list_radius(self.species_present);
        if list_radius != self.cell_grid.list_radius() {
            self.cell_grid = CellGrid::new(self.world_rect, list_radius)
                .expect("Failed to create cell grid for the species present");
        }
        self.cell_grid.rebuild(&self.balls);
    }

    /// Rebuild the grid if the species added since the last rebuild need
    /// wider cells than it has, so the 3x3 cells around a ball still cover all
    /// its neighbours. Call only while every ball is registered in the cell of
    /// its position (not between a position update and the grid moves).
    pub(crate) fn fit_cell_grid(&mut self) {
        if self.physics.max_list_radius(self.species_present) > self.cell_grid.list_radius() {
            self.rebuild_cell_grid();
        }
    }

    /// Edge gap of two balls of `species` at their potential minimum: the
    /// placement margin that seeds balls without compressed pairs. Never
    /// negative, since it applies to pairs with every other species too.
    pub(crate) fn equilibrium_gap(&self, species: SpeciesId) -> f32 {
        let contact = self.physics.contact_distance(species, species);
        (self.physics.equilibrium_distance(species, species) - contact).max(0.0)
    }

    /// Sort the balls by cell (the order of `CellGrid::sorted_balls`, then
    /// the balls outside the grid in their order), so neighbours in space
    /// are neighbours in memory. Every index changes: the neighbour lists
    /// are renumbered (`Verlet::remap`; they keep the same partners, so no
    /// rebuild is forced) and the grid is rebuilt now. Call right after a
    /// grid rebuild, when the grid holds every ball in its sorted arrays.
    pub(crate) fn reorder_balls_by_cell(&mut self) {
        let n = self.balls.len();
        let mut order: Vec<usize> = Vec::with_capacity(n);
        order.extend(self.cell_grid.sorted_balls().iter().map(|&b| b as usize));
        if order.len() < n {
            let mut placed = vec![false; n];
            for &b in &order {
                placed[b] = true;
            }
            order.extend((0..n).filter(|&b| !placed[b]));
        }
        debug_assert_eq!(order.len(), n);

        let mut old: Vec<Option<Ball2D>> = std::mem::take(&mut self.balls)
            .into_iter()
            .map(Some)
            .collect();
        self.balls = order
            .iter()
            .map(|&b| old[b].take().expect("each ball once"))
            .collect();
        if self.cached_force_results.len() == n {
            let results = std::mem::take(&mut self.cached_force_results);
            self.cached_force_results = order.iter().map(|&b| results[b].clone()).collect();
        }
        let mut new_index = vec![0; n];
        for (new, &old) in order.iter().enumerate() {
            new_index[old] = new;
        }
        self.balls
            .par_iter_mut()
            .for_each(|ball| ball.hood.remap(&new_index));
        self.rebuild_cell_grid();
    }

    pub(crate) fn resize_cell_grid(&mut self, world_rect: crate::objects2d::Rect) {
        self.world_rect = world_rect;
        let list_radius = self.physics.max_list_radius(self.species_present);
        self.cell_grid = CellGrid::new(self.world_rect, list_radius)
            .expect("Failed to create cell grid on resize");
        self.rebuild_cell_grid();
    }
}
