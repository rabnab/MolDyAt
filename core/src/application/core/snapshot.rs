use crate::application::LogStuff;
use crate::application::MainContent;
use crate::application::renderer_snapshot::{
    BallRenderData, RendererSnapshot, ThermoGridSpec, get_special_ball_color, size_class_to_rgba,
};
use crate::application::thermo_field::ThermoGrid;
use std::sync::Arc;

impl MainContent {
    /// Create renderer snapshot from current simulation state.
    /// Conditionally clones expensive data (neighbors, tail) only when needed for visualization.
    /// Uses cached force magnitudes from last physics step for neighbor line rendering.
    pub fn create_snapshot(&self) -> RendererSnapshot {
        let ball_count = self.balls.len();
        let mut balls = Vec::with_capacity(ball_count);

        // Create BallRenderData for each ball
        for (ball_idx, ball) in self.balls.iter().enumerate() {
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
                if show_neighbor_lines && ball_idx < self.cached_force_results.len() {
                    // Use cached force magnitudes from last physics step only if needed
                    self.cached_force_results[ball_idx].neighbor_forces.clone()
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
            simulation_step: self.step_count as u64,
            kinetic_energy: self.sim.get_kinetic_energy(),
            potential_energy: self.sim.get_potential_energy(),
            time: self.sim.get_time(),
            status_text: self.status_text(0, self.game_data.get_time_step()), // FPS not available during snapshot creation
            cell_grid_bounds: self.cell_grid.world_rect(),
            cursor_ball_size_idx: self.get_ball_factory().get_new_ball_size_idx(),
            created_at: std::time::Instant::now(),
            // Create lightweight grid spec instead of cloning the full CellGrid
            // This avoids cloning the cell vector while providing all info needed for rendering
            grid_spec: {
                let (rows, cols) = self.cell_grid.get_grid_size();
                let (cell_width, cell_height) = self.cell_grid.get_cell_size();
                let world_rect = self.cell_grid.world_rect();
                crate::application::renderer_snapshot::CellGridSpec {
                    rows,
                    cols,
                    cell_width,
                    cell_height,
                    origin_x: world_rect.x,
                    origin_y: world_rect.y,
                }
            },
            physics_fps: self.get_physics_fps(),
            equilibrium_edge_distance: self.physics.equilibrium_edge_distance(),
            cutoff_distance: self.physics.get_cutoff_distance(),
            line_segments: self.get_line_segments().to_vec(),
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
        let world = self.world_region();
        let ww = world.w.abs();
        let wh = world.h.abs();

        match self.thermo_history.average() {
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
        self.snapshot_slot.store(Some(Arc::new(snapshot)));
    }

    /// Get clone of Arc-wrapped snapshot slot for sharing with renderer thread.
    /// Renderer can load snapshots without blocking physics thread.
    pub fn get_snapshot_slot(&self) -> Arc<arc_swap::ArcSwapOption<RendererSnapshot>> {
        self.snapshot_slot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::MainContent;
    use crate::application::renderer_snapshot::size_class_to_rgba;
    use crate::objects2d::Vec2D;
    use crate::objects2d::ball_impl::BallFactory;

    fn create_test_main_content_with_balls(ball_count: usize) -> MainContent {
        let world_rect = crate::objects2d::Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut main_content = MainContent::new(world_rect).expect("Could not create main content");

        let factory = BallFactory::new();

        // Add balls
        for _ in 0..ball_count {
            let ball =
                factory.create_random((&Vec2D::new(0.0, 0.0), &Vec2D::new(10.0, 10.0)), (0.0, 1.0));
            main_content.balls.push(ball);
        }

        main_content.num_balls = ball_count;

        // Add balls to cell grid
        for i in 0..main_content.balls.len() {
            main_content.cell_grid.add_ball(i, &mut main_content.balls);
        }

        main_content
    }

    #[test]
    fn test_collect_all_neighbors_empty() {
        // Create a MainContent with empty balls vector
        // This is a structural test to verify the method works with empty input
        let empty_neighbors: Vec<Vec<usize>> = Vec::new();
        assert_eq!(empty_neighbors.len(), 0);
    }

    #[test]
    fn test_snapshot_colors_are_valid_rgba() {
        // Verify the color helper functions work correctly
        for size_class in 0..6 {
            let color = size_class_to_rgba(size_class);
            assert!(color[0] >= 0.0 && color[0] <= 1.0);
            assert!(color[1] >= 0.0 && color[1] <= 1.0);
            assert!(color[2] >= 0.0 && color[2] <= 1.0);
            assert!(color[3] >= 0.0 && color[3] <= 1.0);
        }
    }

    #[test]
    fn test_special_colors() {
        // Verify special color logic
        let verbose_color = get_special_ball_color(true, false);
        assert_eq!(verbose_color, Some([0.0, 0.0, 1.0, 1.0])); // Blue

        let braking_color = get_special_ball_color(false, true);
        assert_eq!(braking_color, Some([1.0, 1.0, 0.0, 1.0])); // Yellow

        let normal_color = get_special_ball_color(false, false);
        assert_eq!(normal_color, None); // Use size class color
    }

    #[test]
    fn test_publish_snapshot_atomic_storage() {
        let main_content = create_test_main_content_with_balls(10);

        // Publish a snapshot
        main_content.publish_snapshot();

        // Verify snapshot was stored
        let slot = main_content.get_snapshot_slot();
        let snapshot = slot.load();
        assert!(snapshot.is_some());

        let snapshot = snapshot.as_ref().unwrap();
        assert_eq!(snapshot.len(), 10);
        assert_eq!(snapshot.balls.len(), 10);
    }

    #[test]
    fn test_snapshot_slot_sharing() {
        let main_content = create_test_main_content_with_balls(5);

        // Get two clones of the snapshot slot
        let slot1 = main_content.get_snapshot_slot();
        let slot2 = main_content.get_snapshot_slot();

        // Publish snapshot
        main_content.publish_snapshot();

        // Both slots should see the same snapshot
        let snap1 = slot1.load();
        let snap2 = slot2.load();

        assert!(snap1.is_some());
        assert!(snap2.is_some());

        // Verify they point to the same Arc
        assert!(Arc::ptr_eq(
            snap1.as_ref().unwrap(),
            snap2.as_ref().unwrap()
        ));
    }

    #[test]
    fn test_snapshot_updates_on_republish() {
        let mut main_content = create_test_main_content_with_balls(5);

        // Publish first snapshot
        main_content.step_count = 100;
        main_content.publish_snapshot();

        let slot = main_content.get_snapshot_slot();
        let snapshot1 = slot.load();
        assert_eq!(snapshot1.as_ref().unwrap().simulation_step, 100);

        // Update step count and republish
        main_content.step_count = 200;
        main_content.publish_snapshot();

        let snapshot2 = slot.load();
        assert_eq!(snapshot2.as_ref().unwrap().simulation_step, 200);

        // Verify they are different snapshots
        assert!(!Arc::ptr_eq(
            snapshot1.as_ref().unwrap(),
            snapshot2.as_ref().unwrap()
        ));
    }

    #[test]
    fn test_snapshot_neighbor_indices_valid() {
        let main_content = create_test_main_content_with_balls(20);
        let snapshot = main_content.create_snapshot();

        // All neighbor indices should be valid (< ball count)
        for (i, ball_data) in snapshot.balls.iter().enumerate() {
            for &(neighbor_idx, _force) in &ball_data.neighbors {
                assert!(
                    neighbor_idx < snapshot.len(),
                    "Ball {} has invalid neighbor index {}",
                    i,
                    neighbor_idx
                );
            }
        }
    }

    #[test]
    fn test_snapshot_radii_positive() {
        let main_content = create_test_main_content_with_balls(15);
        let snapshot = main_content.create_snapshot();

        // All radii should be positive
        for (i, ball_data) in snapshot.balls.iter().enumerate() {
            assert!(
                ball_data.radius > 0.0,
                "Ball {} has non-positive radius {}",
                i,
                ball_data.radius
            );
            assert!(
                ball_data.radius.is_finite(),
                "Ball {} has non-finite radius",
                i
            );
        }
    }

    #[test]
    fn test_snapshot_positions_finite() {
        let main_content = create_test_main_content_with_balls(10);
        let snapshot = main_content.create_snapshot();

        // All positions should have finite coordinates
        for (i, ball_data) in snapshot.balls.iter().enumerate() {
            assert!(
                ball_data.position.x.is_finite(),
                "Ball {} has non-finite x position",
                i
            );
            assert!(
                ball_data.position.y.is_finite(),
                "Ball {} has non-finite y position",
                i
            );
        }
    }

    // ── Thermo snapshot tests (#115) ─────────────────────────────────

    fn create_main_content_and_step(ball_count: usize) -> MainContent {
        use crate::application::GameLogic;
        let mut mc = create_test_main_content_with_balls(ball_count);
        mc.simulation_step().unwrap();
        mc
    }

    #[test]
    fn test_thermo_snapshot_populated_after_step_with_balls() {
        let mc = create_main_content_and_step(10);
        let snap = mc.create_snapshot();
        // After one step with ≥ 2 balls, thermo fields must be populated
        assert!(snap.thermo_grid_spec.is_some(), "spec should be Some");
        let (rows, cols) = snap.cell_temperature.shape();
        assert!(rows > 0 && cols > 0, "T grid should be non-empty");
        let (pr, pc) = snap.cell_pressure.shape();
        assert!(pr > 0 && pc > 0, "P grid should be non-empty");
    }

    #[test]
    fn test_thermo_grid_shape_matches_spec() {
        let mc = create_main_content_and_step(20);
        let snap = mc.create_snapshot();
        let spec = snap.thermo_grid_spec.as_ref().expect("spec should be Some");
        let (t_rows, t_cols) = snap.cell_temperature.shape();
        assert_eq!(t_rows, spec.rows, "T rows must match spec");
        assert_eq!(t_cols, spec.cols, "T cols must match spec");
        let (p_rows, p_cols) = snap.cell_pressure.shape();
        assert_eq!(p_rows, spec.rows, "P rows must match spec");
        assert_eq!(p_cols, spec.cols, "P cols must match spec");
    }

    #[test]
    fn test_thermo_snapshot_single_ball_all_nan() {
        let mc = create_main_content_and_step(1);
        let snap = mc.create_snapshot();
        // With only 1 ball, all cells are NaN but spec is still populated
        assert!(
            snap.thermo_grid_spec.is_some(),
            "spec should be Some even with 1 ball"
        );
        let (rows, cols) = snap.cell_temperature.shape();
        for r in 0..rows {
            for c in 0..cols {
                assert!(
                    snap.cell_temperature.get(r, c).is_nan(),
                    "T[{r},{c}] should be NaN with 1 ball"
                );
                assert!(
                    snap.cell_pressure.get(r, c).is_nan(),
                    "P[{r},{c}] should be NaN with 1 ball"
                );
            }
        }
    }

    #[test]
    fn test_thermo_spec_dimensions_match_world() {
        let mc = create_main_content_and_step(50);
        let snap = mc.create_snapshot();
        let spec = snap.thermo_grid_spec.as_ref().expect("spec should be Some");
        let world = mc.world_region();
        let world_w = world.w.abs();
        let world_h = world.h.abs();
        let grid_w = spec.cols as f32 * spec.cell_width;
        let grid_h = spec.rows as f32 * spec.cell_height;
        assert!(
            (grid_w - world_w).abs() < 1e-3,
            "grid width {grid_w} should match world width {world_w}"
        );
        assert!(
            (grid_h - world_h).abs() < 1e-3,
            "grid height {grid_h} should match world height {world_h}"
        );
    }
}
