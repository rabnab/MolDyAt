//! Coordinate transformation utilities for screen ↔ world coordinate conversion.
//!
//! This module provides shared coordinate transformation logic used by both
//! ggez and wgpu renderers to convert between screen pixel coordinates and
//! world simulation coordinates, accounting for viewport pan/zoom.

use crate::graphics::renderer::viewport::ViewportState;
use crate::objects2d::Vec2D;

/// Coordinate transformation context.
///
/// Encapsulates the parameters needed to transform between screen space
/// (pixels) and world space (simulation coordinates), accounting for
/// margins, viewport state (pan/zoom), and world dimensions.
#[derive(Debug, Clone)]
pub struct CoordinateTransform {
    /// Window size in pixels (width, height)
    window_size_px: (f32, f32),
    /// Margin around simulation viewport in pixels (x, y)
    margin_px: (f32, f32),
    /// World dimensions (width, height) at 1.0x zoom
    world_size: (f32, f32),
}

impl CoordinateTransform {
    /// Pixel-to-world-length conversion factor.
    /// 1 world unit = 10 pixels (0.1 world units per pixel)
    pub const LEN_TO_PX: f32 = 0.1;

    /// Create a new coordinate transform context.
    ///
    /// # Arguments
    /// * `window_size_px` - Window size in pixels (width, height)
    /// * `margin_px` - Margin around viewport in pixels (x, y)
    /// * `world_size` - World dimensions (width, height) at 1.0x zoom
    pub fn new(window_size_px: (f32, f32), margin_px: (f32, f32), world_size: (f32, f32)) -> Self {
        Self {
            window_size_px,
            margin_px,
            world_size,
        }
    }

    /// Create from window size and margins, computing world size automatically.
    ///
    /// Uses the standard LEN_TO_PX conversion factor to determine world dimensions.
    ///
    /// # Arguments
    /// * `window_width` - Window width in pixels
    /// * `window_height` - Window height in pixels
    /// * `margin_px` - Margin around viewport in pixels (x, y)
    pub fn from_window_size(window_width: f32, window_height: f32, margin_px: (f32, f32)) -> Self {
        let viewport_width = (window_width - 2.0 * margin_px.0).max(1.0);
        let viewport_height = (window_height - 2.0 * margin_px.1).max(1.0);

        let world_width = viewport_width * Self::LEN_TO_PX;
        let world_height = viewport_height * Self::LEN_TO_PX;

        Self {
            window_size_px: (window_width, window_height),
            margin_px,
            world_size: (world_width, world_height),
        }
    }

    /// Convert screen pixel coordinates to world coordinates.
    ///
    /// # Arguments
    /// * `screen_x` - X coordinate in screen pixels (relative to window)
    /// * `screen_y` - Y coordinate in screen pixels (relative to window)
    /// * `viewport` - Current viewport state (pan/zoom)
    ///
    /// # Returns
    /// Position in world coordinates accounting for viewport transform
    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32, viewport: &ViewportState) -> Vec2D {
        let viewport_width = (self.window_size_px.0 - 2.0 * self.margin_px.0).max(1.0);
        let viewport_height = (self.window_size_px.1 - 2.0 * self.margin_px.1).max(1.0);

        // Normalize screen coordinates to [0, 1] within viewport
        let vp_x = (screen_x - self.margin_px.0) / viewport_width;
        let vp_y = (screen_y - self.margin_px.1) / viewport_height;

        // Compute visible world dimensions at current zoom
        let zoom = viewport.zoom();
        let world_visible_width = self.world_size.0 / zoom;
        let world_visible_height = self.world_size.1 / zoom;

        // Convert to world coordinates centered on pan offset
        let pan = viewport.pan_offset();
        let world_x = pan.x + (vp_x - 0.5) * world_visible_width;
        let world_y = pan.y - (vp_y - 0.5) * world_visible_height;

        Vec2D::new(world_x, world_y)
    }

    /// Convert world coordinates to screen pixel coordinates.
    ///
    /// # Arguments
    /// * `world_pos` - Position in world coordinates
    /// * `viewport` - Current viewport state (pan/zoom)
    ///
    /// # Returns
    /// Position in screen pixels (relative to window)
    pub fn world_to_screen(&self, world_pos: &Vec2D, viewport: &ViewportState) -> [f32; 2] {
        let viewport_width = (self.window_size_px.0 - 2.0 * self.margin_px.0).max(1.0);
        let viewport_height = (self.window_size_px.1 - 2.0 * self.margin_px.1).max(1.0);

        // Compute visible world dimensions at current zoom
        let zoom = viewport.zoom();
        let world_visible_width = self.world_size.0 / zoom;
        let world_visible_height = self.world_size.1 / zoom;

        // Convert from world space to normalized viewport space [0, 1]
        let pan = viewport.pan_offset();
        let vp_x = (world_pos.x - pan.x) / world_visible_width + 0.5;
        let vp_y = (pan.y - world_pos.y) / world_visible_height + 0.5;

        // Convert to screen pixels
        let screen_x = vp_x * viewport_width + self.margin_px.0;
        let screen_y = vp_y * viewport_height + self.margin_px.1;

        [screen_x, screen_y]
    }

    /// Update window size (e.g., when window is resized).
    ///
    /// # Arguments
    /// * `window_width` - New window width in pixels
    /// * `window_height` - New window height in pixels
    pub fn set_window_size(&mut self, window_width: f32, window_height: f32) {
        self.window_size_px = (window_width, window_height);

        // Recalculate world size based on new viewport dimensions
        let viewport_width = (window_width - 2.0 * self.margin_px.0).max(1.0);
        let viewport_height = (window_height - 2.0 * self.margin_px.1).max(1.0);

        self.world_size.0 = viewport_width * Self::LEN_TO_PX;
        self.world_size.1 = viewport_height * Self::LEN_TO_PX;
    }

    /// Get current window size in pixels.
    pub fn window_size_px(&self) -> (f32, f32) {
        self.window_size_px
    }

    /// Get viewport size in pixels (window size minus margins).
    pub fn viewport_size_px(&self) -> (f32, f32) {
        (
            (self.window_size_px.0 - 2.0 * self.margin_px.0).max(1.0),
            (self.window_size_px.1 - 2.0 * self.margin_px.1).max(1.0),
        )
    }

    /// Get world size at 1.0x zoom.
    pub fn world_size(&self) -> (f32, f32) {
        self.world_size
    }

    /// Get margin in pixels.
    pub fn margin_px(&self) -> (f32, f32) {
        self.margin_px
    }
}

/// Compute world rectangle bounds from window size.
///
/// Helper function to calculate the world coordinate bounds for a given
/// window size, accounting for margins. The result is centered at the origin.
///
/// # Arguments
/// * `window_width` - Window width in pixels
/// * `window_height` - Window height in pixels
/// * `margin` - Margin around viewport in pixels (x, y)
///
/// # Returns
/// Rectangle with world bounds: (x, y, width, height) where:
/// - x, y = top-left corner
/// - width, height = dimensions (height is negative for top-down coordinates)
pub fn compute_world_rect_from_window_size(
    window_width: u32,
    window_height: u32,
    margin: (f32, f32),
) -> crate::objects2d::Rect {
    let viewport_width = (window_width as f32 - 2.0 * margin.0).max(1.0);
    let viewport_height = (window_height as f32 - 2.0 * margin.1).max(1.0);

    let world_width = viewport_width * CoordinateTransform::LEN_TO_PX;
    let world_height = viewport_height * CoordinateTransform::LEN_TO_PX;

    // Center at origin with top-left coordinate system
    crate::objects2d::Rect::new(
        -world_width / 2.0,
        world_height / 2.0,
        world_width,
        -world_height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_to_world_no_pan_no_zoom() {
        let transform = CoordinateTransform::new((1000.0, 800.0), (10.0, 10.0), (98.0, 78.0));
        let viewport = ViewportState::new(crate::objects2d::Rect::new(-49.0, 39.0, 98.0, -78.0));

        // Center of screen should map to center of world (0, 0)
        let center_world = transform.screen_to_world(500.0, 400.0, &viewport);
        assert!((center_world.x - 0.0).abs() < 0.01);
        assert!((center_world.y - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_world_to_screen_no_pan_no_zoom() {
        let transform = CoordinateTransform::new((1000.0, 800.0), (10.0, 10.0), (98.0, 78.0));
        let viewport = ViewportState::new(crate::objects2d::Rect::new(-49.0, 39.0, 98.0, -78.0));

        // World center should map to screen center
        let screen_pos = transform.world_to_screen(&Vec2D::zero(), &viewport);
        assert!((screen_pos[0] - 500.0).abs() < 0.1);
        assert!((screen_pos[1] - 400.0).abs() < 0.1);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let transform = CoordinateTransform::new((1280.0, 720.0), (5.0, 5.0), (127.0, 71.0));
        let viewport = ViewportState::new(crate::objects2d::Rect::new(-63.5, 35.5, 127.0, -71.0));

        // Arbitrary screen position
        let screen_x = 640.0;
        let screen_y = 360.0;

        let world_pos = transform.screen_to_world(screen_x, screen_y, &viewport);
        let screen_pos = transform.world_to_screen(&world_pos, &viewport);

        assert!((screen_pos[0] - screen_x).abs() < 0.1);
        assert!((screen_pos[1] - screen_y).abs() < 0.1);
    }

    #[test]
    fn test_compute_world_rect() {
        let rect = compute_world_rect_from_window_size(1000, 800, (10.0, 10.0));

        // World dimensions should match viewport pixels * LEN_TO_PX
        assert!((rect.w - 98.0).abs() < 0.01);
        assert!((rect.h + 78.0).abs() < 0.01); // negative height

        // Should be centered at origin (x + w/2 = 0, y + h/2 = 0)
        // For x: -49.0 + 98.0/2 = -49.0 + 49.0 = 0
        // For y: 39.0 + (-78.0)/2 = 39.0 - 39.0 = 0
        assert!((rect.x + rect.w / 2.0).abs() < 0.01);
        assert!((rect.y + rect.h / 2.0).abs() < 0.01);
    }
}
