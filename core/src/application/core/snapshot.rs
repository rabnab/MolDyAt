use crate::application::LogStuff;
use crate::application::MainContent;
use crate::application::renderer_snapshot::{
    BallRenderData, OperatorSnapshot, RendererSnapshot, ThermoGridSpec, get_special_ball_color,
    size_class_to_rgba,
};
use crate::application::sim_stats::NORMAL_GRAVITY;
use crate::application::thermo_field::ThermoGrid;
use std::sync::Arc;

impl MainContent {
    /// Create renderer snapshot from current simulation state.
    /// Conditionally clones expensive data (neighbors, tail) only when needed for visualization.
    /// Uses cached force magnitudes from last physics step for neighbor line rendering.
    pub fn create_snapshot(&self) -> RendererSnapshot {
        let ball_count = self.world.balls.len();
        let mut balls = Vec::with_capacity(ball_count);

        // Create BallRenderData for each ball
        for (ball_idx, ball) in self.world.balls.iter().enumerate() {
            // Determine color: special colors (verbose/braking) override size class color
            let color = get_special_ball_color(
                ball.verbose.console || ball.verbose.lines(),
                ball.is_braking(),
            )
            .unwrap_or_else(|| size_class_to_rgba(ball.get_size_idx()));

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

            // Only clone tail positions if they'll be used
            let tail = if ball.get_tail_positions().is_some() {
                ball.get_tail_positions()
                    .map_or(Vec::new(), |tail| tail.iter().cloned().collect())
            } else {
                Vec::new()
            };

            balls.push(BallRenderData {
                id: ball.get_id(),
                size_idx: ball.get_size_idx(),
                position: *ball.get_position(),
                velocity: *ball.get_velocity(),
                mass: ball.get_mass(),
                color,
                radius: ball.get_radius(),
                show_neighbor_lines,
                show_main_neighbors_only,
                neighbors,
                update_counter: ball.get_update_counter(),
                tail,
            });
        }

        RendererSnapshot {
            balls,
            simulation_step: self.world.step_count as u64,
            kinetic_energy: self.world.sim.get_kinetic_energy(),
            potential_energy: self.world.sim.get_potential_energy(),
            time: self.world.sim.get_time(),
            status_text: self.status_text(0, self.game_data.get_time_step()), // FPS not available during snapshot creation
            cell_grid_bounds: self.world.cell_grid.world_rect(),
            cursor_ball_size_idx: self.game_data.get_ball_factory().get_new_ball_size_idx(),
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
            equilibrium_edge_distance: self.world.physics.equilibrium_edge_distance(),
            cutoff_distance: self.world.physics.get_cutoff_distance(),
            operators: OperatorSnapshot {
                line_segments: self.world.line_segments.segments().to_vec(),
                valve_segments: self.world.valve_segments.valves().to_vec(),
                source_spheres: self.world.source_spheres.spheres().to_vec(),
                thermostats: self.world.thermostats.thermostats().to_vec(),
            },
            ..{
                let (cell_temperature, cell_pressure, thermo_grid_spec) =
                    self.build_thermo_snapshot_fields();
                RendererSnapshot {
                    cell_temperature,
                    cell_pressure,
                    thermo_grid_spec,
                    ..RendererSnapshot::new()
                }
            }
        }
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
    (gravity / NORMAL_GRAVITY) as u8
}
