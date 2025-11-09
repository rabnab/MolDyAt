use crate::frame::{AppResult, GameLogic, LogStuff, MainContent, VerletIntegration};
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{Ball2D, BallFactory};
use std::time::Instant;

const DT_GOAL: f32 = 0.01; // 1. / (FPS_GOAL as f32);

pub(crate) struct GameLogicData {
    dt_goal: f32,
    ball_factory: BallFactory,
    neighbour_update_interval: usize,
}

struct Timer(Instant);
impl Timer {
    fn start() -> Self {
        Self(Instant::now())
    }

    fn ms(&self) -> u128 {
        self.0.elapsed().as_millis()
    }
}

// fn elapsed_since(start: Instant) -> u128 {
//     let dur = start.elapsed();
//     let updating_ms = dur.as_millis();
//     updating_ms
// }

impl Default for GameLogicData {
    fn default() -> Self {
        GameLogicData {
            dt_goal: DT_GOAL,
            ball_factory: BallFactory::new(),
            neighbour_update_interval: 1,
        }
    }
}

impl GameLogicData {
    pub(crate) fn get_ball_factory(&self) -> &BallFactory {
        &self.ball_factory
    }
}

impl MainContent {
    fn update_sim_and_move_balls(&mut self, dt: f32, bound_rect: &crate::objects2d::Rect) {
        let old_positions: Vec<_> = self
            .get_balls()
            .iter()
            .map(|ball| ball.get_position().clone())
            .collect();

        self.update_sim(dt, bound_rect);

        // Update cell assignments for balls that moved
        for (i, old_pos) in old_positions.iter().enumerate() {
            let new_pos = self.balls[i].get_position().clone();
            self.cell_grid.move_ball(i, &mut self.balls, old_pos, &new_pos);
        }

        self.remove_invalid_balls();
    }
}

impl GameLogic for MainContent {
    fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let world = self.world_region_inset(pt.radius);
        let pos = pt.get_position();

        pos.x >= world.x
            && pos.x <= (world.x + world.w)
            && pos.y >= (world.y + world.h)
            && pos.y <= world.y
    }

    fn is_ball_colliding(&self, pt: &Ball2D) -> bool {
        self.get_balls()
            .iter()
            .any(|ball| self.physics.is_colliding(pt, ball))
    }

    fn collect_neighbours_by_cell(&mut self, dt: f32) {
        if self.step_count % self.get_game_logic_data().neighbour_update_interval == 0 {
            // Single reusable buffer to avoid per-cell allocations
            let mut neighbor_buf: Vec<usize> = Vec::new();
            neighbor_buf.reserve(30);
            
            let cutoff = self.physics.get_cutoff_distance();

            // Only process cells that have balls (tracked via current_cell)
            // Build set of occupied cells from all balls
            let occupied_cells: std::collections::HashSet<_> = self
                .get_balls()
                .iter()
                .filter_map(|b| b.current_cell)
                .collect();

            for (row, col) in occupied_cells {
                neighbor_buf.clear();
                self.cell_grid
                    .collect_ball_indices_into(row, col, &mut neighbor_buf);

                // Get cell ball indices to avoid borrow issues
                let cell_ball_indices: Vec<usize> = if let Some(cell) = self.cell_grid.get_cell(row, col) {
                    cell.get_ball_indices().to_vec()
                } else {
                    continue;
                };

                for &ball_idx in &cell_ball_indices {
                    let ball_radius = self.balls[ball_idx].radius;
                    let min_movement = self.physics.get_min_movement_for_update_hood(ball_radius);

                    Verlet::update_hood(
                        ball_idx,
                        dt,
                        &neighbor_buf,
                        cutoff,
                        min_movement,
                        &mut self.balls,
                    );
                }
            }

            // Update stats - compute neighbor statistics for each ball
            // We cannot call update_closeness_stats directly due to borrow checker,
            // so we compute the stats and update each ball separately
            for i in 0..self.balls.len() {
                let ball_pos = self.balls[i].get_position().clone();
                let verbose_lines = self.balls[i].verbose.lines();
                let neighbour_indices: Vec<usize> = self.balls[i].hood.get_neighbour_indices().to_vec();
                
                // Compute nearest and farthest distances from neighbors
                let (nearest_sq, farthest_sq) = neighbour_indices
                    .iter()
                    .filter_map(|&idx| self.balls.get(idx))
                    .map(|ball| ball_pos.dist(&ball.get_position()).norm_sq())
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), val| {
                        (min.min(val), max.max(val))
                    });
                
                // Update the ball's hood statistics
                let ball_hood = &mut self.balls[i].hood;
                ball_hood.num_neighbours = neighbour_indices.len();
                ball_hood.nearest = if nearest_sq.is_finite() {
                    f32::sqrt(nearest_sq)
                } else {
                    f32::NAN
                };
                ball_hood.farthest = if farthest_sq.is_finite() {
                    f32::sqrt(farthest_sq)
                } else {
                    f32::NAN
                };
                
                let num_neighbors = self.balls[i].hood.get_number_of_neighbors();
                
                // Toggle verbose if needed
                if !verbose_lines && num_neighbors > 100 {
                    self.balls[i].verbose.toggle_other_neighbour();
                } else if verbose_lines && num_neighbors <= 100 {
                    self.balls[i].verbose.toggle_other_neighbour();
                }
                Self::log_closeness_update(&self.balls[i]);
            }
        }
    }

    fn simulation_step(&mut self) -> AppResult<()> {
        self.sim.inc_frame_since_draw();
        if self.sim.is_paused() {
            return Ok(());
        }
        if self.any_log() {
            Self::log_separator_line();
        }
        if self.step_by_step {
            self.sim.set_pause(true);
        }

        let dt = self.get_game_logic_data().dt_goal;
        let bound_rect = self.world_region();

        // timing instrumentation is clearly separated by cfg blocks
        #[cfg(feature = "measure-time")]
        let mut timer = TimingMonad::new();

        #[cfg(feature = "measure-time")]
        timer.time("collect_neighbours", || self.collect_neighbours_by_cell(dt));
        #[cfg(not(feature = "measure-time"))]
        self.collect_neighbours_by_cell(dt);

        // update simulation and move balls (extracted to helper)
        #[cfg(feature = "measure-time")]
        timer.time("update_sim", || {
            self.update_sim_and_move_balls(dt, &bound_rect);
        });
        #[cfg(not(feature = "measure-time"))]
        {
            self.update_sim_and_move_balls(dt, &bound_rect);
        }

        #[cfg(feature = "measure-time")]
        timer.time("calculate_temperature", || {
            self.sim.calculate_temperature_from_balls(&self.balls);
        });
        #[cfg(not(feature = "measure-time"))]
        {
            self.sim.calculate_temperature_from_balls(&self.balls);
        }

        #[cfg(feature = "measure-time")]
        self.log_timings(timer);

        self.step_count += 1;
        Ok(())
    }

    // Helper to encapsulate the simulation update + cell grid updates + cleanup.
    // This reduces duplication and keeps measurement-only code simple.

    fn init_game_logic() -> GameLogicData {
        GameLogicData {
            neighbour_update_interval: 1,
            ..Default::default()
        }
    }
}
