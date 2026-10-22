// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Builds the [`RendererSnapshot`] of a [`MainContent`]: what the renderer
//! and headless output read from the physics thread. Kept outside
//! `thermo_physics`, which has no UI types (#308).

use crate::application::MainContent;
use crate::application::core::SimulationWorld;
use crate::application::log_util::{SpeciesCounts, count_ball};
use crate::application::renderer_snapshot::{
    BallRenderData, OperatorSnapshot, RendererSnapshot, ThermoGridSpec, UNKNOWN_SPECIES_COLOUR,
    ValveSpan, WallSnapshot, get_special_ball_color,
};
use crate::application::scene_layers::SceneLayers;
use crate::application::sim_stats::NORMAL_GRAVITY;
use crate::application::thermo_field::ThermoGrid;
use crate::objects2d::SimThermostat;
use crate::objects2d::ball_impl::{BallPhysics, SpeciesId, SpeciesSet};
use crate::objects2d::wall_chain::Joint;
use std::sync::Arc;

/// Renderer snapshots of the simulation state.
pub trait CreateSnapshot {
    /// A snapshot with every layer (headless runs and tests).
    fn create_snapshot(&self) -> RendererSnapshot {
        self.create_snapshot_with(SceneLayers::ALL)
    }

    /// A snapshot with only `layers` (what the renderer asked for).
    fn create_snapshot_with(&self, layers: SceneLayers) -> RendererSnapshot;
}

impl CreateSnapshot for MainContent {
    /// Create renderer snapshot from current simulation state.
    /// Conditionally clones expensive data (neighbors, tail) only when needed for visualization.
    /// Uses cached force magnitudes from last physics step for neighbor line rendering.
    fn create_snapshot_with(&self, layers: SceneLayers) -> RendererSnapshot {
        let world = self.world();
        let step_config = self.step_config();
        // The species counts of the status lines come from the same pass
        // over the balls when the BALLS layer makes one.
        let mut species_counts = [0; SpeciesSet::CAPACITY];
        let (balls, counts) = if layers.contains(SceneLayers::BALLS) {
            let balls = build_ball_render_data(world, &mut species_counts);
            (balls, Some(&species_counts))
        } else {
            (Vec::new(), None)
        };

        RendererSnapshot {
            balls,
            simulation_step: world.step_count as u64,
            world_generation: self.runtime().world_generation(),
            layers,
            kinetic_energy: world.sim.get_kinetic_energy(),
            potential_energy: world.sim.get_potential_energy(),
            time: world.sim.get_time(),
            // FPS not available during snapshot creation
            status_text: self.status_text_with_counts(0, step_config.get_time_step(), counts),
            cell_grid_bounds: world.cell_grid.world_rect(),
            cursor_species: step_config.get_ball_factory().new_ball_species(),
            species: Arc::clone(world.species()),
            cursor_ring_radii: ring_radii(
                &world.physics,
                step_config.get_ball_factory().new_ball_species(),
            ),
            gravity_force_multiple: current_gravity_force_multiple(world.physics.constants.gravity),
            created_at: std::time::Instant::now(),
            // Create lightweight grid spec instead of cloning the full CellGrid
            // This avoids cloning the cell vector while providing all info needed for rendering
            grid_spec: {
                let (rows, cols) = world.cell_grid.get_grid_size();
                let (cell_width, cell_height) = world.cell_grid.get_cell_size();
                let world_rect = world.cell_grid.world_rect();
                crate::application::renderer_snapshot::CellGridSpec {
                    rows,
                    cols,
                    cell_width,
                    cell_height,
                    origin_x: world_rect.x,
                    origin_y: world_rect.y,
                }
            },
            physics_fps: self.runtime().physics_fps(),
            operators: OperatorSnapshot {
                line_segments: world.line_segments.segments().to_vec(),
                walls: world
                    .wall_chains
                    .chains()
                    .map(|c| WallSnapshot {
                        chain_id: c.id,
                        start: c.start,
                        end: c.end,
                        piece_ids: c.pieces.iter().map(|p| p.line_id).collect(),
                        valve_ids: c.valve_ids().collect(),
                    })
                    .collect(),
                valve_spans: valve_spans(world),
                source_spheres: world.source_spheres.spheres().to_vec(),
                thermostats: if layers.contains(SceneLayers::READOUTS) {
                    thermostat_snapshot(world)
                } else {
                    world.thermostats.thermostats().to_vec()
                },
            },
            ..if layers.contains(SceneLayers::THERMO) {
                let (cell_temperature, cell_pressure, thermo_grid_spec) =
                    build_thermo_snapshot_fields(world);
                RendererSnapshot {
                    cell_temperature,
                    cell_pressure,
                    thermo_grid_spec,
                    ..RendererSnapshot::new()
                }
            } else {
                RendererSnapshot::new()
            }
        }
    }
}

/// Per-ball render data (colours, tails, neighbour lines): the `BALLS`
/// layer, which exists only for drawing.
/// Also counts the balls per species into `species_counts`.
fn build_ball_render_data(
    world: &SimulationWorld,
    species_counts: &mut SpeciesCounts,
) -> Vec<BallRenderData> {
    let ball_count = world.balls.len();
    let mut balls = Vec::with_capacity(ball_count);
    let species_table = &**world.species();
    let physics = &world.physics;
    // Ring radii per species: half the like pair's equilibrium distance
    // and cutoff.
    let rings: Vec<[f32; 2]> = species_table
        .ids()
        .map(|s| ring_radii(physics, s))
        .collect();

    // Create BallRenderData for each ball
    for (ball_idx, ball) in world.balls.iter().enumerate() {
        count_ball(species_counts, ball);
        let species = species_table.get(ball.species());
        let [equilibrium_radius, cutoff_radius] = rings
            .get(ball.species().index())
            .copied()
            .unwrap_or_default();
        // Special colors (verbose/braking) override the species colour
        let color = get_special_ball_color(
            ball.verbose.console || ball.verbose.lines(),
            ball.is_braking(),
        )
        .or(species.map(|s| s.colour))
        .unwrap_or(UNKNOWN_SPECIES_COLOUR);

        let show_neighbor_lines = ball.verbose.lines();
        let show_main_neighbors_only = ball.verbose.is_main();

        // Only clone expensive data if it will actually be used
        let neighbors: Vec<(usize, f32)> =
            if show_neighbor_lines && ball_idx < world.cached_force_results.len() {
                // Use cached force magnitudes from last physics step only if needed
                world.cached_force_results[ball_idx].neighbor_forces.clone()
            } else {
                // Skip cloning if neighbor lines won't be drawn
                Vec::new()
            };

        // Only verbose balls keep a tail
        let tail = ball
            .get_tail_positions()
            .map_or_else(Vec::new, |tail| tail.iter().copied().collect());

        balls.push(BallRenderData {
            id: ball.get_id(),
            species: ball.species(),
            position: *ball.get_position(),
            velocity: *ball.get_velocity(),
            mass: ball.get_mass(),
            color,
            contact_radius: ball.get_contact_radius(),
            display_radius: species.map_or(ball.get_contact_radius(), |s| s.display_radius),
            equilibrium_radius,
            cutoff_radius,
            show_neighbor_lines,
            show_main_neighbors_only,
            neighbors,
            update_counter: ball.get_update_counter(),
            tail,
        });
    }
    balls
}

/// The thermostats with a fresh reading each, for their labels (the
/// `READOUTS` layer). The reading is display data: it is measured here,
/// read-only, for active and inactive thermostats alike, so a paused
/// world or a deactivated thermostat still shows live values.
fn thermostat_snapshot(world: &SimulationWorld) -> Vec<SimThermostat> {
    let mut thermostats = world.thermostats.thermostats().to_vec();
    for (thermostat, reading) in thermostats.iter_mut().zip(world.thermostat_readings()) {
        thermostat.current_local_temp = reading;
    }
    thermostats
}

/// Build the thermo-related fields for the snapshot from the rolling-window history.
///
/// Returns `(temperature_grid, pressure_grid, grid_spec)` without constructing
/// an intermediate `RendererSnapshot`.
fn build_thermo_snapshot_fields(
    world: &SimulationWorld,
) -> (ThermoGrid, ThermoGrid, Option<ThermoGridSpec>) {
    let region = world.world_region();
    let ww = region.w.abs();
    let wh = region.h.abs();

    match world.thermo_history.average() {
        Some((avg_t, avg_p)) => {
            let (rows, cols) = avg_t.shape();
            let cell_width = ww / cols as f32;
            let cell_height = wh / rows as f32;

            // Use the same top-left derivation as compute_cell_thermo
            // so the spec origin matches the binning origin.
            let spec = ThermoGridSpec {
                rows,
                cols,
                cell_width,
                cell_height,
                origin_x: region.x.min(region.x + region.w),
                origin_y: region.y.min(region.y + region.h),
            };

            (avg_t, avg_p, Some(spec))
        }
        None => (
            ThermoGrid::filled(0, 0, 0.0),
            ThermoGrid::filled(0, 0, 0.0),
            None,
        ),
    }
}

/// Resolved world-space geometry of every valve, in chain order.
///
/// Computed once per snapshot so renderer and hit-testing never repeat
/// the chain math.
pub fn valve_spans(world: &SimulationWorld) -> Vec<ValveSpan> {
    let mut spans = Vec::new();
    for chain in world.wall_chains.chains() {
        for (index, joint) in chain.joints.iter().enumerate() {
            let Joint::Valve {
                valve_id,
                t_start,
                t_end,
            } = joint
            else {
                continue;
            };
            let Some(valve) = world.valve_segments.find(*valve_id) else {
                continue;
            };
            spans.push(ValveSpan {
                valve_id: *valve_id,
                p0: chain.point_at(*t_start),
                p1: chain.point_at(*t_end),
                thickness: world.joint_thickness(chain, index),
                is_closed: valve.is_closed,
            });
        }
    }
    spans
}

fn current_gravity_force_multiple(gravity: f32) -> u8 {
    (gravity / NORMAL_GRAVITY).round() as u8
}

/// Equilibrium and cutoff ring radii of `species`: half the like pair's
/// equilibrium distance and cutoff, so the rings of two balls touch there.
fn ring_radii(physics: &BallPhysics, species: SpeciesId) -> [f32; 2] {
    [
        0.5 * physics.equilibrium_distance(species, species),
        0.5 * physics.cutoff(species, species),
    ]
}
