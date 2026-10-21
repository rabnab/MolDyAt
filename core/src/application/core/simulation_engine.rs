// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::step_scratch::{NeighbourStats, RebuildScratch, UpdateScratch};
use super::{RuntimeOptions, SimulationWorld, StatsAccumulator, StepConfig};
use crate::application::phase_timings::{Phase, PhaseClock};
use crate::application::thermo_field::{THERMO_SAMPLE_STRIDE, compute_cell_thermo};
use crate::application::{AppResult, log_util};
use crate::logging::log_update_hood_start;
use crate::objects2d::ball_impl::ForceResult;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::simd_forces::{KernelPoint, force_simd};
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::wall_contact::normal_velocity_after_contact;
use crate::objects2d::{Rect, Vec2D};
use rayon::prelude::*;

/// Stepping/update behavior over the persistent simulation state.
///
/// `MainContent` still owns runtime-only bookkeeping and high-level step
/// orchestration, while the world-only update path here now operates on
/// [`SimulationWorld`] directly.
pub(crate) struct SimulationEngine;

impl SimulationEngine {
    /// `log_timings`: log each phase's time (`RuntimeOptions::show_timings`).
    pub(crate) fn collect_neighbours_by_cell(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        dt: f32,
        log_timings: bool,
    ) {
        if world
            .step_count
            .is_multiple_of(step_config.neighbour_update_interval())
        {
            let mut clock = PhaseClock::start(log_timings);
            Self::update_neighbor_hoods_for_occupied_cells(world, dt);
            Self::update_neighbor_statistics(world);
            clock.lap(&mut world.phase_timings, Phase::Neighbours);
        }
    }

    pub(crate) fn simulation_step(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        options: &RuntimeOptions,
    ) -> AppResult<()> {
        let dt = step_config.get_time_step();
        Self::simulation_step_with_dt(world, step_config, options, dt)
    }

    pub(crate) fn simulation_step_with_dt(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        options: &RuntimeOptions,
        dt: f32,
    ) -> AppResult<()> {
        if world.sim.is_paused() {
            return Ok(());
        }
        let log_timings = options.show_timings;
        if log_timings {
            log_util::log_separator_line();
        }
        if options.step_by_step {
            world.sim.set_pause(true);
        }

        let bound_rect = world.world_rect;

        Self::collect_neighbours_by_cell(world, step_config, dt, log_timings);
        Self::update_sim_and_move_balls(world, step_config, dt, &bound_rect, log_timings);

        let mut clock = PhaseClock::start(log_timings);
        let (sim, balls) = (&mut world.sim, &world.balls);
        sim.calculate_temperature_from_balls(balls);
        clock.lap(&mut world.phase_timings, Phase::Thermo);

        world.step_count += 1;
        world.phase_timings.steps += 1;
        Ok(())
    }

    /// [`Self::record_thermo_fields`] every [`THERMO_SAMPLE_STRIDE`] steps,
    /// and right away when the history is empty (just shown, cleared after a
    /// resize or load), so the heatmap appears without waiting (#229).
    pub(crate) fn record_thermo_fields_if_due(world: &mut SimulationWorld, log_timings: bool) {
        if world.thermo_history.is_empty() || world.step_count.is_multiple_of(THERMO_SAMPLE_STRIDE)
        {
            Self::record_thermo_fields(world, log_timings);
        }
    }

    /// Add the per-cell temperature and pressure of the last step to the
    /// thermo history. Only the heatmap overlay (and headless output) uses
    /// them, so the physics thread calls this only while they are shown
    /// (the `THERMO` layer).
    pub(crate) fn record_thermo_fields(world: &mut SimulationWorld, log_timings: bool) {
        if world.balls.len() != world.cached_force_results.len() {
            return;
        }
        let mut clock = PhaseClock::start(log_timings);
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
        // Booked to `thermo` like the global temperature, but logged under
        // its own name so the two lines of a step can be told apart.
        clock.lap_as(&mut world.phase_timings, Phase::Thermo, "thermo fields");
    }

    pub(crate) fn update_sim(
        world: &mut SimulationWorld,
        dt: f32,
        bound_rect: &Rect,
        log_timings: bool,
    ) {
        let mut stats_accumulator = StatsAccumulator {
            pot: 0.,
            kin: 0.,
            v_com: Vec2D::zero(),
            sum_mass: 0.,
            max_v: 0.,
            max_id: 0,
        };

        let mut clock = PhaseClock::start(log_timings);
        world.with_scratch(
            |s| &mut s.update,
            |world, scratch| {
                // Walls cannot change within a step: one list for the contact
                // sweep and the overlap pass.
                world.collect_solid_obstacles_into(&mut scratch.obstacles);
                let sweep_checks = Self::update_positions(world, dt, bound_rect, scratch);
                Self::update_old_positions(world);
                clock.lap(&mut world.phase_timings, Phase::Positions);

                Self::compute_forces(world, &mut scratch.force_results);
                clock.lap(&mut world.phase_timings, Phase::Forces);

                world.sim.wall_distance_checks += sweep_checks;
                world.sim.wall_contacts += scratch
                    .bounce_infos
                    .iter()
                    .map(|b| b.iter().count() as u64)
                    .sum::<u64>();
                let (wall_px, wall_py) = Self::apply_forces_and_collect_stats(
                    world,
                    &scratch.force_results,
                    &scratch.bounce_infos,
                    dt,
                    bound_rect,
                    &mut stats_accumulator,
                );

                // The previous step's results become the next step's buffer.
                std::mem::swap(&mut world.cached_force_results, &mut scratch.force_results);

                let (seg_px, seg_py) = world
                    .resolve_wall_overlaps_with(&scratch.obstacles, &mut scratch.overlap_passes);
                world.apply_thermostats(dt, &mut stats_accumulator);

                world.sim.wall_momentum_x += wall_px + seg_px;
                world.sim.wall_momentum_y += wall_py + seg_py;
            },
        );

        stats_accumulator.v_com = stats_accumulator
            .v_com
            .scaled(1. / stats_accumulator.sum_mass);
        world.sim.update(dt, stats_accumulator);
        clock.lap(&mut world.phase_timings, Phase::Apply);
    }

    fn update_sim_and_move_balls(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        dt: f32,
        bound_rect: &Rect,
        log_timings: bool,
    ) {
        world.with_scratch(
            |s| &mut s.old_positions,
            |world, old_positions| {
                old_positions.clear();
                old_positions.extend(world.balls.iter().map(|ball| *ball.get_position()));

                Self::update_sim(world, dt, bound_rect, log_timings);
                let mut clock = PhaseClock::start(log_timings);
                world.emit_from_source_spheres(step_config.get_ball_factory());

                for (i, old_pos) in old_positions.iter().enumerate() {
                    let new_pos = *world.balls[i].get_position();
                    world.move_ball_in_cell_grid(i, old_pos, &new_pos);
                }

                world.remove_invalid_balls();
                clock.lap(&mut world.phase_timings, Phase::Cells);
            },
        );
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
        // Every ball is in the cell of its position now, so the grid can be
        // rebuilt if a new species needs wider cells.
        world.fit_cell_grid();
        world.with_scratch(
            |s| &mut s.rebuild,
            |world, scratch| Self::rebuild_neighbor_hoods(world, dt, scratch),
        );
    }

    fn rebuild_neighbor_hoods(world: &mut SimulationWorld, dt: f32, scratch: &mut RebuildScratch) {
        let physics = &world.physics;
        let min_movement =
            physics.get_min_movement_for_update_hood() * world.neighbor_movement_fault_scale();
        let cell_grid = &world.cell_grid;
        let (grid_rows, grid_cols) = cell_grid.get_grid_size();

        // Which cell each ball is in (flat index, `NO_CELL` if none), read from
        // the grid itself: only balls in a cell take part, as in the per-cell
        // loop this replaces.
        const NO_CELL: u32 = u32::MAX;
        let cell_of = &mut scratch.cell_of;
        cell_of.clear();
        cell_of.resize(world.balls.len(), NO_CELL);
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
        // One entry per ball (an indexed collect fills the reused buffer in
        // ball order); `None` for balls with nothing to do.
        let checked = &mut scratch.checked;
        world
            .balls
            .par_iter_mut()
            .zip(cell_of.par_iter())
            .enumerate()
            .map(|(ball_idx, (ball, &flat))| {
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
            .collect_into_vec(checked);

        // Verbose balls are logged here, in ball order, not in the parallel pass.
        for &(ball_idx, _, _, _, verbose) in checked.iter().flatten() {
            if verbose {
                log_update_hood_start(&world.balls[ball_idx], physics.get_skin_thickness());
            }
        }
        let to_rebuild = &mut scratch.to_rebuild;
        to_rebuild.clear();
        to_rebuild.extend(
            checked
                .iter()
                .flatten()
                .filter(|&&(_, _, _, rebuild, _)| rebuild)
                .map(|&(ball_idx, row, col, _, _)| (ball_idx, row, col)),
        );

        let balls = &world.balls;
        let rebuilds = &mut scratch.rebuilds;
        to_rebuild
            .par_iter()
            .map_init(Vec::new, |candidates, &(ball_idx, row, col)| {
                candidates.clear();
                cell_grid.collect_ball_indices_into(row, col, candidates);
                Verlet::rebuild(ball_idx, candidates, physics, balls)
            })
            .collect_into_vec(rebuilds);

        for rebuild in rebuilds.drain(..) {
            Verlet::apply_rebuild(rebuild, physics, &mut world.balls);
        }
    }

    fn update_neighbor_statistics(world: &mut SimulationWorld) {
        world.with_scratch(
            |s| &mut s.neighbour_stats,
            Self::update_neighbor_statistics_into,
        );
    }

    fn update_neighbor_statistics_into(
        world: &mut SimulationWorld,
        stats: &mut Vec<NeighbourStats>,
    ) {
        let balls = &world.balls;
        (0..balls.len())
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
            .collect_into_vec(stats);

        for &(i, num_neighbours, nearest, farthest, should_toggle) in stats.iter() {
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
        scratch: &mut UpdateScratch,
    ) -> u64 {
        let restitution = world.wall_restitution_fault();
        let physics = &world.physics;
        let obstacles = &scratch.obstacles;
        world
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
                    let contact = physics.wall_contact_distance(ball.species());
                    bounce_info.line_wall = SimulationWorld::sweep_wall_contact(
                        obstacles,
                        ball,
                        contact,
                        start,
                        dt,
                        restitution,
                        &mut checks,
                    );
                }
                (bounce_info, checks)
            })
            .unzip_into_vecs(&mut scratch.bounce_infos, &mut scratch.sweep_checks);
        scratch.sweep_checks.iter().sum()
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

    /// Pair forces of every ball, into `results` (replaced, in ball order).
    fn compute_forces(world: &SimulationWorld, results: &mut Vec<ForceResult>) {
        let physics = &world.physics;
        let balls = &world.balls;
        let points = &world.kernel_points;
        (0..balls.len())
            .into_par_iter()
            .map(|i| force_simd(physics, i, balls, points))
            .collect_into_vec(results);
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

    /// `factor` times the shorter world side (`speed_limit_factor` of
    /// `SimConfig`, 20% by default). World rects have a negative height
    /// (y points up), so the extents are taken as absolute values.
    pub(crate) fn default_speed_limit(bound_rect: &Rect, factor: f32) -> f32 {
        f32::min(bound_rect.w.abs(), bound_rect.h.abs()) * factor
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
            .unwrap_or_else(|| Self::default_speed_limit(bound_rect, world.speed_limit_factor));
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
