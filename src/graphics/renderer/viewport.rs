use crate::objects2d::{Rect, Vec2D};

/// Viewport state for zoom and pan controls
#[derive(Debug, Clone)]
pub struct ViewportState {
    /// Zoom level (1.0 = full box, 100.0 = max zoom)
    zoom: f32,
    /// Pan offset in world coordinates
    pan_offset: Vec2D,
    /// World bounds (the full simulation box)
    world_bounds: Rect,
}

impl ViewportState {
    /// Minimum zoom level (cannot zoom out beyond full simulation box)
    pub const MIN_ZOOM: f32 = 1.0;
    /// Maximum zoom level
    pub const MAX_ZOOM: f32 = 100.0;

    /// Create a new viewport state with default view (1.0x zoom, centered)
    pub fn new(world_bounds: Rect) -> Self {
        Self {
            zoom: Self::MIN_ZOOM,
            pan_offset: Vec2D::zero(),
            world_bounds,
        }
    }

    /// Get current zoom level
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Get current pan offset in world coordinates
    pub fn pan_offset(&self) -> &Vec2D {
        &self.pan_offset
    }

    /// Get world bounds
    pub fn world_bounds(&self) -> &Rect {
        &self.world_bounds
    }

    /// Update world bounds (called when window is resized)
    pub fn set_world_bounds(&mut self, world_bounds: Rect) {
        self.world_bounds = world_bounds;
        // Re-constrain pan to keep content visible
        self.constrain_pan();
    }

    /// Zoom in/out at a specific point in world coordinates while keeping that point fixed on screen.
    ///
    /// # Arguments
    /// * `world_point` - Point in world coordinates to zoom toward (typically cursor position)
    /// * `zoom_delta` - Multiplicative zoom factor (e.g., 1.1 to zoom in 10%, 0.9 to zoom out 10%)
    ///
    /// # Returns
    /// `true` if zoom level changed, `false` if already at zoom limits
    pub fn zoom_at_point(&mut self, world_point: &Vec2D, zoom_delta: f32) -> bool {
        let old_zoom = self.zoom;

        // Calculate new zoom level
        let new_zoom = (self.zoom * zoom_delta).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);

        // If zoom didn't change (hit limits), return false
        if (new_zoom - old_zoom).abs() < 1e-6 {
            return false;
        }

        // Adjust pan to zoom at the cursor position
        // Goal: keep world_point at the same screen position after zoom
        //
        // The relationship is: screen_pos = (world_point - pan_offset) * zoom
        // Before: screen_pos = (world_point - old_pan) * old_zoom
        // After:  screen_pos = (world_point - new_pan) * new_zoom
        //
        // Since screen_pos must be the same:
        // (world_point - old_pan) * old_zoom = (world_point - new_pan) * new_zoom
        // world_point - old_pan = (world_point - new_pan) * (new_zoom / old_zoom)
        // world_point - old_pan = world_point * (new_zoom / old_zoom) - new_pan * (new_zoom / old_zoom)
        // new_pan * (new_zoom / old_zoom) = world_point * (new_zoom / old_zoom) - world_point + old_pan
        // new_pan = world_point - (world_point - old_pan) * (old_zoom / new_zoom)

        let zoom_ratio = old_zoom / new_zoom;
        let offset_from_point = &self.pan_offset - world_point;
        let new_offset_from_point = offset_from_point.scaled(zoom_ratio);
        self.pan_offset = world_point + &new_offset_from_point;
        self.zoom = new_zoom;

        // Constrain pan to keep simulation box visible
        self.constrain_pan();

        true
    }

    /// Set pan offset to a specific position in world coordinates.
    /// Pan is automatically constrained to keep simulation box visible.
    ///
    /// # Arguments
    /// * `pan` - New pan offset in world coordinates (center of viewport)
    pub fn set_pan(&mut self, pan: Vec2D) {
        self.pan_offset = pan;
        self.constrain_pan();
    }

    /// Adjust pan offset by a delta in world coordinates.
    ///
    /// # Arguments
    /// * `delta` - Amount to move the viewport in world coordinates
    pub fn adjust_pan(&mut self, delta: &Vec2D) {
        self.pan_offset = &self.pan_offset + delta;
        self.constrain_pan();
    }

    /// Reset to default view (1.0x zoom, centered)
    pub fn reset(&mut self) {
        self.zoom = Self::MIN_ZOOM;
        self.pan_offset = Vec2D::zero();
    }

    /// Get the visible world rectangle (accounting for zoom and pan).
    ///
    /// # Arguments
    /// * `viewport_size_world` - Size of viewport in world coordinates at 1.0x zoom (width, height)
    ///
    /// # Returns
    /// Rectangle representing the visible portion of the world
    pub fn visible_world_rect(&self, viewport_size_world: (f32, f32)) -> Rect {
        // At zoom 1.0, we see the full world
        // At zoom 2.0, we see half the world, etc.
        let visible_width = viewport_size_world.0 / self.zoom;
        let visible_height = viewport_size_world.1 / self.zoom;

        // Center on pan offset
        let left = self.pan_offset.x - visible_width / 2.0;
        let top = self.pan_offset.y + visible_height / 2.0;

        Rect::new(left, top, visible_width, -visible_height)
    }

    /// Constrain pan to keep viewport within the simulation box bounds
    /// This ensures the viewport never shows areas outside the world region (reflective walls)
    fn constrain_pan(&mut self) {
        let world = &self.world_bounds;

        // Calculate world bounds extents
        // Note: world.h can be negative (y-axis inverted), so we normalize
        let world_left = world.x;
        let world_right = world.x + world.w;
        let world_top = world.y;
        let world_bottom = world.y + world.h;

        // Normalize to handle negative heights
        let world_min_x = world_left.min(world_right);
        let world_max_x = world_left.max(world_right);
        let world_min_y = world_top.min(world_bottom);
        let world_max_y = world_top.max(world_bottom);

        let world_width = world_max_x - world_min_x;
        let world_height = world_max_y - world_min_y;

        // At zoom level z, the visible area has width = world_width / z
        // The pan_offset is the center of the visible area
        // So visible area extends from: pan_offset - visible_width/2 to pan_offset + visible_width/2

        // To keep viewport within world bounds:
        // pan_offset - visible_width/2 >= world_min_x
        // pan_offset + visible_width/2 <= world_max_x
        // Which gives: world_min_x + visible_width/2 <= pan_offset <= world_max_x - visible_width/2

        let visible_half_width = world_width / (2.0 * self.zoom);
        let visible_half_height = world_height / (2.0 * self.zoom);

        // Calculate valid pan range
        let min_pan_x = world_min_x + visible_half_width;
        let max_pan_x = world_max_x - visible_half_width;
        let min_pan_y = world_min_y + visible_half_height;
        let max_pan_y = world_max_y - visible_half_height;

        // Clamp pan offset to valid range
        // If the visible area is larger than the world (zoom < 1.0), center on world
        if max_pan_x < min_pan_x {
            self.pan_offset.x = (world_min_x + world_max_x) / 2.0;
        } else {
            self.pan_offset.x = self.pan_offset.x.clamp(min_pan_x, max_pan_x);
        }

        if max_pan_y < min_pan_y {
            self.pan_offset.y = (world_min_y + world_max_y) / 2.0;
        } else {
            self.pan_offset.y = self.pan_offset.y.clamp(min_pan_y, max_pan_y);
        }
    }

    /// Check if a world point is visible in the current viewport.
    ///
    /// # Arguments
    /// * `point` - Point in world coordinates to test
    /// * `viewport_size_world` - Size of viewport in world coordinates at 1.0x zoom
    ///
    /// # Returns
    /// `true` if point is within visible bounds
    pub fn is_point_visible(&self, point: &Vec2D, viewport_size_world: (f32, f32)) -> bool {
        let visible_rect = self.visible_world_rect(viewport_size_world);

        point.x >= visible_rect.x
            && point.x <= visible_rect.x + visible_rect.w
            && point.y <= visible_rect.y
            && point.y >= visible_rect.y + visible_rect.h
    }

    /// Check if a circle (ball) is at least partially visible (frustum culling).
    ///
    /// # Arguments
    /// * `center` - Center of circle in world coordinates
    /// * `radius` - Radius of circle in world units
    /// * `viewport_size_world` - Size of viewport in world coordinates at 1.0x zoom
    ///
    /// # Returns
    /// `true` if any part of the circle intersects the visible viewport
    pub fn is_circle_visible(
        &self,
        center: &Vec2D,
        radius: f32,
        viewport_size_world: (f32, f32),
    ) -> bool {
        let visible_rect = self.visible_world_rect(viewport_size_world);

        // Check if circle intersects with visible rectangle
        // Find the closest point on the rectangle to the circle center
        let closest_x = center
            .x
            .clamp(visible_rect.x, visible_rect.x + visible_rect.w);
        let closest_y = center
            .y
            .clamp(visible_rect.y + visible_rect.h, visible_rect.y);

        // Check if closest point is within circle radius
        let dx = center.x - closest_x;
        let dy = center.y - closest_y;
        let distance_sq = dx * dx + dy * dy;

        distance_sq <= radius * radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn test_viewport_default() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let viewport = ViewportState::new(world);

        assert!(approx_eq(viewport.zoom(), 1.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 0.0, 1e-6));
    }

    #[test]
    fn test_zoom_limits() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut viewport = ViewportState::new(world);

        // Try to zoom out below minimum
        let point = Vec2D::zero();
        viewport.zoom_at_point(&point, 0.5);
        assert!(approx_eq(viewport.zoom(), ViewportState::MIN_ZOOM, 1e-6));

        // Zoom in to maximum
        viewport.zoom_at_point(&point, 200.0);
        assert!(approx_eq(viewport.zoom(), ViewportState::MAX_ZOOM, 1e-6));

        // Try to zoom beyond maximum
        let changed = viewport.zoom_at_point(&point, 2.0);
        assert!(!changed);
        assert!(approx_eq(viewport.zoom(), ViewportState::MAX_ZOOM, 1e-6));
    }

    #[test]
    fn test_zoom_at_point_center() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut viewport = ViewportState::new(world);

        // Zoom at center point - pan should remain at origin
        let center = Vec2D::zero();
        viewport.zoom_at_point(&center, 2.0);

        assert!(approx_eq(viewport.zoom(), 2.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 0.0, 1e-6));
    }

    #[test]
    fn test_zoom_at_point_offset() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut viewport = ViewportState::new(world);

        // Zoom at an offset point
        let point = Vec2D::new(5.0, 3.0);
        viewport.zoom_at_point(&point, 2.0);

        // Calculate what the pan should be:
        // We want (5,3) to map to the same screen position before and after zoom
        // Before: screen = (5-0)*1 = 5, 3
        // After:  screen = (5-pan)*2, so 5 = (5-pan)*2, pan = 5 - 5/2 = 2.5
        println!(
            "After first zoom: zoom={}, pan=({}, {})",
            viewport.zoom(),
            viewport.pan_offset().x,
            viewport.pan_offset().y
        );

        assert!(approx_eq(viewport.zoom(), 2.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().x, 2.5, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 1.5, 1e-6));

        // Zoom again at the same point
        viewport.zoom_at_point(&point, 2.0);

        println!(
            "After second zoom: zoom={}, pan=({}, {})",
            viewport.zoom(),
            viewport.pan_offset().x,
            viewport.pan_offset().y
        );

        // After second zoom to 4x:
        // screen = (5-pan)*4, so 10 = (5-pan)*4, pan = 5 - 10/4 = 2.5
        // Wait, screen position should be the same as before...
        // Before second zoom: screen = (5-2.5)*2 = 5
        // After second zoom:  screen = (5-pan)*4, so 5 = (5-pan)*4, pan = 5 - 5/4 = 3.75
        assert!(approx_eq(viewport.zoom(), 4.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().x, 3.75, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 2.25, 1e-6));
    }

    #[test]
    fn test_reset() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut viewport = ViewportState::new(world);

        // Zoom and pan
        let point = Vec2D::new(5.0, 5.0);
        viewport.zoom_at_point(&point, 10.0);
        viewport.set_pan(Vec2D::new(3.0, -2.0));

        // Reset
        viewport.reset();

        assert!(approx_eq(viewport.zoom(), 1.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 0.0, 1e-6));
    }

    #[test]
    fn test_visible_world_rect() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut viewport = ViewportState::new(world);

        // At 1x zoom, centered, with viewport size 20x20 world units
        let visible = viewport.visible_world_rect((20.0, 20.0));
        assert!(approx_eq(visible.x, -10.0, 1e-6));
        assert!(approx_eq(visible.y, 10.0, 1e-6));
        assert!(approx_eq(visible.w, 20.0, 1e-6));
        assert!(approx_eq(visible.h, -20.0, 1e-6));

        // At 2x zoom, centered
        viewport.zoom_at_point(&Vec2D::zero(), 2.0);
        let visible = viewport.visible_world_rect((20.0, 20.0));
        assert!(approx_eq(visible.w, 10.0, 1e-6));
        assert!(approx_eq(visible.h, -10.0, 1e-6));
    }

    #[test]
    fn test_is_circle_visible() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let viewport = ViewportState::new(world);

        // Circle at center should be visible
        let center = Vec2D::zero();
        assert!(viewport.is_circle_visible(&center, 1.0, (20.0, 20.0)));

        // Circle far outside should not be visible
        let far_point = Vec2D::new(1000.0, 1000.0);
        assert!(!viewport.is_circle_visible(&far_point, 1.0, (20.0, 20.0)));
    }

    #[test]
    fn test_pan_constraints() {
        // World bounds: x=-10 to 10, y=-10 to 10 (note: height is negative in rect)
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut viewport = ViewportState::new(world);

        // At 1x zoom, the visible area is the same size as the world
        // So pan should be constrained to stay at world center (0, 0)
        viewport.set_pan(Vec2D::new(1000.0, 1000.0));
        assert!(approx_eq(viewport.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 0.0, 1e-6));

        // Try to pan in negative direction at 1x zoom
        viewport.set_pan(Vec2D::new(-1000.0, -1000.0));
        assert!(approx_eq(viewport.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 0.0, 1e-6));

        // At 2x zoom, the visible area is half the world size
        // So we can pan up to ±5 in each direction
        viewport.zoom_at_point(&Vec2D::zero(), 2.0);

        viewport.set_pan(Vec2D::new(1000.0, 1000.0));
        assert!(approx_eq(viewport.pan_offset().x, 5.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, 5.0, 1e-6));

        viewport.set_pan(Vec2D::new(-1000.0, -1000.0));
        assert!(approx_eq(viewport.pan_offset().x, -5.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, -5.0, 1e-6));

        // Pan to a position within bounds at 2x zoom should work
        viewport.set_pan(Vec2D::new(3.0, -2.0));
        assert!(approx_eq(viewport.pan_offset().x, 3.0, 1e-6));
        assert!(approx_eq(viewport.pan_offset().y, -2.0, 1e-6));
    }
}
