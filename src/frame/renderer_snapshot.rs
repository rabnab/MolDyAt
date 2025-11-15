//! Renderer Snapshot Module
//!
//! ARCHITECTURAL CONSTRAINT: This module must not depend on ggez.
//! All ggez-specific code belongs in src/frame/ggez/.
//!
//! This module provides a framework-independent snapshot of the simulation state
//! for rendering. It contains only positions, colors (as RGBA arrays), and neighbor
//! information, using only standard Rust types.

use crate::objects2d::Vec2D;
use std::time::Instant;

/// A snapshot of simulation state for rendering.
/// Contains only the data needed for visualization, using framework-independent types.
#[derive(Clone)]
pub struct RendererSnapshot {
    /// Ball positions (x, y) for rendering
    pub positions: Vec<Vec2D>,

    /// Ball colors as RGBA [r, g, b, a] in range [0.0, 1.0]
    /// NOT ggez::Color to maintain framework independence
    pub colors: Vec<[f32; 4]>,

    /// Ball radii for rendering (needed for circle drawing)
    pub radii: Vec<f32>,

    /// Neighbor relationships for visualization
    /// neighbor_info[i] contains indices of neighbors of ball i
    pub neighbor_info: Vec<Vec<usize>>,

    /// Simulation step counter (for display)
    pub simulation_step: u64,

    /// Timestamp for debugging
    pub created_at: Instant,
}

impl RendererSnapshot {
    /// Creates a new empty snapshot
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            colors: Vec::new(),
            radii: Vec::new(),
            neighbor_info: Vec::new(),
            simulation_step: 0,
            created_at: Instant::now(),
        }
    }

    /// Creates a snapshot with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            positions: Vec::with_capacity(capacity),
            colors: Vec::with_capacity(capacity),
            radii: Vec::with_capacity(capacity),
            neighbor_info: Vec::with_capacity(capacity),
            simulation_step: 0,
            created_at: Instant::now(),
        }
    }

    /// Returns the number of balls in this snapshot
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Returns true if the snapshot contains no balls
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
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
        assert_eq!(snapshot.positions.capacity(), 100);
        assert_eq!(snapshot.colors.capacity(), 100);
        assert_eq!(snapshot.radii.capacity(), 100);
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
