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

            for row in 0..rows {
                for col in 0..cols {
                    if let Some(cell) = self.cell_grid.get_cell(row, col) {
                        let neighbor_balls = self.cell_grid.get_balls_in_neighborhood(row, col);

                        for ball_rc in &cell.balls {
                            let ball_radius = { ball_rc.borrow().radius };

                            let neighbors = neighbor_balls.iter().filter(|neighbor| {
                                !std::ptr::eq(neighbor.as_ptr(), ball_rc.as_ptr())
                            });

                            let mut ball = ball_rc.borrow_mut();

                            Verlet::update_hood(
                                &mut *ball,
                                dt,
                                neighbors,
                                self.physics.get_cutoff_distance(),
                                self.physics.get_min_movement_for_update_hood(ball_radius),
                            );
                        }
                    }
                }
            }

            // Update stats
            for ball_rc in self.get_ball_vec().iter() {
                let ball_pos = ball_rc.borrow().get_position().clone();
                let mut ball = ball_rc.borrow_mut();
                ball.hood.update_closeness_stats(&ball_pos);
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
            self.sim.calculate_temperature(&ball_vec);
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
