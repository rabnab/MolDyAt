use crate::frame::{GameLogic, LogStuff, MainContent, VerletIntegration};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{Ball2D, BallFactory};
use ggez::{GameError, GameResult};
use rand::distr::Distribution;
use rand::rng;
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
    pub(crate) fn get_ball_factory(&mut self) -> &mut BallFactory {
        &mut self.ball_factory
    }
}

impl<'a> GameLogic for MainContent<'a> {
    fn init_game_logic() -> GameLogicData {
        GameLogicData {
            neighbour_update_interval: 1,
            ..Default::default()
        }
    }

    fn is_ball_in_region(&self, pt: &Ball2D) -> bool {
        let world = self.get_window_definition().world_region_inset(pt.radius);
        let pos = &pt.get_position();

        pos.x >= f32::min(world.x, world.x + world.w)
            && pos.x <= f32::max(world.x, world.x + world.w)
            && pos.y >= f32::min(world.y, world.y + world.h)
            && pos.y <= f32::max(world.y, world.y + world.h)
    }

    fn is_ball_colliding(&self, pt: &Ball2D) -> bool {
        self.get_ball_vec()
            .iter()
            .map(|ball_rc| ball_rc.borrow())
            .any(|ball| self.physics.is_colliding(pt, &(*ball)))
    }

    fn add_candidate_px(&mut self, x_px: f32, y_px: f32) -> GameResult {
        let ball_pos = self.get_window_definition().px_to_coord(x_px, y_px);

        let ball_v = if self.new_ball.rand_speed {
            let mut rng = rng();
            let normal = self.calculate_normal_distribution_of_speed();
            Vec2D::new(normal.sample(&mut rng), normal.sample(&mut rng))
        } else {
            Vec2D::new(0., 0.)
        };
        let ball_size = self.new_ball.get_ball_size();
        let ball_mass = self.new_ball.get_ball_mass();

        let candidate = self
            .get_game_logic_data()
            .get_ball_factory()
            .create_massive_with_velocity(ball_pos, ball_size, ball_mass, ball_v);

        if !self.is_ball_in_region(&candidate) {
            return Err(GameError::CustomError(
                "new ball not within region".to_string(),
            ));
        }
        if self.is_ball_colliding(&candidate) {
            return Err(GameError::CustomError(
                "new ball colliding with existing ball".to_string(),
            ));
        }
        self.add_candidate_to_vec(candidate);
        self.sim.clear_energy();
        Ok(())
    }

    // fn collect_neighbours(&mut self, dt: f32) {
    //     if self.step_count % self.get_game_logic_data().neighbour_update_interval == 0 {
    //         let balls = self.get_ball_vec();
    //         for i in 0..balls.len() {
    //             let ball_radius = { balls[i].borrow().radius };
    //             let mut ball = balls[i].borrow_mut();
    //
    //             let neighbours = balls
    //                 .iter()
    //                 .enumerate()
    //                 .filter_map(|(j, rc)| (j != i).then_some(rc));
    //             Verlet::update_hood(
    //                 &mut *ball,
    //                 dt,
    //                 neighbours,
    //                 self.physics.get_cutoff_distance(),
    //                 self.physics.get_min_movement_for_update_hood(ball_radius),
    //             );
    //         }
    //
    //         // Update stats
    //         for ball_rc in self.get_ball_vec().iter() {
    //             let ball_pos = ball_rc.borrow().get_position().clone();
    //             let mut ball = ball_rc.borrow_mut();
    //             ball.hood.update_closeness_stats(&ball_pos);
    //             Self::log_closeness_update(&ball);
    //         }
    //     }
    // }

    fn collect_neighbours_by_cell(&mut self, dt: f32) {
        if self.step_count % self.get_game_logic_data().neighbour_update_interval == 0 {
            let (rows, cols) = self.cell_grid.get_grid_size();

            for row in 0..rows {
                for col in 0..cols {
                    if let Some(cell) = self.cell_grid.get_cell(row, col) {
                        let neighbor_balls = self.cell_grid.get_balls_in_neighborhood(row, col);

                        for ball_rc in &cell.balls {
                            let ball_radius = { ball_rc.borrow().radius };
                            let mut ball = ball_rc.borrow_mut();

                            let neighbors = neighbor_balls
                                .iter()
                                .filter(|neighbor| !std::ptr::eq(neighbor.as_ptr(), ball_rc.as_ptr()));

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

    fn simulation_step(&mut self) -> GameResult {
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
        let bound_rect = self.get_window_definition().compute_world_bounds_rect();

        let start_all = Timer::start();
        self.collect_neighbours_by_cell(dt);
        let collecting_ms = start_all.ms();

        let start = Timer::start();

        // Store old positions before update
        let old_positions: Vec<_> = self.get_ball_vec()
            .iter()
            .map(|ball| ball.borrow().get_position().clone())
            .collect();

        self.update_sim(dt, &bound_rect)?;

        // Update cell grid with new positions
        // Update cell grid with new positions
        let updates: Vec<_> = self.get_ball_vec()
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
        let updating_ms = start.ms();

        let start = Timer::start();

        let ball_vec = self.get_ball_vec().clone();
        self.sim.calculate_temperature(&ball_vec);

        let props_ms = start.ms();
        let all_ms = start_all.ms();

        self.log_timings(collecting_ms, updating_ms, props_ms, all_ms);

        self.step_count += 1;
        Ok(())
    }
}
