// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::line_segments::Obstacle;
use super::step_scratch::{NeighbourStats, RebuildScratch, UpdateScratch};
use super::wall_forces::{molecule_backstop, soft_wall_force};
use super::{RuntimeOptions, SimulationWorld, StatsAccumulator, StepConfig};
use crate::application::phase_timings::{Phase, PhaseClock};
use crate::application::sim_stats::SafetyNets;
use crate::application::thermo_field::{
    THERMO_SAMPLE_STRIDE, compute_cell_fields, thermo_grid_dims,
};
use crate::application::{AppResult, log_util};
use crate::logging::log_update_hood_start;
use crate::objects2d::ball_impl::ball_core::{Ball2D, BounceInfo};
use crate::objects2d::ball_impl::external_field::{
    ExternalField, Field, UniformField, fields_acceleration,
};
use crate::objects2d::ball_impl::molecule::axis;
use crate::objects2d::ball_impl::simd_forces::{KernelPoint, force_simd};
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::wall_contact::normal_velocity_after_contact;
use crate::objects2d::ball_impl::wall_law::WALL_RAMP_TIME;
use crate::objects2d::ball_impl::{ForceResult, SpeciesSet};
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
            clock.lap(&mut world.cache.phase_timings, Phase::Neighbours);
        }
    }

    #[cfg(test)]
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

        world.step_count += 1;
        world.cache.phase_timings.steps += 1;
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

    /// Add the per-cell fields (temperature, pressure, density, flow
    /// velocity) of the last step to the thermo history. Only the field
    /// overlay (and headless output) uses them, so the physics thread calls
    /// this only while they are shown (the `THERMO` layer).
    pub(crate) fn record_thermo_fields(world: &mut SimulationWorld, log_timings: bool) {
        if world.balls.len() != world.cache.force_results.len() {
            return;
        }
        let mut clock = PhaseClock::start(log_timings);
        // The per-cell fields have a grid of their own, sized from the
        // world only (#290, docs/simulation.md, Thermodynamic Grid Space).
        let (rows, cols) = thermo_grid_dims(world.world_rect.w, world.world_rect.h);
        let fields = compute_cell_fields(
            &world.balls,
            &world.cache.force_results,
            &world.world_rect,
            rows,
            cols,
        );
        world.thermo_history.push(fields);
        clock.lap(&mut world.cache.phase_timings, Phase::Thermo);
    }

    pub(crate) fn update_sim(
        world: &mut SimulationWorld,
        dt: f32,
        bound_rect: &Rect,
        log_timings: bool,
    ) {
        let mut stats_accumulator = StatsAccumulator::default();

        let mut clock = PhaseClock::start(log_timings);
        world.with_scratch(
            |s| &mut s.update,
            |world, scratch| {
                // Walls cannot change within a step: one list for the contact
                // sweep and the overlap pass.
                world.collect_solid_obstacles_into(&mut scratch.obstacles);
                let sweep_checks = Self::update_positions(world, dt, bound_rect, scratch);
                Self::update_old_positions(world);
                clock.lap(&mut world.cache.phase_timings, Phase::Positions);

                Self::compute_forces(
                    world,
                    &scratch.obstacles,
                    bound_rect,
                    &mut scratch.force_results,
                );
                clock.lap(&mut world.cache.phase_timings, Phase::Forces);

                world.sim.wall_distance_checks += sweep_checks;
                let contacts = scratch
                    .bounce_infos
                    .iter()
                    .map(|b| b.iter().count() as u64 + u64::from(b.impulses))
                    .sum::<u64>();
                world.sim.wall_contacts += contacts;
                // Safety nets of this step (#422); braking and capped pairs
                // are summed in the apply pass.
                let mut nets = SafetyNets {
                    backstop_hits: if world.physics.walls().soft {
                        contacts
                    } else {
                        0
                    },
                    ..SafetyNets::default()
                };
                let (wall_px, wall_py) = Self::apply_forces_and_collect_stats(
                    world,
                    &scratch.force_results,
                    &scratch.bounce_infos,
                    &mut scratch.apply_stats,
                    dt,
                    bound_rect,
                    &mut stats_accumulator,
                );

                // The previous step's results become the next step's buffer.
                std::mem::swap(&mut world.cache.force_results, &mut scratch.force_results);

                let corrections_before = world.sim.wall_overlap_corrections;
                let (seg_px, seg_py) = world
                    .resolve_wall_overlaps_with(&scratch.obstacles, &mut scratch.overlap_passes);
                nets.overlap_corrections = world.sim.wall_overlap_corrections - corrections_before;
                nets.braked = stats_accumulator.braked;
                nets.capped_pairs = stats_accumulator.capped_pairs / 2;
                world.sim.safety_nets_step = nets;
                world.sim.safety_nets_total += nets;
                world.apply_thermostats(dt, &mut stats_accumulator);

                world.sim.wall_momentum_x += wall_px + f64::from(seg_px);
                world.sim.wall_momentum_y += wall_py + f64::from(seg_py);
            },
        );

        // The external fields' potential energy (gravity, `[[fields]]`) from
        // the positions at the end of the step, after the wall-overlap
        // fallback moved balls (#334, #347). Skipped without fields, so
        // those runs stay bit-identical.
        //
        // Gravity is a `UniformField` too, but stays out of `fields()` on
        // purpose: it is a runtime setting (`G` cycles it, it is not part of
        // the config's `[[fields]]`), its floor follows the box on a resize,
        // and its kick sits inside `accel_with_speed_limit`, where moving it
        // would change the f32 rounding of every gravity run.
        let gravity = world.physics.constants.gravity;
        if gravity != 0.0 {
            stats_accumulator.pot += Self::field_potential_energy(
                world,
                &UniformField {
                    g: gravity,
                    floor: bound_rect.get_tl().1.min(bound_rect.get_br().1),
                },
            );
        }
        for field in world.physics.fields() {
            stats_accumulator.pot += Self::field_potential_energy(world, field);
        }

        stats_accumulator.v_com = Vec2D::new(
            (stats_accumulator.momentum[0] / stats_accumulator.sum_mass) as f32,
            (stats_accumulator.momentum[1] / stats_accumulator.sum_mass) as f32,
        );
        world.sim.update(dt, stats_accumulator);
        clock.lap(&mut world.cache.phase_timings, Phase::Apply);
    }

    fn update_sim_and_move_balls(
        world: &mut SimulationWorld,
        step_config: &StepConfig,
        dt: f32,
        bound_rect: &Rect,
        log_timings: bool,
    ) {
        Self::update_sim(world, dt, bound_rect, log_timings);
        let mut clock = PhaseClock::start(log_timings);
        // Every ball into the cell of its new position: one counting sort
        // (`CellGrid::rebuild`). Emission comes after it, so the grid holds
        // every ball at its position and can check the emitted balls (#250).
        world.cache.cell_grid.rebuild(&world.balls);
        world.emit_from_source_spheres(step_config.get_ball_factory());
        world.remove_invalid_balls();
        let interval = world.reorder_interval as usize;
        if interval > 0 && (world.step_count + 1).is_multiple_of(interval) {
            world.reorder_balls_by_cell();
        }
        clock.lap(&mut world.cache.phase_timings, Phase::Cells);
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
        let cell_grid = &world.cache.cell_grid;
        let (_, grid_cols) = cell_grid.get_grid_size();

        // Which cell the grid holds each ball in (flat index, `NO_CELL` if
        // none): only balls in a cell take part. A ball pushed without
        // being added to the grid is in none.
        const NO_CELL: u32 = u32::MAX;
        let cell_of = cell_grid.flat_cells();

        // (ball, row, col, rebuild, verbose) for balls that rebuild or log.
        // One entry per ball (an indexed collect fills the reused buffer in
        // ball order); `None` for balls with nothing to do.
        let checked = &mut scratch.checked;
        world
            .balls
            .par_iter_mut()
            .enumerate()
            .map(|(ball_idx, ball)| {
                let flat = cell_of.get(ball_idx).copied().unwrap_or(NO_CELL);
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

                if !ball.cold.hood.hood_dirty {
                    return (i, 0, f32::NAN, f32::NAN, false);
                }

                let ball_pos = *ball.get_position();
                let neighbour_indices = ball.cold.hood.get_neighbour_indices();
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
            if world.balls[i].cold.hood.hood_dirty {
                let ball_hood = &mut world.balls[i].cold.hood;
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
                let sites = physics.species().sites(ball.species());
                if sites.len() > 1 {
                    let walls = (bound_rect, obstacles.as_slice());
                    return (
                        molecule_backstop(physics, ball, sites, walls, dt, restitution),
                        0,
                    );
                }
                // Atoms meet the walls (behind soft walls: the backstop) at the
                // exact contact time.
                let start = *ball.get_position();
                let mut bounce_info = physics.update_positions(ball, dt, bound_rect, restitution);
                let mut checks = 0;
                // A ball that reached a box wall is not swept against line
                // walls this step: the contact normals must stay orthogonal.
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
    /// force kernel reads into `kernel_points`, in the frame of the cell
    /// grid (#392).
    fn update_old_positions(world: &mut SimulationWorld) {
        let frame = world.cache.cell_grid.kernel_frame();
        let kernel = &mut world.cache.kernel_points;
        kernel.cell = frame.cell;
        kernel.contacts.clear();
        kernel
            .contacts
            .extend(world.physics.species().iter().map(|s| s.contact_radius()));
        kernel
            .points
            .resize(world.balls.len(), KernelPoint::default());
        world
            .balls
            .par_iter_mut()
            .zip(kernel.points.par_iter_mut())
            .for_each(|(ball, point)| {
                ball.update_old_pos();
                *point = frame.point(ball);
            });
    }

    /// Pair forces of every ball, plus the soft wall forces of the box
    /// `bound` and `obstacles` (see `wall_forces`), into `results`
    /// (replaced, in ball order). One walk over each neighbour list: the
    /// SIMD kernel evaluates the pair potential and hands every pair to its
    /// interactions (`BallPhysics::interactions`), for the species that have
    /// any with a species present; the others take the kernel alone.
    fn compute_forces(
        world: &SimulationWorld,
        obstacles: &[Obstacle],
        bound: &Rect,
        results: &mut Vec<ForceResult>,
    ) {
        let physics = &world.physics;
        let balls = &world.balls;
        let points = &world.cache.kernel_points;
        let soft_walls = physics.soft_walls();
        let present = world.cache.species_present;
        let interacting: SpeciesSet = present
            .iter()
            .filter(|&a| {
                present
                    .iter()
                    .any(|b| !physics.interactions(a)[b.index()].is_empty())
            })
            .collect();
        (0..balls.len())
            .into_par_iter()
            .map(|i| {
                let ball = &balls[i];
                let mut result = if interacting.contains(ball.species()) {
                    let row = physics.interactions(ball.species());
                    let axis_i = axis(ball.theta);
                    force_simd(physics, i, balls, points, |out, j, species, d| {
                        for x in &row[species] {
                            if d.norm_sq() < x.cutoff() * x.cutoff() {
                                x.eval(-d, axis_i, axis(balls[j].theta), out);
                            }
                        }
                    })
                } else {
                    force_simd(physics, i, balls, points, |_, _, _, _| {})
                };
                if let Some(walls) = soft_walls {
                    soft_wall_force(walls, ball, bound, obstacles).add_to(&mut result);
                }
                result
            })
            .collect_into_vec(results);
    }

    /// Velocity-Verlet kick of one ball, with its wall contacts, added to
    /// `stats`. Returns the ball's squared speed and id for the maximum
    /// speed. Reads only this ball, so the balls can be updated in parallel.
    fn apply_single_force(
        ball: &mut Ball2D,
        result: &ForceResult,
        bounce_info: &BounceInfo,
        params: &ApplyParams,
        stats: &mut ApplyStats,
    ) -> (f32, usize) {
        let ApplyParams {
            gravity,
            fields,
            restitution,
            dt,
            speed_limit,
            wall_ramp_step,
        } = *params;

        let mut wall_px = 0.0;
        let mut wall_py = 0.0;

        let mut accel = ball.accel_with_speed_limit(&result.force, gravity, speed_limit, dt);
        if !fields.is_empty() {
            accel += fields_acceleration(fields, *ball.get_position(), ball.mass);
        }

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
        // Site impulses of a molecule on the walls.
        wall_px += bounce_info.wall_momentum.x;
        wall_py += bounce_info.wall_momentum.y;

        // Soft walls: the impulse their force gives the ball over the step
        // is taken from the walls (zero with hard walls).
        wall_px -= result.wall_force.x * dt;
        wall_py -= result.wall_force.y * dt;

        ball.accel = accel;
        if ball.inertia > 0.0 {
            ball.kick_rotation(result.torque, dt);
        }
        if ball.leaving_wall && wall_ramp_step > 0.0 {
            ball.cold.wall_ramp += wall_ramp_step;
            ball.leaving_wall = ball.cold.wall_ramp < 1.0;
        }

        if ball.verbose.console {
            log::debug!(
                target: crate::logging::VERBOSE_TARGET,
                "updated to  v {:?}  a {:?}",
                ball.v,
                ball.accel
            );
        }

        let speed_sq = ball.v.norm_sq();
        let momentum = ball.moment();
        // Per-ball terms in f32, their sums in f64 (#421).
        stats.pot += f64::from(result.potential_energy * 0.5);
        stats.pot += f64::from(result.wall_potential);
        stats.px += f64::from(momentum.x);
        stats.py += f64::from(momentum.y);
        stats.kin += ball.kinetic_energy();
        stats.mass += f64::from(ball.mass);
        stats.braked += u64::from(ball.braking);
        stats.capped_pairs += u64::from(result.capped_pairs);
        stats.wall_px += f64::from(wall_px);
        stats.wall_py += f64::from(wall_py);
        (speed_sq, ball.get_id())
    }

    /// The balls' potential energy in `field`, summed in ball order in f64.
    /// Gravity measures heights from the bottom wall (`UniformField::floor`):
    /// a world far from the origin would otherwise add `m·g·y` terms so large
    /// that f32 loses the kinetic and pair energy next to them. A ball with a
    /// non-finite position (NaN, removed later this step) is left out, so it
    /// cannot turn the reported energy into NaN.
    fn field_potential_energy(world: &SimulationWorld, field: &impl ExternalField) -> f64 {
        world
            .balls
            .iter()
            .filter(|ball| {
                let p = ball.get_position();
                p.x.is_finite() && p.y.is_finite()
            })
            .map(|ball| f64::from(field.potential(*ball.get_position(), ball.mass)))
            .sum::<f64>()
    }

    /// `factor` times the shorter world side (`speed_limit_factor` of
    /// `SimConfig`, 20% by default). World rects have a negative height
    /// (y points up), so the extents are taken as absolute values.
    pub(crate) fn default_speed_limit(bound_rect: &Rect, factor: f32) -> f32 {
        f32::min(bound_rect.w.abs(), bound_rect.h.abs()) * factor
    }

    /// Kick every ball (in parallel) and add up the step's stats and wall
    /// momentum. Each chunk of [`APPLY_CHUNK`] balls sums its balls in
    /// order, and the chunk sums are combined in chunk order, so the sums
    /// do not depend on the thread count or on how Rayon splits the work
    /// (#311).
    fn apply_forces_and_collect_stats(
        world: &mut SimulationWorld,
        force_results: &[ForceResult],
        bounce_infos: &[BounceInfo],
        chunk_stats: &mut Vec<ApplyStats>,
        dt: f32,
        bound_rect: &Rect,
        stats_accumulator: &mut StatsAccumulator,
    ) -> (f64, f64) {
        // The zips below stop at the shortest list, so a mismatch would
        // silently skip balls; the former indexed loop panicked instead.
        // Checked once per step, so kept in release builds too.
        assert_eq!(
            world.balls.len(),
            force_results.len(),
            "one force result per ball"
        );
        assert_eq!(
            world.balls.len(),
            bounce_infos.len(),
            "one bounce info per ball"
        );
        let params = ApplyParams {
            gravity: world.physics.constants.gravity,
            fields: world.physics.fields(),
            restitution: world.wall_restitution_fault(),
            dt,
            speed_limit: world
                .speed_limit_override
                .unwrap_or_else(|| Self::default_speed_limit(bound_rect, world.speed_limit_factor)),
            wall_ramp_step: if world.physics.walls().soft {
                dt / WALL_RAMP_TIME
            } else {
                0.0
            },
        };

        world
            .balls
            .par_chunks_mut(APPLY_CHUNK)
            .zip(force_results.par_chunks(APPLY_CHUNK))
            .zip(bounce_infos.par_chunks(APPLY_CHUNK))
            .map(|((balls, results), bounces)| {
                let mut stats = ApplyStats::default();
                for ((ball, result), bounce_info) in balls.iter_mut().zip(results).zip(bounces) {
                    let (speed_sq, id) =
                        Self::apply_single_force(ball, result, bounce_info, &params, &mut stats);
                    if stats.max_v * stats.max_v < speed_sq {
                        stats.max_v = f32::sqrt(speed_sq);
                        stats.max_id = id;
                    }
                }
                stats
            })
            .collect_into_vec(chunk_stats);

        let mut total_wall_px = 0.0;
        let mut total_wall_py = 0.0;
        for chunk in chunk_stats.iter() {
            stats_accumulator.pot += chunk.pot;
            stats_accumulator.momentum[0] += chunk.px;
            stats_accumulator.momentum[1] += chunk.py;
            stats_accumulator.kin += chunk.kin;
            stats_accumulator.sum_mass += chunk.mass;
            stats_accumulator.braked += chunk.braked;
            stats_accumulator.capped_pairs += chunk.capped_pairs;
            if stats_accumulator.max_v < chunk.max_v {
                stats_accumulator.max_v = chunk.max_v;
                stats_accumulator.max_id = chunk.max_id;
            }
            total_wall_px += chunk.wall_px;
            total_wall_py += chunk.wall_py;
        }

        (total_wall_px, total_wall_py)
    }
}

/// Balls per chunk of the apply pass. Fixed, so the sums do not depend on
/// the thread count; large enough that Rayon's overhead per chunk is small.
/// Changing it changes the summation order, and so the f32 results.
const APPLY_CHUNK: usize = 1024;

/// Step-wide inputs of the apply pass.
struct ApplyParams<'a> {
    gravity: f32,
    /// The `[[fields]]` (#347); gravity is `gravity`.
    fields: &'a [Field],
    restitution: f32,
    dt: f32,
    speed_limit: f32,
    /// How much `BallCold::wall_ramp` grows per step: `dt / WALL_RAMP_TIME`
    /// with soft walls, 0 with hard walls (the overlap pass ends it).
    wall_ramp_step: f32,
}

/// Stats of one chunk of the apply pass, summed in ball order, in f64
/// (#421): a few thousand f32 terms lose digits in an f32 sum.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ApplyStats {
    /// Half the pair potential energy (each pair is counted by both balls).
    pot: f64,
    kin: f64,
    /// Momentum of the balls.
    px: f64,
    py: f64,
    mass: f64,
    max_v: f32,
    max_id: usize,
    /// Momentum given to the walls the balls touched.
    wall_px: f64,
    wall_py: f64,
    /// Balls slowed by the speed limit (#422).
    braked: u64,
    /// Capped pair evaluations, each pair counted by both balls (#422).
    capped_pairs: u64,
}
