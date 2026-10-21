// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::application::log_util::{SpeciesCounts, count_ball};
use crate::application::renderer_snapshot::{
    BallRenderData, OperatorSnapshot, RendererSnapshot, ThermoGridSpec, UNKNOWN_SPECIES_COLOUR,
    WallSnapshot, get_special_ball_color,
};
use crate::application::scene_layers::SceneLayers;
use crate::application::sim_stats::NORMAL_GRAVITY;
use crate::application::thermo_field::ThermoGrid;
use crate::objects2d::SimThermostat;
use crate::objects2d::ball_impl::{BallPhysics, SpeciesId, SpeciesSet};
use std::sync::Arc;

impl MainContent {
    /// Create renderer snapshot from current simulation state.
    /// Conditionally clones expensive data (neighbors, tail) only when needed for visualization.
    /// Uses cached force magnitudes from last physics step for neighbor line rendering.
    pub fn create_snapshot(&self) -> RendererSnapshot {
        let layers = self.runtime.snapshot_layers();
        // The species counts of the status lines come from the same pass
        // over the balls when the BALLS layer makes one.
        let mut species_counts = [0; SpeciesSet::CAPACITY];
        let (balls, counts) = if layers.contains(SceneLayers::BALLS) {
            let balls = self.build_ball_render_data(&mut species_counts);
            (balls, Some(&species_counts))
        } else {
            (Vec::new(), None)
        };

        RendererSnapshot {
            balls,
            simulation_step: self.world.step_count as u64,
            world_generation: self.runtime.world_generation(),
            layers,
            kinetic_energy: self.world.sim.get_kinetic_energy(),
            potential_energy: self.world.sim.get_potential_energy(),
            time: self.world.sim.get_time(),
            // FPS not available during snapshot creation
            status_text: self.status_text_with_counts(0, self.step_config.get_time_step(), counts),
            cell_grid_bounds: self.world.cell_grid.world_rect(),
            cursor_species: self.step_config.get_ball_factory().new_ball_species(),
            species: Arc::clone(self.world.species()),
            cursor_ring_radii: ring_radii(
                &self.world.physics,
                self.step_config.get_ball_factory().new_ball_species(),
            ),
            gravity_force_multiple: current_gravity_force_multiple(
                self.world.physics.constants.gravity,
            ),
            created_at: std::time::Instant::now(),
            // Create lightweight grid spec instead of cloning the full CellGrid
            // This avoids cloning the cell vector while providing all info needed for rendering
            grid_spec: {
                let (rows, cols) = self.world.cell_grid.get_grid_size();
                let (cell_width, cell_height) = self.world.cell_grid.get_cell_size();
                let world_rect = self.world.cell_grid.world_rect();
                crate::application::renderer_snapshot::CellGridSpec {
                    rows,
                    cols,
                    cell_width,
                    cell_height,
                    origin_x: world_rect.x,
                    origin_y: world_rect.y,
                }
            },
            physics_fps: self.runtime.physics_fps(),
            operators: OperatorSnapshot {
                line_segments: self.world.line_segments.segments().to_vec(),
                walls: self
                    .world
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
                valve_spans: self.world.valve_spans(),
                source_spheres: self.world.source_spheres.spheres().to_vec(),
                thermostats: if layers.contains(SceneLayers::READOUTS) {
                    self.thermostat_snapshot()
                } else {
                    self.world.thermostats.thermostats().to_vec()
                },
            },
            ..if layers.contains(SceneLayers::THERMO) {
                let (cell_temperature, cell_pressure, thermo_grid_spec) =
                    self.build_thermo_snapshot_fields();
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

    /// Per-ball render data (colours, tails, neighbour lines): the `BALLS`
    /// layer, which exists only for drawing.
    /// Also counts the balls per species into `species_counts`.
    fn build_ball_render_data(&self, species_counts: &mut SpeciesCounts) -> Vec<BallRenderData> {
        let ball_count = self.world.balls.len();
        let mut balls = Vec::with_capacity(ball_count);
        let species_table = &**self.world.species();
        let physics = &self.world.physics;
        // Ring radii per species: half the like pair's equilibrium distance
        // and cutoff.
        let rings: Vec<[f32; 2]> = species_table
            .ids()
            .map(|s| ring_radii(physics, s))
            .collect();

        // Create BallRenderData for each ball
        for (ball_idx, ball) in self.world.balls.iter().enumerate() {
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
                if show_neighbor_lines && ball_idx < self.world.cached_force_results.len() {
                    // Use cached force magnitudes from last physics step only if needed
                    self.world.cached_force_results[ball_idx]
                        .neighbor_forces
                        .clone()
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
    fn thermostat_snapshot(&self) -> Vec<SimThermostat> {
        let mut thermostats = self.world.thermostats.thermostats().to_vec();
        for (thermostat, reading) in thermostats.iter_mut().zip(self.world.thermostat_readings()) {
            thermostat.current_local_temp = reading;
        }
        thermostats
    }

    /// Build the thermo-related fields for the snapshot from the rolling-window history.
    ///
    /// Returns `(temperature_grid, pressure_grid, grid_spec)` without constructing
    /// an intermediate `RendererSnapshot`.
    fn build_thermo_snapshot_fields(&self) -> (ThermoGrid, ThermoGrid, Option<ThermoGridSpec>) {
        let world = self.world.world_region();
        let ww = world.w.abs();
        let wh = world.h.abs();

        match self.world.thermo_history.average() {
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
                    origin_x: world.x.min(world.x + world.w),
                    origin_y: world.y.min(world.y + world.h),
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

    /// Publish current snapshot to shared slot using atomic Arc swap.
    /// Lock-free publication allows physics thread to continue without waiting for renderer.
    pub fn publish_snapshot(&self) {
        let snapshot = self.create_snapshot();
        self.runtime.publish_snapshot(snapshot);
    }

    /// Get clone of Arc-wrapped snapshot slot for sharing with renderer thread.
    /// Renderer can load snapshots without blocking physics thread.
    pub fn get_snapshot_slot(&self) -> Arc<arc_swap::ArcSwapOption<RendererSnapshot>> {
        self.runtime.snapshot_slot()
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
