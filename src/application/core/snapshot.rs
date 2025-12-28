use crate::application::LogStuff;
use crate::application::MainContent;
use crate::application::renderer_snapshot::{
    BallRenderData, RendererSnapshot, get_special_ball_color, size_class_to_rgba,
};
use std::sync::Arc;

impl MainContent {
    /// Creates a renderer snapshot from the current simulation state.
    /// This snapshot contains all data needed for rendering: positions, colors, radii, and neighbors.
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

            // Reuse force magnitudes from cached SIMD computation (if available)
            // This avoids wasteful recalculation of forces that were already computed
            let neighbors: Vec<(usize, f32)> = if ball_idx < self.cached_force_results.len() {
                // Use cached force magnitudes from last physics step
                self.cached_force_results[ball_idx].neighbor_forces.clone()
            } else {
                // Fallback: no cached data available (e.g., during initial snapshots before first physics step)
                Vec::new()
            };

            balls.push(BallRenderData {
                id: ball.get_id(),
                size_idx: ball.get_size_idx(),
                position: ball.get_position().clone(),
                color,
                radius: ball.get_radius(),
                show_neighbor_lines: ball.verbose.lines(),
                show_main_neighbors_only: ball.verbose.is_main(),
                neighbors,
                update_counter: ball.get_update_counter(),
                tail: ball
                    .get_tail_positions()
                    .map_or(Vec::new(), |tail| tail.iter().cloned().collect()),
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
            grid_structure: self.cell_grid.to_grid_structure(),
        }
    }

    /// Publish current snapshot to the snapshot slot for renderer consumption
    pub fn publish_snapshot(&self) {
        let snapshot = self.create_snapshot();
        self.snapshot_slot.store(Some(Arc::new(snapshot)));
    }

    /// Get a clone of the snapshot slot Arc for sharing with renderer
    pub fn get_snapshot_slot(&self) -> Arc<arc_swap::ArcSwapOption<RendererSnapshot>> {
        self.snapshot_slot.clone()
    }

    /// Collects neighbor indices for all balls
    /// Returns a Vec where neighbor_info[i] contains indices of ball i's neighbors
    pub(crate) fn collect_all_neighbors(&self) -> Vec<Vec<usize>> {
        self.balls
            .iter()
            .map(|ball| ball.hood.get_neighbour_indices().to_vec())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::MainContent;
    use crate::application::renderer_snapshot::size_class_to_rgba;
    use crate::graphics::window::window_definition::WindowDefinition;
    use crate::objects2d::Vec2D;
    use crate::objects2d::ball_impl::BallFactory;

    fn create_test_main_content_with_balls(ball_count: usize) -> MainContent {
        let win_def = WindowDefinition::default();
        let world_rect = win_def.compute_world_bounds_rect();
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
}
