// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::line_segments::Obstacle;
use super::{SimulationWorld, StatsAccumulator, StepConfig};
use crate::application::thermo_field::compute_cell_thermo;
use crate::application::{AppResult, log_util};
use crate::logging::log_update_hood_start;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::simd_forces::{KernelPoint, force_simd};
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::wall_contact::normal_velocity_after_contact;
use crate::objects2d::ball_impl::{BallFactory, ForceResult};
use crate::objects2d::{Rect, Vec2D};
use crate::timing_monad::TimingMonad;
use rayon::prelude::*;
use std::time::Instant;

/// Stepping/update behavior over the persistent simulation state.
///
/// `MainContent` still owns runtime-only bookkeeping and high-level step
/// orchestration, while the world-only update path here now operates on
/// [`SimulationWorld`] directly.
pub(crate) struct SimulationEngine;

impl SimulationEngine {
    pub(crate) fn collect_neighbours_by_cell(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        dt: f32,
    ) {
        if world
            .step_count
            .is_multiple_of(step_config.neighbour_update_interval())
        {
            let start = Instant::now();
            let mut timer = TimingMonad::new();
            timer.time("update neighbours", || {
                Self::update_neighbor_hoods_for_occupied_cells(world, dt)
            });
            timer.time("update statistics", || {
                Self::update_neighbor_statistics(world)
            });
            if world.show_timings {
                timer.log();
            }
            world.phase_timings.neighbours += start.elapsed();
        }
    }

    pub(crate) fn simulation_step(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
    ) -> AppResult<()> {
        let dt = step_config.get_time_step();
        Self::simulation_step_with_dt(world, step_config, dt)
    }

    pub(crate) fn simulation_step_with_dt(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        dt: f32,
    ) -> AppResult<()> {
        if world.sim.is_paused() {
            return Ok(());
        }
        if world.show_timings {
            log_util::log_separator_line();
        }
        if world.step_by_step {
            world.sim.set_pause(true);
        }

        let bound_rect = world.world_rect;

        #[cfg(feature = "measure-time")]
        let mut timer = TimingMonad::new();

        #[cfg(feature = "measure-time")]
        timer.time("collect_neighbours", || {
            Self::collect_neighbours_by_cell(world, step_config, dt)
        });
        #[cfg(not(feature = "measure-time"))]
        Self::collect_neighbours_by_cell(world, step_config, dt);

        #[cfg(feature = "measure-time")]
        timer.time("update_sim", || {
            Self::update_sim_and_move_balls(world, step_config, dt, &bound_rect);
        });
        #[cfg(not(feature = "measure-time"))]
        Self::update_sim_and_move_balls(world, step_config, dt, &bound_rect);

        let thermo_start = Instant::now();
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
        world.phase_timings.thermo += thermo_start.elapsed();

        #[cfg(feature = "measure-time")]
        if world.show_timings {
            timer.log();
        }

        world.step_count += 1;
        world.phase_timings.steps += 1;
        Ok(())
    }

    /// Add the per-cell temperature and pressure of the last step to the
    /// thermo history. Only the heatmap overlay (and headless output) uses
    /// them, so the physics thread calls this only while they are shown
    /// (the `THERMO` layer).
    pub(crate) fn record_thermo_fields(world: &mut SimulationWorld) {
        if world.balls.len() != world.cached_force_results.len() {
            return;
        }
        let start = Instant::now();
        // The T/P overlay uses the physics cell grid's shape (see
        // docs/simulation.md, Thermodynamic Grid Space).
        let (phys_rows, phys_cols) = world.cell_grid.get_grid_size();
        let (t_grid, p_grid) = compute_cell_thermo(
            &world.balls,
            &world.cached_force_results,
            &world.world_rect,
            phys_rows.max(1),
            phys_cols.max(1),
        );
        world.thermo_history.push(t_grid, p_grid);
        world.phase_timings.thermo += start.elapsed();
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

        let start = Instant::now();
        // Walls cannot change within a step: one list for the contact sweep
        // and the overlap pass.
        let obstacles = world.collect_solid_obstacles();
        let (bounce_infos, sweep_checks) =
            Self::update_positions(world, dt, bound_rect, &obstacles);
        world.sim.wall_distance_checks += sweep_checks;
        world.sim.wall_contacts += bounce_infos
            .iter()
            .map(|b| b.iter().count() as u64)
            .sum::<u64>();
        Self::update_old_positions(world);
        let positions_done = Instant::now();
        world.phase_timings.positions += positions_done - start;

        let force_results = Self::compute_forces(world);
        let forces_done = Instant::now();
        world.phase_timings.forces += forces_done - positions_done;
        let (wall_px, wall_py) = Self::apply_forces_and_collect_stats(
            world,
            &force_results,
            &bounce_infos,
            dt,
            bound_rect,
            &mut stats_accumulator,
        );

        world.cached_force_results = force_results;

        let (seg_px, seg_py) = world.resolve_wall_overlaps(&obstacles);
        world.apply_thermostats(dt, &mut stats_accumulator);

        world.sim.wall_momentum_x += wall_px + seg_px;
        world.sim.wall_momentum_y += wall_py + seg_py;

        stats_accumulator.v_com = stats_accumulator
            .v_com
            .scaled(1. / stats_accumulator.sum_mass);
        world.sim.update(dt, stats_accumulator);
        world.phase_timings.apply += forces_done.elapsed();
    }

    fn update_sim_and_move_balls(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        dt: f32,
        bound_rect: &Rect,
    ) {
        let old_positions: Vec<_> = world
            .balls
            .iter()
            .map(|ball| *ball.get_position())
            .collect();

        Self::update_sim(world, dt, bound_rect);
        let start = Instant::now();
        world.emit_from_source_spheres(step_config.get_ball_factory());

        for (i, old_pos) in old_positions.iter().enumerate() {
            let new_pos = *world.balls[i].get_position();
            world.move_ball_in_cell_grid(i, old_pos, &new_pos);
        }

        world.remove_invalid_balls();
        world.phase_timings.cells += start.elapsed();
    }

    /// Rebuild the neighbour lists of the balls that moved far enough.
    ///
    /// 1. Parallel: per-ball counters, and which balls must rebuild.
    /// 2. Parallel: each such ball builds its new list from its 3x3 cells,
    ///    reading only, and diffs it against its current list.
    /// 3. Serial, in ball order: install the lists and add or remove the
    ///    changed partners. The result does not depend on that order (see
    ///    `Verlet::apply_rebuild`), so it equals a serial rebuild.
    fn update_neighbor_hoods_for_occupied_cells(world: &mut SimulationWorld, dt: f32) {
        let cutoff_and_skin =
            world.physics.get_cutoff_distance() + world.physics.get_skin_thickness();
        let max_radius = BallFactory::get_max_radius();
        let min_movement = world.physics.get_min_movement_for_update_hood(max_radius)
            * world.neighbor_movement_fault_scale();
        let cell_grid = &world.cell_grid;
        let (grid_rows, grid_cols) = cell_grid.get_grid_size();

        // Which cell each ball is in (flat index, `NO_CELL` if none), read from
        // the grid itself: only balls in a cell take part, as in the per-cell
        // loop this replaces.
        const NO_CELL: u32 = u32::MAX;
        let mut cell_of = vec![NO_CELL; world.balls.len()];
        for row in 0..grid_rows {
            for col in 0..grid_cols {
                if let Some(cell) = cell_grid.get_cell(row, col) {
                    let flat = (row * grid_cols + col) as u32;
                    for &ball_idx in cell.get_ball_indices() {
                        cell_of[ball_idx] = flat;
                    }
                }
            }
        }

        // (ball, row, col, rebuild, verbose) for balls that rebuild or log.
        let checked: Vec<(usize, usize, usize, bool, bool)> = world
            .balls
            .par_iter_mut()
            .zip(cell_of.par_iter())
            .enumerate()
            .filter_map(|(ball_idx, (ball, &flat))| {
                if flat == NO_CELL {
                    return None;
                }
                let (row, col) = (flat as usize / grid_cols, flat as usize % grid_cols);
                // Rebuilds only search the 3x3 cells around the ball's cell,
                // so that cell must match its position.
                debug_assert_eq!(
                    cell_grid.get_cell_indices(ball.get_position()),
                    Some((row, col)),
                    "ball {ball_idx} is in cell ({row}, {col}) of the grid but not by position"
                );
                let verbose = ball.verbose.console;
                let rebuild = Verlet::begin_update(ball, dt, min_movement);
                (rebuild || verbose).then_some((ball_idx, row, col, rebuild, verbose))
            })
            .collect();

        // Verbose balls are logged here, in ball order, not in the parallel pass.
        for &(ball_idx, _, _, _, verbose) in &checked {
            if verbose {
                log_update_hood_start(&world.balls[ball_idx], cutoff_and_skin);
            }
        }
        let to_rebuild: Vec<(usize, usize, usize)> = checked
            .into_iter()
            .filter(|&(_, _, _, rebuild, _)| rebuild)
            .map(|(ball_idx, row, col, _, _)| (ball_idx, row, col))
            .collect();

        let balls = &world.balls;
        let rebuilds: Vec<_> = to_rebuild
            .par_iter()
            .map_init(Vec::new, |candidates, &(ball_idx, row, col)| {
                candidates.clear();
                cell_grid.collect_ball_indices_into(row, col, candidates);
                Verlet::rebuild(ball_idx, candidates, cutoff_and_skin, balls)
            })
            .collect();

        for rebuild in rebuilds {
            Verlet::apply_rebuild(rebuild, cutoff_and_skin, &mut world.balls);
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
                log_util::log_closeness_update(&world.balls[i]);
            }
        }
    }

    fn update_positions(
        world: &mut SimulationWorld,
        dt: f32,
        bound_rect: &Rect,
        obstacles: &[Obstacle],
    ) -> (Vec<BounceInfo>, u64) {
        let restitution = world.wall_restitution_fault();
        let physics = &world.physics;
        let results: Vec<(BounceInfo, u64)> = world
            .balls
            .par_iter_mut()
            .map(|ball| {
                let start = *ball.get_position();
                let mut bounce_info = physics.update_positions(ball, dt, bound_rect, restitution);
                let mut checks = 0;
                // A ball that reached a box wall is not swept against line
                // walls this step (the contact normals must stay orthogonal);
                // if it ends inside one, the overlap pass handles it.
                if !bounce_info.touched_box() {
                    bounce_info.line_wall = SimulationWorld::sweep_wall_contact(
                        obstacles,
                        ball,
                        start,
                        dt,
                        restitution,
                        &mut checks,
                    );
                }
                (bounce_info, checks)
            })
            .collect();
        let checks = results.iter().map(|(_, c)| c).sum();
        (results.into_iter().map(|(b, _)| b).collect(), checks)
    }

    /// Store each ball's position before the force pass and copy what the
    /// force kernel reads into `kernel_points`.
    fn update_old_positions(world: &mut SimulationWorld) {
        world
            .kernel_points
            .resize(world.balls.len(), KernelPoint::default());
        world
            .balls
            .par_iter_mut()
            .zip(world.kernel_points.par_iter_mut())
            .for_each(|(ball, point)| {
                ball.update_old_pos();
                *point = KernelPoint::from(&*ball);
            });
    }

    fn compute_forces(world: &SimulationWorld) -> Vec<ForceResult> {
        let physics = &world.physics;
        let balls = &world.balls;
        let points = &world.kernel_points;
        (0..balls.len())
            .into_par_iter()
            .map(|i| force_simd(physics, i, balls, points))
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

        // Velocity-Verlet kick; contacts replace the normal component with
        // the reflected one (see `wall_contact`), and the walls take the
        // momentum difference.
        let kick = (accel + ball.accel).scaled(0.5 * dt);
        let mut v_new = ball.v + kick;
        for contact in bounce_info.iter() {
            let normal = contact.normal;
            let v_normal = ball.v.dot(&normal);
            let kick_normal = kick.dot(&normal);
            let v_normal_new =
                normal_velocity_after_contact(v_normal, kick_normal, contact, restitution);
            let dv = v_normal_new - (v_normal + kick_normal);
            v_new = v_new.add_scaled(&normal, dv);
            if contact.counts_for_momentum {
                wall_px -= ball.mass * dv * normal.x;
                wall_py -= ball.mass * dv * normal.y;
            }
        }
        ball.v = v_new;

        ball.accel = accel;

        if ball.verbose.console {
            log::debug!(
                target: crate::logging::VERBOSE_TARGET,
                "updated to  v {:?}  a {:?}",
                ball.v,
                ball.accel
            );
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

    /// 20% of the shorter world side. World rects have a negative height
    /// (y points up), so the extents are taken as absolute values.
    pub(crate) fn default_speed_limit(bound_rect: &Rect) -> f32 {
        f32::min(bound_rect.w.abs(), bound_rect.h.abs()) * 0.2
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
            .unwrap_or_else(|| Self::default_speed_limit(bound_rect));
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
