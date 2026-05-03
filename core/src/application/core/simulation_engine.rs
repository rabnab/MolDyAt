use super::{MainContent, StatsAccumulator};
use crate::application::thermo_field::compute_cell_thermo;
use crate::application::{AppResult, LogStuff};
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::simd_forces::force_simd;
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{Ball2D, BallFactory, ForceResult};
use crate::objects2d::{Rect, Vec2D};
use crate::timing_monad::TimingMonad;
use rayon::prelude::*;

/// Stepping/update behavior over the persistent simulation state held by `MainContent`.
///
/// `MainContent` remains the compatibility facade for now, while simulation-step
/// orchestration and Verlet update logic live here.
pub(crate) struct SimulationEngine;

impl SimulationEngine {
    pub(crate) fn collect_neighbours_by_cell(content: &mut MainContent, dt: f32) {
        if content
            .step_count
            .is_multiple_of(content.get_game_logic_data().neighbour_update_interval())
        {
            let mut neighbor_buf: Vec<usize> = Vec::with_capacity(30);
            let mut timer = TimingMonad::new();
            timer.time("update neighbours", || {
                Self::update_neighbor_hoods_for_occupied_cells(content, dt, &mut neighbor_buf)
            });
            timer.time("update statistics", || {
                Self::update_neighbor_statistics(content)
            });
            content.log_timings(timer);
        }
    }

    pub(crate) fn simulation_step(content: &mut MainContent) -> AppResult<()> {
        content.sim.inc_frame_since_draw();
        if content.sim.is_paused() {
            return Ok(());
        }
        if content.any_log() {
            MainContent::log_separator_line();
        }
        if content.step_by_step {
            content.sim.set_pause(true);
        }

        let dt = content.get_game_logic_data().get_time_step();
        let bound_rect = content.world_region();

        #[cfg(feature = "measure-time")]
        let mut timer = TimingMonad::new();

        #[cfg(feature = "measure-time")]
        timer.time("collect_neighbours", || {
            Self::collect_neighbours_by_cell(content, dt)
        });
        #[cfg(not(feature = "measure-time"))]
        Self::collect_neighbours_by_cell(content, dt);

        #[cfg(feature = "measure-time")]
        timer.time("update_sim", || {
            Self::update_sim_and_move_balls(content, dt, &bound_rect);
        });
        #[cfg(not(feature = "measure-time"))]
        Self::update_sim_and_move_balls(content, dt, &bound_rect);

        #[cfg(feature = "measure-time")]
        timer.time("calculate_temperature", || {
            let (sim, balls) = (&mut content.world.sim, &content.world.balls);
            sim.calculate_temperature_from_balls(balls);
        });
        #[cfg(not(feature = "measure-time"))]
        {
            let (sim, balls) = (&mut content.world.sim, &content.world.balls);
            sim.calculate_temperature_from_balls(balls);
        }

        #[cfg(feature = "measure-time")]
        content.log_timings(timer);

        if content.balls.len() == content.cached_force_results.len() {
            let world = content.world_region();
            let (phys_rows, phys_cols) = content.cell_grid.get_grid_size();
            let grid_rows = (phys_rows / 2).max(1);
            let grid_cols = (phys_cols / 2).max(1);
            let (t_grid, p_grid) = compute_cell_thermo(
                &content.balls,
                &content.cached_force_results,
                &world,
                grid_rows,
                grid_cols,
            );
            content.thermo_history.push(t_grid, p_grid);
        }

        content.step_count += 1;
        Ok(())
    }

    pub(crate) fn update_sim(content: &mut MainContent, dt: f32, bound_rect: &Rect) {
        let mut stats_accumulator = StatsAccumulator {
            pot: 0.,
            kin: 0.,
            v_com: Vec2D::zero(),
            sum_mass: 0.,
            max_v: 0.,
            max_id: 0,
        };

        let bounce_infos = Self::update_positions(content, dt, bound_rect);
        Self::update_old_positions(content);
        content.sim.reset_min_dist(bound_rect);

        let force_results = Self::compute_forces(content);
        let (wall_px, wall_py) = Self::apply_forces_and_collect_stats(
            content,
            &force_results,
            &bounce_infos,
            dt,
            bound_rect,
            &mut stats_accumulator,
        );

        content.cached_force_results = force_results;

        let (seg_px, seg_py) = content.resolve_line_segment_collisions();
        content.apply_thermostats(dt, &mut stats_accumulator);
        content.emit_from_source_spheres();

        content.sim.wall_momentum_x += wall_px + seg_px;
        content.sim.wall_momentum_y += wall_py + seg_py;

        stats_accumulator.v_com = stats_accumulator
            .v_com
            .scaled(1. / stats_accumulator.sum_mass);
        content.sim.update(dt, stats_accumulator);
    }

    pub(crate) fn is_ball_in_region(content: &MainContent, pt: &Ball2D) -> bool {
        let world = content.world_region_inset(pt.radius);
        let pos = pt.get_position();

        pos.x >= world.x
            && pos.x <= (world.x + world.w)
            && pos.y >= (world.y + world.h)
            && pos.y <= world.y
    }

    pub(crate) fn is_ball_colliding(content: &MainContent, pt: &Ball2D, margin: f32) -> bool {
        content
            .get_balls()
            .iter()
            .any(|ball| content.physics.is_colliding(pt, ball, margin))
    }

    fn update_sim_and_move_balls(content: &mut MainContent, dt: f32, bound_rect: &Rect) {
        let old_positions: Vec<_> = content
            .get_balls()
            .iter()
            .map(|ball| *ball.get_position())
            .collect();

        Self::update_sim(content, dt, bound_rect);

        for (i, old_pos) in old_positions.iter().enumerate() {
            let new_pos = *content.world.balls[i].get_position();
            content.world.move_ball_in_cell_grid(i, old_pos, &new_pos);
        }

        content.remove_invalid_balls();
    }

    fn update_neighbor_hoods_for_occupied_cells(
        content: &mut MainContent,
        dt: f32,
        neighbor_buf: &mut Vec<usize>,
    ) {
        let cutoff_and_skin =
            content.physics.get_cutoff_distance() + content.physics.get_skin_thickness();
        let max_radius = BallFactory::get_max_radius();
        let min_movement = content.physics.get_min_movement_for_update_hood(max_radius);
        let (grid_rows, grid_cols) = content.cell_grid.get_grid_size();

        for row in 0..grid_rows {
            for col in 0..grid_cols {
                let cell_ball_indices = if let Some(cell) = content.cell_grid.get_cell(row, col) {
                    cell.get_ball_indices().to_vec()
                } else {
                    continue;
                };

                neighbor_buf.clear();
                content
                    .cell_grid
                    .collect_ball_indices_into(row, col, neighbor_buf);

                for ball_idx in cell_ball_indices {
                    Verlet::update_hood(
                        ball_idx,
                        dt,
                        neighbor_buf,
                        cutoff_and_skin,
                        min_movement,
                        &mut content.balls,
                    );
                }
            }
        }
    }

    fn update_neighbor_statistics(content: &mut MainContent) {
        let balls = &content.world.balls;
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
            if content.balls[i].hood.hood_dirty {
                let ball_hood = &mut content.balls[i].hood;
                ball_hood.num_neighbours = num_neighbours;
                ball_hood.nearest = nearest;
                ball_hood.farthest = farthest;
                ball_hood.hood_dirty = false;

                if should_toggle {
                    content.balls[i].verbose.toggle_other_neighbour();
                }
                MainContent::log_closeness_update(&content.balls[i]);
            }
        }
    }

    fn update_positions(content: &mut MainContent, dt: f32, bound_rect: &Rect) -> Vec<BounceInfo> {
        let physics = &content.world.physics;
        content
            .world
            .balls
            .par_iter_mut()
            .map(|ball| physics.update_positions(ball, dt, bound_rect))
            .collect()
    }

    fn update_old_positions(content: &mut MainContent) {
        content
            .world
            .balls
            .par_iter_mut()
            .for_each(|ball| ball.update_old_pos());
    }

    fn compute_forces(content: &MainContent) -> Vec<ForceResult> {
        let physics = &content.physics;
        let balls = &content.balls;
        (0..balls.len())
            .into_par_iter()
            .map(|i| force_simd(physics, i, balls))
            .collect()
    }

    fn apply_single_force_and_collect_stats(
        content: &mut MainContent,
        ball_idx: usize,
        result: &ForceResult,
        bounce_info: &BounceInfo,
        dt: f32,
        speed_limit: f32,
        stats_accumulator: &mut StatsAccumulator,
    ) -> (f32, f32) {
        let gravity = content.world.physics.constants.gravity;
        let ball = &mut content.world.balls[ball_idx];

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
                -v_at_collision_x
            } else {
                v_at_collision_x
            };
            let v_after_bounce_y = if bounce_info.bounced_y {
                -v_at_collision_y
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

        content.sim.update_min_dist(result.min_dist);
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
        content: &mut MainContent,
        force_results: &[ForceResult],
        bounce_infos: &[BounceInfo],
        dt: f32,
        bound_rect: &Rect,
        stats_accumulator: &mut StatsAccumulator,
    ) -> (f32, f32) {
        let speed_limit = content
            .speed_limit_override
            .unwrap_or_else(|| f32::min(bound_rect.w, bound_rect.h) * 0.2);
        let mut total_wall_px = 0.0;
        let mut total_wall_py = 0.0;

        for (i, result) in force_results.iter().enumerate() {
            let (wall_px, wall_py) = Self::apply_single_force_and_collect_stats(
                content,
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
