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
            .get_ball_vec()
            .iter()
            .map(|ball| ball.borrow().get_position().clone())
            .collect();

        self.update_sim(dt, bound_rect);

        let updates: Vec<_> = self
            .get_ball_vec()
            .iter()
            .enumerate()
            .map(|(i, ball_rc)| {
                let new_pos = ball_rc.borrow().get_position().clone();
                (ball_rc.clone(), old_positions[i].clone(), new_pos)
            })
            .collect();

        for (ball_rc, old_pos, new_pos) in updates {
            self.cell_grid.move_ball(ball_rc, &old_pos, &new_pos);
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
        self.get_ball_vec()
            .iter()
            .map(|ball_rc| ball_rc.borrow())
            .any(|ball| self.physics.is_colliding(pt, &(*ball)))
    }

    fn collect_neighbours_by_cell(&mut self, dt: f32) {
        if self.step_count % self.get_game_logic_data().neighbour_update_interval == 0 {
            let (rows, cols) = self.cell_grid.get_grid_size();

            // Single reusable buffer to avoid per-cell allocations
            let mut neighbor_buf: Vec<_> = Vec::new();
            // reserve some capacity to reduce reallocations; tune as needed
            neighbor_buf.reserve(30);
            
            let cutoff = self.physics.get_cutoff_distance();

            for row in 0..rows {
                for col in 0..cols {
                    neighbor_buf.clear();
                    self.cell_grid
                        .collect_balls_into(row, col, &mut neighbor_buf);

                    // Clone cell.balls to avoid holding immutable borrow
                    let cell_balls: Vec<_> = if let Some(cell) = self.cell_grid.get_cell(row, col) {
                        cell.balls.clone()
                    } else {
                        continue;
                    };

                    for ball_rc in &cell_balls {
                        let ball_id = ball_rc.borrow().get_id();
                        let ball_radius = ball_rc.borrow().radius;
                        
                        // Find ball index in v_points
                        let ball_idx = if let Some(idx) = self.get_ball_vec().iter().position(|b| b.borrow().get_id() == ball_id) {
                            idx
                        } else {
                            continue;
                        };
                        
                        // Get min_movement before taking mutable borrow
                        let min_movement = self.physics.get_min_movement_for_update_hood(ball_radius);

                        let neighbors = neighbor_buf.iter().filter(|neighbor| {
                            !std::ptr::eq(neighbor.as_ptr(), ball_rc.as_ptr())
                        });

                        // Get mutable access to the entire balls vector
                        let balls = self.get_ball_vec_mut();

                        Verlet::update_hood(
                            ball_idx,
                            dt,
                            neighbors,
                            cutoff,
                            min_movement,
                            balls,
                        );
                    }
                }
            }

            // Update stats
            for ball_rc in self.get_ball_vec().iter() {
                let ball_pos = ball_rc.borrow().get_position().clone();
                let mut ball = ball_rc.borrow_mut();
                let balls = self.get_ball_vec();
                ball.hood.update_closeness_stats(&ball_pos, balls);
                if !ball.verbose.lines() && ball.hood.get_number_of_neighbors() > 100 {
                    ball.verbose.toggle_other_neighbour();
                } else if ball.verbose.lines() && ball.hood.get_number_of_neighbors() <= 100 {
                    ball.verbose.toggle_other_neighbour();
                }
                Self::log_closeness_update(&ball);
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
            let ball_vec = self.get_ball_vec().clone();
            self.sim.calculate_temperature(&ball_vec);
        });
        #[cfg(not(feature = "measure-time"))]
        {
            let ball_vec = self.get_ball_vec().clone();
            // self.sim.calculate_temperature(&ball_vec);
            // Hold Ref guards so the borrows stay valid while calculate_temperature_fast runs
            let mut borrows: Vec<std::cell::Ref<'_, Ball2D>> = Vec::with_capacity(ball_vec.len());
            for rc in &ball_vec {
                borrows.push(rc.borrow());
            }

            // Build a slice of &Ball2D references (no cloning of Ball2D)
            let mut refs: Vec<&Ball2D> = Vec::with_capacity(borrows.len());
            for r in &borrows {
                refs.push(&*r);
            }
            self.sim.calculate_temperature_fast(refs.as_slice());
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
