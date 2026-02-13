use crate::application::{AppResult, GameLogic, LogStuff, MainContent, VerletIntegration};
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{Ball2D, BallFactory};
use crate::timing_monad::TimingMonad;
use rayon::prelude::*;
use std::time::Instant;

const DT_GOAL: f32 = 0.01; // 1. / (FPS_GOAL as f32);

pub struct GameLogicData {
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

    pub(crate) fn double_time_step(&mut self) {
        self.dt_goal *= 2.0;
        // Clamp to reasonable range to prevent instability
        if self.dt_goal > 0.16 {
            self.dt_goal = 0.16;
        }
    }

    pub(crate) fn halve_time_step(&mut self) {
        self.dt_goal *= 0.5;
        // Clamp to reasonable range
        if self.dt_goal < 0.000625 {
            self.dt_goal = 0.000625;
        }
    }

    pub(crate) fn get_time_step(&self) -> f32 {
        self.dt_goal
    }
}

impl MainContent {
    fn update_sim_and_move_balls(&mut self, dt: f32, bound_rect: &crate::objects2d::Rect) {
        let old_positions: Vec<_> = self
            .get_balls()
            .iter()
            .map(|ball| *ball.get_position())
            .collect();

        self.update_sim(dt, bound_rect);

        // Update cell assignments for balls that moved
        for (i, old_pos) in old_positions.iter().enumerate() {
            let new_pos = *self.balls[i].get_position();
            self.cell_grid
                .move_ball(i, &mut self.balls, old_pos, &new_pos);
        }

        self.remove_invalid_balls();
    }

    /// Update neighbor hood for all balls in occupied cells
    fn update_neighbor_hoods_for_occupied_cells(&mut self, dt: f32, neighbor_buf: &mut Vec<usize>) {
        let cutoff_and_skin =
            self.physics.get_cutoff_distance() + self.physics.get_skin_thickness();

        let max_radius = BallFactory::get_max_radius();
        let min_movement = self.physics.get_min_movement_for_update_hood(max_radius);

        let (grid_rows, grid_cols) = self.cell_grid.get_grid_size();

        for row in 0..grid_rows {
            for col in 0..grid_cols {
                // Get cell ball indices to avoid borrow issues
                let cell_ball_indices = if let Some(cell) = self.cell_grid.get_cell(row, col) {
                    cell.get_ball_indices()
                } else {
                    continue;
                };

                neighbor_buf.clear();

                self.cell_grid
                    .collect_ball_indices_into(row, col, neighbor_buf);

                for &ball_idx in cell_ball_indices {
                    Verlet::update_hood(
                        ball_idx,
                        dt,
                        neighbor_buf,
                        cutoff_and_skin,
                        min_movement,
                        &mut self.balls,
                    );
                }
            }
        }
    }

    /// Update neighbor statistics for all balls
    fn update_neighbor_statistics(&mut self) {
        // Compute statistics in parallel (read-only phase)
        // Only compute stats for balls with dirty hoods
        let stats: Vec<(usize, usize, f32, f32, bool, bool)> = (0..self.balls.len())
            .into_par_iter()
            .map(|i| {
                let ball = &self.balls[i];

                // Skip computation for clean hoods
                if !ball.hood.hood_dirty {
                    return (i, 0, f32::NAN, f32::NAN, false, false);
                }

                let ball_pos = *ball.get_position();
                let verbose_lines = ball.verbose.lines();
                let neighbour_indices = ball.hood.get_neighbour_indices();

                let (num_neighbours, nearest, farthest) =
                    Verlet::compute_neighbor_stats(&ball_pos, neighbour_indices, &self.balls);

                let should_toggle = !verbose_lines && num_neighbours > 100;

                (
                    i,
                    num_neighbours,
                    nearest,
                    farthest,
                    should_toggle,
                    verbose_lines,
                )
            })
            .collect();

        // Apply updates sequentially (mutation phase)
        for (i, num_neighbours, nearest, farthest, should_toggle, _verbose_lines) in
            stats.into_iter()
        {
            // Only update if the hood was dirty (num_neighbours won't be 0 for dirty hoods since we skip)
            if self.balls[i].hood.hood_dirty {
                let ball_hood = &mut self.balls[i].hood;
                ball_hood.num_neighbours = num_neighbours;
                ball_hood.nearest = nearest;
                ball_hood.farthest = farthest;
                // Reset the dirty flag after computing stats
                ball_hood.hood_dirty = false;

                if should_toggle {
                    self.balls[i].verbose.toggle_other_neighbour();
                }
                Self::log_closeness_update(&self.balls[i]);
            }
        }
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

    fn is_ball_colliding(&self, pt: &Ball2D, margin: f32) -> bool {
        self.get_balls()
            .iter()
            .any(|ball| self.physics.is_colliding(pt, ball, margin))
    }

    fn collect_neighbours_by_cell(&mut self, dt: f32) {
        if self
            .step_count
            .is_multiple_of(self.get_game_logic_data().neighbour_update_interval)
        {
            // Single reusable buffer to avoid per-cell allocations
            let mut neighbor_buf: Vec<usize> = Vec::with_capacity(30);
            let mut timer = TimingMonad::new();
            timer.time("update neighbours", || {
                self.update_neighbor_hoods_for_occupied_cells(dt, &mut neighbor_buf)
            });
            timer.time("update statistics", || self.update_neighbor_statistics());
            self.log_timings(timer);
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
