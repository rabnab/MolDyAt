//! Renderer Snapshot Module
//!
//! ARCHITECTURAL CONSTRAINT: This module must not depend on ggez.
//! All ggez-specific code belongs in src/application/ggez/.
//!
//! This module provides a framework-independent snapshot of the simulation state
//! for rendering. It contains only positions, colors (as RGBA arrays), and neighbor
//! information, using only standard Rust types.
//!
//! # Design: Array-of-Structures
//!
//! This module uses an **array-of-structures** design (single `Vec<BallRenderData>`)
//! rather than **structure-of-arrays** (separate vectors for each field).
//!
//! Rationale:
//! - **Type safety**: Impossible to have inconsistent vector lengths.
//! - **Simplicity**: Data for each ball is grouped together logically.
//! - **Access pattern**: Rendering typically needs all fields (position, color, radius)
//!   together for each ball, so SoA cache benefits don't apply.
//! - **Immutability**: Snapshots are immutable after creation, so no need for
//!   partial field updates that might benefit from SoA.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use std::time::Instant;

/// Rendering data for a single ball.
///
/// Contains all information needed to render one ball, including visual appearance
/// and neighbor relationships for visualization.
#[derive(Clone, Debug)]
pub struct BallRenderData {
    /// Ball ID for debugging and tracking
    pub id: usize,

    /// Ball size index (0-4 for different size classes)
    pub size_idx: usize,

    /// Ball position in 2D space
    pub position: Vec2D,

    /// Ball color as RGBA [r, g, b, a] in range [0.0, 1.0]
    /// NOT ggez::Color to maintain framework independence
    pub color: [f32; 4],

    /// Ball radius for rendering (must be positive and finite)
    pub radius: f32,

    /// Whether to draw neighbor lines for this ball (verbose mode)
    pub show_neighbor_lines: bool,

    /// Whether this ball shows only main neighbors (stronger forces)
    pub show_main_neighbors_only: bool,

    /// Indices of this ball's neighbors in the snapshot with force magnitudes
    /// Each entry is (neighbor_index, force_magnitude)
    /// Used for visualizing neighbor relationships with color-coded force strength
    pub neighbors: Vec<(usize, f32)>,
}

/// A snapshot of simulation state for rendering.
///
/// Contains only the data needed for visualization, using framework-independent types.
/// Uses array-of-structures design to guarantee consistency and simplify ball data access.
#[derive(Clone)]
pub struct RendererSnapshot {
    /// All balls in the simulation with their rendering data
    pub balls: Vec<BallRenderData>,

    /// Simulation step counter (for display)
    pub simulation_step: u64,

    /// Kinetic energy of the system
    pub kinetic_energy: f32,

    /// Potential energy of the system
    pub potential_energy: f32,

    /// Simulation time
    pub time: f32,

    /// Status text for display (ball count, FPS, etc.)
    pub status_text: Option<String>,

    /// Cell grid world bounds (for rendering the grid)
    pub cell_grid_bounds: crate::objects2d::Rect,

    /// Current ball size index for cursor display
    pub cursor_ball_size_idx: usize,

    /// Timestamp for debugging
    pub created_at: Instant,

    pub grid_structure: CellGrid,
}

impl RendererSnapshot {
    /// Creates a new empty snapshot
    pub fn new() -> Self {
        Self {
            balls: Vec::new(),
            simulation_step: 0,
            kinetic_energy: 0.0,
            potential_energy: 0.0,
            time: 0.0,
            status_text: None,
            cell_grid_bounds: crate::objects2d::Rect::new(0.0, 0.0, 0.0, 0.0),
            cursor_ball_size_idx: 0,
            created_at: Instant::now(),
            grid_structure: CellGrid::default(),
        }
    }

    /// Creates a snapshot with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            balls: Vec::with_capacity(capacity),
            simulation_step: 0,
            kinetic_energy: 0.0,
            potential_energy: 0.0,
            time: 0.0,
            status_text: None,
            cell_grid_bounds: crate::objects2d::Rect::new(0.0, 0.0, 0.0, 0.0),
            cursor_ball_size_idx: 0,
            created_at: Instant::now(),
            grid_structure: CellGrid::default(),
        }
    }

    /// Returns the number of balls in this snapshot
    pub fn len(&self) -> usize {
        self.balls.len()
    }

    /// Returns true if the snapshot contains no balls
    pub fn is_empty(&self) -> bool {
        self.balls.is_empty()
    }

    /// Returns the total energy (kinetic + potential)
    pub fn total_energy(&self) -> f32 {
        self.kinetic_energy + self.potential_energy
    }
}

impl Default for RendererSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert size class to RGBA color (framework-independent)
/// This function determines ball colors based on their size class.
pub fn size_class_to_rgba(size_class: usize) -> [f32; 4] {
    match size_class {
        0 => [1.0, 0.0, 0.0, 1.0], // Red
        1 => [0.0, 1.0, 0.0, 1.0], // Green
        2 => [0.0, 0.0, 1.0, 1.0], // Blue
        3 => [1.0, 1.0, 0.0, 1.0], // Yellow
        4 => [1.0, 0.0, 1.0, 1.0], // Magenta
        _ => [1.0, 1.0, 1.0, 1.0], // White (fallback)
    }
}

/// Apply special coloring for verbose or braking balls
/// Returns Some(color) if special coloring should override size class color
pub fn get_special_ball_color(is_verbose: bool, is_braking: bool) -> Option<[f32; 4]> {
    if is_verbose {
        Some([0.0, 0.0, 1.0, 1.0]) // Blue for verbose balls
    } else if is_braking {
        Some([1.0, 1.0, 0.0, 1.0]) // Yellow for braking balls
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_new() {
        let snapshot = RendererSnapshot::new();
        assert_eq!(snapshot.len(), 0);
        assert!(snapshot.is_empty());
    }

    #[test]
    fn test_snapshot_with_capacity() {
        let snapshot = RendererSnapshot::with_capacity(100);
        assert_eq!(snapshot.len(), 0);
        assert_eq!(snapshot.balls.capacity(), 100);
    }

    #[test]
    fn test_size_class_to_rgba() {
        assert_eq!(size_class_to_rgba(0), [1.0, 0.0, 0.0, 1.0]); // Red
        assert_eq!(size_class_to_rgba(1), [0.0, 1.0, 0.0, 1.0]); // Green
        assert_eq!(size_class_to_rgba(2), [0.0, 0.0, 1.0, 1.0]); // Blue
        assert_eq!(size_class_to_rgba(3), [1.0, 1.0, 0.0, 1.0]); // Yellow
        assert_eq!(size_class_to_rgba(4), [1.0, 0.0, 1.0, 1.0]); // Magenta
        assert_eq!(size_class_to_rgba(999), [1.0, 1.0, 1.0, 1.0]); // White (fallback)
    }

    #[test]
    fn test_colors_are_valid_rgba() {
        for size_class in 0..6 {
            let color = size_class_to_rgba(size_class);
            // All RGBA values should be in [0.0, 1.0]
            assert!(color[0] >= 0.0 && color[0] <= 1.0);
            assert!(color[1] >= 0.0 && color[1] <= 1.0);
            assert!(color[2] >= 0.0 && color[2] <= 1.0);
            assert!(color[3] >= 0.0 && color[3] <= 1.0);
        }
    }

    #[test]
    fn test_get_special_ball_color() {
        assert_eq!(
            get_special_ball_color(true, false),
            Some([0.0, 0.0, 1.0, 1.0])
        ); // Verbose -> Blue
        assert_eq!(
            get_special_ball_color(false, true),
            Some([1.0, 1.0, 0.0, 1.0])
        ); // Braking -> Yellow
        assert_eq!(
            get_special_ball_color(true, true),
            Some([0.0, 0.0, 1.0, 1.0])
        ); // Verbose takes priority
        assert_eq!(get_special_ball_color(false, false), None); // No special color
    }
}
