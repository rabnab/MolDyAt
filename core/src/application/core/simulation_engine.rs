use super::{GameLogicData, MainContent, SimulationWorld, StatsAccumulator};
use crate::application::thermo_field::compute_cell_thermo;
use crate::application::{AppResult, LogStuff};
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::simd_forces::force_simd;
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{BallFactory, ForceResult};
use crate::objects2d::{Rect, Vec2D};
use crate::timing_monad::TimingMonad;
use rayon::prelude::*;

/// Stepping/update behavior over the persistent simulation state.
///
/// `MainContent` still owns runtime-only bookkeeping and high-level step
/// orchestration, while the world-only update path here now operates on
/// [`SimulationWorld`] directly.
pub(crate) struct SimulationEngine;

impl SimulationEngine {
    pub(crate) fn collect_neighbours_by_cell(
        world: &mut SimulationWorld,
        game_data: &GameLogicData,
        dt: f32,
    ) {
        if world
            .step_count
            .is_multiple_of(game_data.neighbour_update_interval())
        {
            let mut neighbor_buf: Vec<usize> = Vec::with_capacity(30);
            let mut timer = TimingMonad::new();
            timer.time("update neighbours", || {
                Self::update_neighbor_hoods_for_occupied_cells(world, dt, &mut neighbor_buf)
            });
            timer.time("update statistics", || {
                Self::update_neighbor_statistics(world)
            });
            if world.show_timings {
                timer.log();
            }
        }
    }

    pub(crate) fn simulation_step(
        world: &mut SimulationWorld,
        game_data: &GameLogicData,
    ) -> AppResult<()> {
        let dt = game_data.get_time_step();
        Self::simulation_step_with_dt(world, game_data, dt)
    }

    pub(crate) fn simulation_step_with_dt(
        world: &mut SimulationWorld,
        game_data: &GameLogicData,
        dt: f32,
    ) -> AppResult<()> {
        if world.sim.is_paused() {
            return Ok(());
        }
        if world.show_timings {
            MainContent::log_separator_line();
        }
        if world.step_by_step {
            world.sim.set_pause(true);
        }

        let bound_rect = world.world_rect;

        #[cfg(feature = "measure-time")]
        let mut timer = TimingMonad::new();

        #[cfg(feature = "measure-time")]
        timer.time("collect_neighbours", || {
            Self::collect_neighbours_by_cell(world, game_data, dt)
        });
        #[cfg(not(feature = "measure-time"))]
        Self::collect_neighbours_by_cell(world, game_data, dt);

        #[cfg(feature = "measure-time")]
        timer.time("update_sim", || {
            Self::update_sim_and_move_balls(world, game_data, dt, &bound_rect);
        });
        #[cfg(not(feature = "measure-time"))]
        Self::update_sim_and_move_balls(world, game_data, dt, &bound_rect);

        #[cfg(feature = "measure-time")]
        timer.time("calculate_temperature", || {
            let (sim, balls) = (&mut world.sim, &world.balls);
            sim.calculate_temperature_from_balls(balls);
        });
        #[cfg(not(feature = "measure-time"))]
        {
            let (sim, balls) = (&mut world.sim, &world.balls);
            sim.calculate_temperature_from_balls(balls);
        }

        #[cfg(feature = "measure-time")]
        if world.show_timings {
            timer.log();
        }

        if world.balls.len() == world.cached_force_results.len() {
            let world_rect = world.world_rect;
            let (phys_rows, phys_cols) = world.cell_grid.get_grid_size();
            let grid_rows = phys_rows.max(1);
            let grid_cols = phys_cols.max(1);
            let (t_grid, p_grid) = compute_cell_thermo(
                &world.balls,
                &world.cached_force_results,
                &world_rect,
                grid_rows,
                grid_cols,
            );
            world.thermo_history.push(t_grid, p_grid);
        }

        world.step_count += 1;
        Ok(())
    }

    pub(crate) fn update_sim(world: &mut SimulationWorld, dt: f32, bound_rect: &Rect) {
        let mut stats_accumulator = StatsAccumulator {
            pot: 0.,
            kin: 0.,
            v_com: Vec2D::zero(),
            sum_mass: 0.,
            max_v: 0.,
            max_id: 0,
        };

        let bounce_infos = Self::update_positions(world, dt, bound_rect);
        Self::update_old_positions(world);

        let force_results = Self::compute_forces(world);
        let (wall_px, wall_py) = Self::apply_forces_and_collect_stats(
            world,
            &force_results,
            &bounce_infos,
            dt,
            bound_rect,
            &mut stats_accumulator,
        );

        world.cached_force_results = force_results;

        let (seg_px, seg_py) = world.resolve_line_segment_collisions();
        world.apply_thermostats(dt, &mut stats_accumulator);

        world.sim.wall_momentum_x += wall_px + seg_px;
        world.sim.wall_momentum_y += wall_py + seg_py;

        stats_accumulator.v_com = stats_accumulator
            .v_com
            .scaled(1. / stats_accumulator.sum_mass);
        world.sim.update(dt, stats_accumulator);
    }

    fn update_sim_and_move_balls(
        world: &mut SimulationWorld,
        game_data: &GameLogicData,
        dt: f32,
        bound_rect: &Rect,
    ) {
        let old_positions: Vec<_> = world
            .balls
            .iter()
            .map(|ball| *ball.get_position())
            .collect();

        Self::update_sim(world, dt, bound_rect);
        world.emit_from_source_spheres(game_data.get_ball_factory());

        for (i, old_pos) in old_positions.iter().enumerate() {
            let new_pos = *world.balls[i].get_position();
            world.move_ball_in_cell_grid(i, old_pos, &new_pos);
        }

        world.remove_invalid_balls();
    }

    fn update_neighbor_hoods_for_occupied_cells(
        world: &mut SimulationWorld,
        dt: f32,
        neighbor_buf: &mut Vec<usize>,
    ) {
        let cutoff_and_skin =
            world.physics.get_cutoff_distance() + world.physics.get_skin_thickness();
        let max_radius = BallFactory::get_max_radius();
        let min_movement = world.physics.get_min_movement_for_update_hood(max_radius)
            * world.neighbor_movement_fault_scale();
        let cell_grid = &world.cell_grid;
        let balls = &mut world.balls;
        let (grid_rows, grid_cols) = cell_grid.get_grid_size();

        for row in 0..grid_rows {
            for col in 0..grid_cols {
                let cell_ball_indices = if let Some(cell) = cell_grid.get_cell(row, col) {
                    cell.get_ball_indices()
                } else {
                    continue;
                };

                neighbor_buf.clear();
                cell_grid.collect_ball_indices_into(row, col, neighbor_buf);

                for &ball_idx in cell_ball_indices {
                    Verlet::update_hood(
                        ball_idx,
                        dt,
                        neighbor_buf,
                        cutoff_and_skin,
                        min_movement,
                        balls,
                    );
                }
            }
        }
    }

    fn update_neighbor_statistics(world: &mut SimulationWorld) {
        let balls = &world.balls;
        let stats: Vec<(usize, usize, f32, f32, bool)> = (0..balls.len())
            .into_par_iter()
            .map(|i| {
                let ball = &balls[i];

                if !ball.hood.hood_dirty {
                    return (i, 0, f32::NAN, f32::NAN, false);
                }

                let ball_pos = *ball.get_position();
                let neighbour_indices = ball.hood.get_neighbour_indices();
                let verbose_lines = ball.verbose.lines();

                let (num_neighbours, nearest, farthest) =
                    Verlet::compute_neighbor_stats(&ball_pos, neighbour_indices, balls);

                (
                    i,
                    num_neighbours,
                    nearest,
                    farthest,
                    !verbose_lines && num_neighbours > 100,
                )
            })
            .collect();

        for (i, num_neighbours, nearest, farthest, should_toggle) in stats.into_iter() {
            if world.balls[i].hood.hood_dirty {
                let ball_hood = &mut world.balls[i].hood;
                ball_hood.num_neighbours = num_neighbours;
                ball_hood.nearest = nearest;
                ball_hood.farthest = farthest;
                ball_hood.hood_dirty = false;

                if should_toggle {
                    world.balls[i].verbose.toggle_other_neighbour();
                }
                MainContent::log_closeness_update(&world.balls[i]);
            }
        }
    }

    fn update_positions(
        world: &mut SimulationWorld,
        dt: f32,
        bound_rect: &Rect,
    ) -> Vec<BounceInfo> {
        let physics = &world.physics;
        world
            .balls
            .par_iter_mut()
            .map(|ball| physics.update_positions(ball, dt, bound_rect))
            .collect()
    }

    fn update_old_positions(world: &mut SimulationWorld) {
        world
            .balls
            .par_iter_mut()
            .for_each(|ball| ball.update_old_pos());
    }

    fn compute_forces(world: &SimulationWorld) -> Vec<ForceResult> {
        let physics = &world.physics;
        let balls = &world.balls;
        (0..balls.len())
            .into_par_iter()
            .map(|i| force_simd(physics, i, balls))
            .collect()
    }

    fn apply_single_force_and_collect_stats(
        world: &mut SimulationWorld,
        ball_idx: usize,
        result: &ForceResult,
        bounce_info: &BounceInfo,
        dt: f32,
        speed_limit: f32,
        stats_accumulator: &mut StatsAccumulator,
    ) -> (f32, f32) {
        let gravity = world.physics.constants.gravity;
        let restitution = world.wall_restitution_fault();
        let ball = &mut world.balls[ball_idx];

        let mut wall_px = 0.0;
        let mut wall_py = 0.0;

        let accel = ball.accel_with_speed_limit(&result.force, gravity, speed_limit, dt);

        if bounce_info.bounced_x || bounce_info.bounced_y {
            let t_collision = bounce_info.collision_time_fraction;
            let v_at_collision_x = ball.v.x + 0.5 * (ball.accel.x + accel.x) * dt * t_collision;
            let v_at_collision_y = ball.v.y + 0.5 * (ball.accel.y + accel.y) * dt * t_collision;

            if bounce_info.bounced_x {
                wall_px = 2.0 * ball.mass * v_at_collision_x;
            }
            if bounce_info.bounced_y {
                wall_py = 2.0 * ball.mass * v_at_collision_y;
            }

            let v_after_bounce_x = if bounce_info.bounced_x {
                -restitution * v_at_collision_x
            } else {
                v_at_collision_x
            };
            let v_after_bounce_y = if bounce_info.bounced_y {
                -restitution * v_at_collision_y
            } else {
                v_at_collision_y
            };

            ball.v.x = v_after_bounce_x + 0.5 * (ball.accel.x + accel.x) * dt * (1.0 - t_collision);
            ball.v.y = v_after_bounce_y + 0.5 * (ball.accel.y + accel.y) * dt * (1.0 - t_collision);
        } else {
            ball.v.x += 0.5 * (accel.x + ball.accel.x) * dt;
            ball.v.y += 0.5 * (accel.y + ball.accel.y) * dt;
        }

        ball.accel = accel;

        if ball.verbose.console {
            println!("updated to  v {:?}  a {:?}", ball.v, ball.accel);
        }

        let momentum = ball.moment();
        let speed_sq = ball.v.norm_sq();
        let mass = ball.mass;
        let ball_id = ball.get_id();

        stats_accumulator.pot += result.potential_energy * 0.5;
        stats_accumulator.v_com += momentum;
        stats_accumulator.kin += speed_sq * mass * 0.5;
        stats_accumulator.sum_mass += mass;
        if stats_accumulator.max_v * stats_accumulator.max_v < speed_sq {
            stats_accumulator.max_v = f32::sqrt(speed_sq);
            stats_accumulator.max_id = ball_id;
        }

        (wall_px, wall_py)
    }

    fn apply_forces_and_collect_stats(
        world: &mut SimulationWorld,
        force_results: &[ForceResult],
        bounce_infos: &[BounceInfo],
        dt: f32,
        bound_rect: &Rect,
        stats_accumulator: &mut StatsAccumulator,
    ) -> (f32, f32) {
        let speed_limit = world
            .speed_limit_override
            .unwrap_or_else(|| f32::min(bound_rect.w, bound_rect.h) * 0.2);
        let mut total_wall_px = 0.0;
        let mut total_wall_py = 0.0;

        for (i, result) in force_results.iter().enumerate() {
            let (wall_px, wall_py) = Self::apply_single_force_and_collect_stats(
                world,
                i,
                result,
                &bounce_infos[i],
                dt,
                speed_limit,
                stats_accumulator,
            );
            total_wall_px += wall_px;
            total_wall_py += wall_py;
        }

        (total_wall_px, total_wall_py)
    }
}
