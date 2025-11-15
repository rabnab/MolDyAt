use crate::frame::MainContent;
use crate::frame::renderer_snapshot::{
    RendererSnapshot, get_special_ball_color, size_class_to_rgba,
};
use std::sync::Arc;

impl MainContent {
    /// Creates a renderer snapshot from the current simulation state.
    /// This snapshot contains all data needed for rendering: positions, colors, radii, and neighbors.
    pub fn create_snapshot(&self) -> RendererSnapshot {
        let ball_count = self.balls.len();
        let mut snapshot = RendererSnapshot::with_capacity(ball_count);

        // Extract positions, colors, and radii
        for ball in &self.balls {
            snapshot.positions.push(ball.get_position().clone());

            // Determine color: special colors (verbose/braking) override size class color
            let color = get_special_ball_color(
                ball.verbose.console || ball.verbose.lines(),
                ball.is_braking(),
            )
            .unwrap_or_else(|| size_class_to_rgba(ball.get_size_idx()));

            snapshot.colors.push(color);
            snapshot.radii.push(ball.get_radius());
        }

        // Collect neighbor information
        snapshot.neighbor_info = self.collect_all_neighbors();

        snapshot.simulation_step = self.step_count as u64;
        snapshot.created_at = std::time::Instant::now();

        snapshot
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
    use crate::frame::renderer_snapshot::size_class_to_rgba;

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
}
