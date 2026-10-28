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
    BallRenderData, CellGridSpec, DebugSnapshot, FrameInfo, NeighborLines, OperatorSnapshot,
    RendererSnapshot, ThermoGridSpec, UNKNOWN_SPECIES_COLOUR, ValveSpan, WallSnapshot,
    get_special_ball_color,
};
use crate::application::scene_layers::SceneLayers;
use crate::application::sim_stats::NORMAL_GRAVITY;
use crate::application::thermo_field::CellFields;
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
    fn create_snapshot_with(&self, layers: SceneLayers) -> RendererSnapshot {
        let mut snapshot = RendererSnapshot::new();
        self.fill_snapshot(layers, &mut snapshot);
        snapshot
    }

    /// Overwrite `snapshot` with the current state and only `layers`,
    /// reusing its allocations (#341): an earlier snapshot refilled this way
    /// allocates no new ball vector or top-level operator vectors (the
    /// per-wall id lists and verbose balls' neighbour lines are rebuilt).
    fn fill_snapshot(&self, layers: SceneLayers, snapshot: &mut RendererSnapshot);
}

impl CreateSnapshot for MainContent {
    /// Fill the snapshot from the current simulation state.
    /// Debug data (neighbour lines) is built only for verbose balls, from
    /// the force magnitudes cached by the last physics step.
    fn fill_snapshot(&self, layers: SceneLayers, snapshot: &mut RendererSnapshot) {
        let world = self.world();
        let step_config = self.step_config();
        // The species counts of the status lines and the metrics store come
        // from the same pass over the balls when the BALLS layer makes one.
        let mut species_counts = [0; SpeciesSet::CAPACITY];
        snapshot.balls.data.clear();
        let mut debug = snapshot.debug.take().unwrap_or_default();
        debug.neighbor_lines.clear();
        if layers.contains(SceneLayers::BALLS) {
            fill_ball_render_data(world, &mut species_counts, snapshot, &mut debug);
        } else {
            for ball in world.balls() {
                count_ball(&mut species_counts, ball);
            }
        }
        snapshot.debug = (!debug.neighbor_lines.is_empty()).then_some(debug);
        let (wall_px, wall_py) = self.wall_momentum();
        let new_ball_species = step_config.get_ball_factory().new_ball_species();

        snapshot.frame = FrameInfo {
            simulation_step: world.step_count() as u64,
            world_generation: self.runtime().world_generation(),
            layers,
            time: world.sim().get_time(),
            kinetic_energy: world.sim().get_kinetic_energy(),
            potential_energy: world.sim().get_potential_energy(),
            wall_momentum: wall_px.hypot(wall_py),
            species_counts,
            gravity_force_multiple: current_gravity_force_multiple(
                world.physics().constants.gravity,
            ),
            physics_fps: self.runtime().physics_fps(),
            world_rect: world.cell_grid().world_rect(),
            status: self.status_numbers(step_config.get_time_step()),
            created_at: std::time::Instant::now(),
        };

        let balls = &mut snapshot.balls;
        if !Arc::ptr_eq(&balls.species, world.species()) {
            balls.species = Arc::clone(world.species());
        }
        balls.cursor_species = new_ball_species;
        balls.cursor_ring_radii = ring_radii(world.physics(), new_ball_species);

        fill_operator_snapshot(world, layers, &mut snapshot.operators);

        let fields = &mut snapshot.fields;
        // A lightweight grid spec instead of a clone of the full CellGrid.
        let (rows, cols) = world.cell_grid().get_grid_size();
        let (cell_width, cell_height) = world.cell_grid().get_cell_size();
        let world_rect = world.cell_grid().world_rect();
        fields.cell_grid = CellGridSpec {
            rows,
            cols,
            cell_width,
            cell_height,
            origin_x: world_rect.x,
            origin_y: world_rect.y,
        };
        (fields.cells, fields.thermo_grid) = if layers.contains(SceneLayers::THERMO) {
            build_thermo_snapshot_fields(world)
        } else {
            (CellFields::empty(), None)
        };
    }
}

/// Per-ball render data (colours, rings) into `snapshot.balls` and the
/// neighbour lines of verbose balls into `debug`: the `BALLS` layer, which
/// exists only for drawing. Also counts the balls per species into
/// `species_counts`.
fn fill_ball_render_data(
    world: &SimulationWorld,
    species_counts: &mut SpeciesCounts,
    snapshot: &mut RendererSnapshot,
    debug: &mut DebugSnapshot,
) {
    let balls = &mut snapshot.balls.data;
    balls.reserve(world.balls().len());
    let species_table = &**world.species();
    let physics = world.physics();
    // Ring radii per species: half the like pair's equilibrium distance
    // and cutoff.
    let mut rings = [[0.0; 2]; SpeciesSet::CAPACITY];
    for (ring, species) in rings.iter_mut().zip(species_table.ids()) {
        *ring = ring_radii(physics, species);
    }
    let force_results = world.cached_force_results();

    for (ball_idx, ball) in world.balls().iter().enumerate() {
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

        // Debug data only for verbose balls that show their neighbours.
        if ball.verbose.lines()
            && let Some(result) = force_results.get(ball_idx)
        {
            debug.neighbor_lines.push(NeighborLines {
                ball: ball_idx,
                main_only: ball.verbose.is_main(),
                neighbors: result.neighbor_forces.clone(),
            });
        }

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
        });
    }
}

/// The operators into `operators`, reusing its vectors.
fn fill_operator_snapshot(
    world: &SimulationWorld,
    layers: SceneLayers,
    operators: &mut OperatorSnapshot,
) {
    operators.line_segments.clear();
    operators
        .line_segments
        .extend_from_slice(world.line_segments().segments());
    operators.walls.clear();
    operators
        .walls
        .extend(world.wall_chains().chains().map(|c| WallSnapshot {
            chain_id: c.id,
            start: c.start,
            end: c.end,
            piece_ids: c.pieces.iter().map(|p| p.line_id).collect(),
            valve_ids: c.valve_ids().collect(),
        }));
    fill_valve_spans(world, &mut operators.valve_spans);
    operators.source_spheres.clear();
    operators
        .source_spheres
        .extend_from_slice(world.source_spheres().spheres());
    operators.thermostats.clear();
    operators
        .thermostats
        .extend_from_slice(world.thermostats().thermostats());
    if layers.contains(SceneLayers::READOUTS) {
        add_thermostat_readings(world, &mut operators.thermostats);
    }
}

/// A fresh reading for each thermostat, for their labels (the `READOUTS`
/// layer). The reading is display data: it is measured here, read-only,
/// for active and inactive thermostats alike, so a paused world or a
/// deactivated thermostat still shows live values.
fn add_thermostat_readings(world: &SimulationWorld, thermostats: &mut [SimThermostat]) {
    for (thermostat, reading) in thermostats.iter_mut().zip(world.thermostat_readings()) {
        thermostat.current_local_temp = reading;
    }
}

/// The averaged per-cell fields and their grid layout, from the
/// rolling-window history; empty fields and no layout before the first
/// sample.
fn build_thermo_snapshot_fields(world: &SimulationWorld) -> (CellFields, Option<ThermoGridSpec>) {
    let Some(cells) = world.thermo_history().average() else {
        return (CellFields::empty(), None);
    };
    let region = world.world_region();
    let (rows, cols) = cells.shape();
    // The same top-left derivation as `compute_cell_fields`, so the spec
    // origin matches the binning origin.
    let spec = ThermoGridSpec {
        rows,
        cols,
        cell_width: region.w.abs() / cols as f32,
        cell_height: region.h.abs() / rows as f32,
        origin_x: region.x.min(region.x + region.w),
        origin_y: region.y.min(region.y + region.h),
    };
    (cells, Some(spec))
}

/// Resolved world-space geometry of every valve, in chain order.
///
/// Computed once per snapshot so renderer and hit-testing never repeat
/// the chain math. Replaces the contents of `spans`.
fn fill_valve_spans(world: &SimulationWorld, spans: &mut Vec<ValveSpan>) {
    spans.clear();
    for chain in world.wall_chains().chains() {
        for (index, joint) in chain.joints.iter().enumerate() {
            let Joint::Valve {
                valve_id,
                t_start,
                t_end,
            } = joint
            else {
                continue;
            };
            let Some(valve) = world.valve_segments().find(*valve_id) else {
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
