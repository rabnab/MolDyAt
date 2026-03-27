use crate::objects2d::{Rect, Vec2D};
use std::cell::Cell;

/// Pixel margins around the simulation viewport.
///
/// Supports asymmetric margins (e.g. a wider left margin to accommodate
/// a sidebar panel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Margins {
    /// Create margins with explicit values for each edge.
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Create symmetric margins (same on left/right, same on top/bottom).
    pub const fn symmetric(x: f32, y: f32) -> Self {
        Self {
            left: x,
            top: y,
            right: x,
            bottom: y,
        }
    }

    /// Total horizontal margin (left + right).
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    /// Total vertical margin (top + bottom).
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Simple 3x3 matrix for 2D transformations.
/// Used for efficient coordinate transforms with caching.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    // Row-major order for row vectors: [m0 m1 m2; m3 m4 m5; m6 m7 m8]
    m0: f32,
    m1: f32,
    m2: f32,
    m3: f32,
    m4: f32,
    m5: f32,
    m6: f32,
    m7: f32,
    m8: f32,
}

impl Mat3 {
    /// Create identity matrix
    pub fn identity() -> Self {
        Self {
            m0: 1.0,
            m1: 0.0,
            m2: 0.0,
            m3: 0.0,
            m4: 1.0,
            m5: 0.0,
            m6: 0.0,
            m7: 0.0,
            m8: 1.0,
        }
    }

    /// Create translation matrix
    pub fn translation(tx: f32, ty: f32) -> Self {
        Self {
            m0: 1.0,
            m1: 0.0,
            m2: tx,
            m3: 0.0,
            m4: 1.0,
            m5: ty,
            m6: 0.0,
            m7: 0.0,
            m8: 1.0,
        }
    }

    /// Create scale matrix
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m0: sx,
            m1: 0.0,
            m2: 0.0,
            m3: 0.0,
            m4: sy,
            m5: 0.0,
            m6: 0.0,
            m7: 0.0,
            m8: 1.0,
        }
    }

    /// Matrix multiplication (self * other)
    pub fn mul(&self, other: &Mat3) -> Mat3 {
        Mat3 {
            m0: self.m0 * other.m0 + self.m1 * other.m3 + self.m2 * other.m6,
            m1: self.m0 * other.m1 + self.m1 * other.m4 + self.m2 * other.m7,
            m2: self.m0 * other.m2 + self.m1 * other.m5 + self.m2 * other.m8,
            m3: self.m3 * other.m0 + self.m4 * other.m3 + self.m5 * other.m6,
            m4: self.m3 * other.m1 + self.m4 * other.m4 + self.m5 * other.m7,
            m5: self.m3 * other.m2 + self.m4 * other.m5 + self.m5 * other.m8,
            m6: self.m6 * other.m0 + self.m7 * other.m3 + self.m8 * other.m6,
            m7: self.m6 * other.m1 + self.m7 * other.m4 + self.m8 * other.m7,
            m8: self.m6 * other.m2 + self.m7 * other.m5 + self.m8 * other.m8,
        }
    }

    /// Compute matrix inverse
    pub fn inverse(&self) -> Mat3 {
        let det = self.m0 * (self.m4 * self.m8 - self.m5 * self.m7)
            - self.m1 * (self.m3 * self.m8 - self.m5 * self.m6)
            + self.m2 * (self.m3 * self.m7 - self.m4 * self.m6);

        if det.abs() < 1e-10 {
            return Mat3::identity(); // Fallback to identity if singular
        }

        let inv_det = 1.0 / det;

        Mat3 {
            m0: (self.m4 * self.m8 - self.m5 * self.m7) * inv_det,
            m1: (self.m2 * self.m7 - self.m1 * self.m8) * inv_det,
            m2: (self.m1 * self.m5 - self.m2 * self.m4) * inv_det,
            m3: (self.m5 * self.m6 - self.m3 * self.m8) * inv_det,
            m4: (self.m0 * self.m8 - self.m2 * self.m6) * inv_det,
            m5: (self.m2 * self.m3 - self.m0 * self.m5) * inv_det,
            m6: (self.m3 * self.m7 - self.m4 * self.m6) * inv_det,
            m7: (self.m1 * self.m6 - self.m0 * self.m7) * inv_det,
            m8: (self.m0 * self.m4 - self.m1 * self.m3) * inv_det,
        }
    }

    /// Transform a 2D point: [x, y] -> [x', y']
    pub fn transform_point2(&self, p: &[f32; 2]) -> Vec2D {
        let x = self.m0 * p[0] + self.m1 * p[1] + self.m2;
        let y = self.m3 * p[0] + self.m4 * p[1] + self.m5;
        Vec2D::new(x, y)
    }
}

impl std::ops::Mul for Mat3 {
    type Output = Mat3;

    fn mul(self, other: Mat3) -> Mat3 {
        self.mul(&other)
    }
}

impl std::ops::Mul<&Mat3> for Mat3 {
    type Output = Mat3;

    fn mul(self, other: &Mat3) -> Mat3 {
        Mat3::mul(&self, other)
    }
}

/// Unified viewport transformation system.
/// Provides matrix caching for efficient coordinate transforms and GPU-ready matrices.
/// Manages zoom, pan, and coordinate transformations between world and pixel space.
#[derive(Debug, Clone)]
pub struct ViewportTransform {
    // Logical viewport state
    /// World bounds (the full simulation box)
    world_bounds: Rect,
    /// Zoom level (1.0 = full box, up to 100.0 = max zoom)
    zoom: f32,
    /// Pan offset in world coordinates (center of viewport)
    pan_offset: Vec2D,

    // Physical bounds (pixels)
    /// Window size in pixels (width, height)
    window_size_px: (f32, f32),
    /// Margin around simulation viewport in pixels
    margin_px: Margins,
    /// Viewport rectangle in pixels (position and size)
    viewport_px: Rect,

    // Cached transformation matrices (using Cell for interior mutability)
    /// Transform from world coordinates to pixel coordinates
    world_to_px_matrix: Cell<Mat3>,
    /// Transform from pixel coordinates to world coordinates
    px_to_world_matrix: Cell<Mat3>,

    /// Dirty flag: recalculate matrices on next query (using Cell for interior mutability)
    dirty: Cell<bool>,
}

impl ViewportTransform {
    /// Pixel-to-world-length conversion factor.
    /// 0.1 world units per pixel (1 pixel = 0.1 world units, or 10 pixels = 1 world unit)
    pub const LEN_TO_PX: f32 = 0.1;

    /// Minimum zoom level (cannot zoom out beyond full simulation box)
    pub const MIN_ZOOM: f32 = 1.0;
    /// Maximum zoom level
    pub const MAX_ZOOM: f32 = 100.0;

    /// Create a new viewport transform.
    ///
    /// # Arguments
    /// * `world_bounds` - The full simulation box bounds
    /// * `window_size_px` - Window size in pixels (width, height)
    /// * `margin_px` - Margin around viewport in pixels
    pub fn new(world_bounds: Rect, window_size_px: (f32, f32), margin_px: Margins) -> Self {
        let mut transform = Self {
            world_bounds,
            zoom: Self::MIN_ZOOM,
            pan_offset: Vec2D::zero(),
            window_size_px,
            margin_px,
            viewport_px: Rect::default(),
            world_to_px_matrix: Cell::new(Mat3::identity()),
            px_to_world_matrix: Cell::new(Mat3::identity()),
            dirty: Cell::new(true),
        };
        transform.recalculate_viewport_px();
        transform.recalculate_matrices();
        transform
    }

    /// Compute world rectangle bounds from window size.
    ///
    /// Helper function to calculate the world coordinate bounds for a given
    /// window size, accounting for margins. The result is centered at the origin.
    ///
    /// # Arguments
    /// * `window_width` - Window width in pixels
    /// * `window_height` - Window height in pixels
    /// * `margin` - Margin around viewport in pixels
    ///
    /// # Returns
    /// Rectangle with world bounds: (x, y, width, height) where:
    /// - x, y = top-left corner
    /// - width, height = dimensions (height is negative for top-down coordinates)
    pub fn compute_world_bounds_from_window_size(
        window_width: u32,
        window_height: u32,
        margin: Margins,
    ) -> Rect {
        let viewport_width = (window_width as f32 - margin.horizontal()).max(1.0);
        let viewport_height = (window_height as f32 - margin.vertical()).max(1.0);

        let world_width = viewport_width * Self::LEN_TO_PX;
        let world_height = viewport_height * Self::LEN_TO_PX;

        // Center at origin with top-left coordinate system
        Rect::new(
            -world_width / 2.0,
            world_height / 2.0,
            world_width,
            -world_height,
        )
    }

    // --- Accessors (cheap - use cached matrices) ---

    /// Get current zoom level
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Get current pan offset in world coordinates
    pub fn pan_offset(&self) -> Vec2D {
        self.pan_offset
    }

    /// Get world bounds
    pub fn world_bounds(&self) -> Rect {
        self.world_bounds
    }

    /// Get window size in pixels
    pub fn window_size_px(&self) -> (f32, f32) {
        self.window_size_px
    }

    /// Get viewport size in pixels (window size minus margins)
    pub fn viewport_size_px(&self) -> (f32, f32) {
        (
            (self.window_size_px.0 - self.margin_px.horizontal()).max(1.0),
            (self.window_size_px.1 - self.margin_px.vertical()).max(1.0),
        )
    }

    /// Get the pixel margins.
    pub fn margins(&self) -> Margins {
        self.margin_px
    }

    /// Get viewport rectangle in pixels
    pub fn viewport_px(&self) -> Rect {
        self.viewport_px
    }

    /// Convert pixel coordinates to world coordinates using cached matrix.
    pub fn px_to_world(&self, px_pos: &[f32; 2]) -> Vec2D {
        self.ensure_matrices_calculated();
        self.px_to_world_matrix.get().transform_point2(px_pos)
    }

    /// Convert world coordinates to pixel coordinates using cached matrix.
    pub fn world_to_px(&self, world_pos: &Vec2D) -> [f32; 2] {
        self.ensure_matrices_calculated();
        let result = self
            .world_to_px_matrix
            .get()
            .transform_point2(&[world_pos.x, world_pos.y]);
        [result.x, result.y]
    }

    /// Convert world coordinates (as `[f32; 2]`) to pixel coordinates.
    ///
    /// Convenience variant of [`world_to_px`](Self::world_to_px) that
    /// accepts a raw array instead of a [`Vec2D`] reference.
    pub fn world_to_px_arr(&self, pos: [f32; 2]) -> [f32; 2] {
        self.ensure_matrices_calculated();
        let result = self.world_to_px_matrix.get().transform_point2(&pos);
        [result.x, result.y]
    }

    /// Get world region visible in the current viewport.
    pub fn visible_world_rect(&self) -> Rect {
        self.ensure_matrices_calculated();

        // Calculate visible world dimensions at current zoom
        let world = &self.world_bounds;
        let world_width = (world.w).abs();
        let world_height = (world.h).abs();

        let visible_width = world_width / self.zoom;
        let visible_height = world_height / self.zoom;

        let left = self.pan_offset.x - visible_width / 2.0;
        let top = self.pan_offset.y + visible_height / 2.0;

        Rect::new(left, top, visible_width, -visible_height)
    }

    /// Get GPU-ready transformation matrices.
    /// Returns (world_to_px, px_to_world) matrices.
    pub fn matrices(&self) -> (Mat3, Mat3) {
        self.ensure_matrices_calculated();
        (self.world_to_px_matrix.get(), self.px_to_world_matrix.get())
    }

    // --- Mutations (mark dirty if changed) ---

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
        self.zoom = (self.zoom * zoom_delta).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);

        if (self.zoom - old_zoom).abs() < 1e-6 {
            return false; // No change
        }

        // Adjust pan to zoom at the cursor position
        let zoom_ratio = old_zoom / self.zoom;
        let offset_from_point = self.pan_offset - world_point;
        self.pan_offset = world_point + offset_from_point.scaled(zoom_ratio);
        self.constrain_pan();

        self.dirty.set(true);
        true
    }

    /// Pan to a new position in world coordinates.
    pub fn pan(&mut self, new_pan: Vec2D) {
        self.pan_offset = new_pan;
        self.constrain_pan();
        self.dirty.set(true);
    }

    /// Pan by a delta in world coordinates.
    pub fn adjust_pan(&mut self, delta: &Vec2D) {
        self.pan_offset = self.pan_offset + delta;
        self.constrain_pan();
        self.dirty.set(true);
    }

    /// Reset to default view (1.0x zoom, centered)
    pub fn reset(&mut self) {
        self.zoom = Self::MIN_ZOOM;
        self.pan_offset = Vec2D::zero();
        self.dirty.set(true);
    }

    /// Apply pan update from pixel delta (used by event loop handlers).
    /// Converts pixel space delta to world space delta accounting for zoom.
    ///
    /// # Arguments
    /// * `pixel_delta` - Delta in screen pixels (from InputAction::PanUpdate)
    /// * `pan_start_offset` - Viewport pan offset when pan started
    /// * `zoom` - Current zoom level
    ///
    /// # Returns
    /// New pan offset to apply
    pub fn apply_pan_delta(pixel_delta: Vec2D, pan_start_offset: Vec2D, zoom: f32) -> Vec2D {
        // Convert pixel delta to world delta
        let world_dx = (pixel_delta.x * Self::LEN_TO_PX) / zoom;
        let world_dy = (pixel_delta.y * Self::LEN_TO_PX) / zoom;

        // Invert delta because panning moves viewport in opposite direction
        let pan_delta = Vec2D::new(-world_dx, world_dy);
        pan_start_offset + pan_delta
    }

    /// Called when window is resized.
    pub fn on_window_resize(&mut self, new_size_px: (f32, f32)) {
        self.window_size_px = new_size_px;
        self.recalculate_viewport_px();
        self.dirty.set(true);
    }

    /// Update world bounds (called when simulation box changes)
    pub fn set_world_bounds(&mut self, world_bounds: Rect) {
        self.world_bounds = world_bounds;
        self.constrain_pan();
        self.dirty.set(true);
    }

    /// Ensure transformation matrices are calculated (called by queries)
    fn ensure_matrices_calculated(&self) {
        if self.dirty.get() {
            self.recalculate_matrices();
        }
    }

    /// Recalculate viewport rectangle in pixels
    fn recalculate_viewport_px(&mut self) {
        self.viewport_px = Rect::new(
            self.margin_px.left,
            self.margin_px.top,
            self.window_size_px.0 - self.margin_px.horizontal(),
            self.window_size_px.1 - self.margin_px.vertical(),
        );
    }

    /// Regenerate cached transformation matrices
    fn recalculate_matrices(&self) {
        // Build world-to-px transform following the same logic as the old CoordinateTransform:
        //
        // Old code did:
        //   world_visible_width = world_size.0 / zoom
        //   world_visible_height = world_size.1 / zoom
        //   vp_x = (world_pos.x - pan.x) / world_visible_width + 0.5
        //   vp_y = (pan.y - world_pos.y) / world_visible_height + 0.5
        //   screen_x = vp_x * viewport_width + margin.0
        //   screen_y = vp_y * viewport_height + margin.1
        //
        // As matrices:
        // 1. Translate by -pan_offset
        // 2. Scale by zoom / world_size * viewport_px (world to normalized viewport to pixels)
        // 3. Translate to viewport center + margin
        // Note: Y is inverted (world Y up, screen Y down)

        let viewport_width = self.viewport_px.w;
        let viewport_height = self.viewport_px.h;

        // World size is stored in world_bounds (note: h may be negative)
        let world_width = self.world_bounds.w.abs();
        let world_height = self.world_bounds.h.abs();

        // Avoid division by zero
        let world_width = if world_width < 1e-6 { 1.0 } else { world_width };
        let world_height = if world_height < 1e-6 {
            1.0
        } else {
            world_height
        };

        // Step 1: Translate by -pan_offset (move pan center to origin)
        let pan_matrix = Mat3::translation(-self.pan_offset.x, -self.pan_offset.y);

        // Step 2: Scale from world to pixels
        // scale_x = viewport_width * zoom / world_width
        // scale_y = -viewport_height * zoom / world_height (negative for Y flip)
        let scale_x = viewport_width * self.zoom / world_width;
        let scale_y = -viewport_height * self.zoom / world_height;
        let scale_matrix = Mat3::scale(scale_x, scale_y);

        // Step 3: Translate to viewport center (which is at margin + viewport/2)
        let viewport_center_x = self.margin_px.left + viewport_width / 2.0;
        let viewport_center_y = self.margin_px.top + viewport_height / 2.0;
        let translate_matrix = Mat3::translation(viewport_center_x, viewport_center_y);

        // Combine: first pan, then scale, then translate
        self.world_to_px_matrix
            .set(translate_matrix * scale_matrix * pan_matrix);
        self.px_to_world_matrix
            .set(self.world_to_px_matrix.get().inverse());

        self.dirty.set(false);
    }

    /// Ensure pan doesn't show too much outside world bounds
    fn constrain_pan(&mut self) {
        let world = &self.world_bounds;

        // Calculate world bounds extents
        let world_left = world.x;
        let world_right = world.x + world.w;
        let world_top = world.y;
        let world_bottom = world.y + world.h;

        // Normalize to handle negative dimensions
        let world_min_x = world_left.min(world_right);
        let world_max_x = world_left.max(world_right);
        let world_min_y = world_top.min(world_bottom);
        let world_max_y = world_top.max(world_bottom);

        let world_width = world_max_x - world_min_x;
        let world_height = world_max_y - world_min_y;

        // At zoom level z, visible area has size = world_size / z
        let visible_half_width = world_width / (2.0 * self.zoom);
        let visible_half_height = world_height / (2.0 * self.zoom);

        // Pan offset must keep viewport within world bounds
        let min_pan_x = world_min_x + visible_half_width;
        let max_pan_x = world_max_x - visible_half_width;
        let min_pan_y = world_min_y + visible_half_height;
        let max_pan_y = world_max_y - visible_half_height;

        // Clamp pan offset to valid range
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

    /// Check if a circle is visible in the current viewport.
    ///
    /// # Arguments
    /// * `center` - Circle center position in world coordinates
    /// * `radius` - Circle radius in world units
    ///
    /// # Returns
    /// `true` if the circle intersects the visible viewport rectangle
    pub fn is_circle_visible(&self, center: &Vec2D, radius: f32) -> bool {
        let visible_rect = self.visible_world_rect();

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
mod mat3_tests {
    use super::*;

    fn approx_eq_mat(a: Mat3, b: Mat3, eps: f32) -> bool {
        (a.m0 - b.m0).abs() <= eps
            && (a.m1 - b.m1).abs() <= eps
            && (a.m2 - b.m2).abs() <= eps
            && (a.m3 - b.m3).abs() <= eps
            && (a.m4 - b.m4).abs() <= eps
            && (a.m5 - b.m5).abs() <= eps
            && (a.m6 - b.m6).abs() <= eps
            && (a.m7 - b.m7).abs() <= eps
            && (a.m8 - b.m8).abs() <= eps
    }

    #[test]
    fn test_mat3_identity() {
        let i = Mat3::identity();
        let p = i.transform_point2(&[5.0, 3.0]);
        assert!((p.x - 5.0).abs() < 1e-6);
        assert!((p.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_translation() {
        let t = Mat3::translation(2.0, 3.0);
        let p = t.transform_point2(&[1.0, 1.0]);
        assert!((p.x - 3.0).abs() < 1e-6);
        assert!((p.y - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_scale() {
        let s = Mat3::scale(2.0, 3.0);
        let p = s.transform_point2(&[1.0, 1.0]);
        assert!((p.x - 2.0).abs() < 1e-6);
        assert!((p.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_multiply() {
        let t = Mat3::translation(1.0, 2.0);
        let s = Mat3::scale(2.0, 2.0);
        let combined = t * s;

        let p = combined.transform_point2(&[1.0, 1.0]);
        // Scale first: (1, 1) -> (2, 2)
        // Translate: (2, 2) -> (3, 4)
        assert!((p.x - 3.0).abs() < 1e-6);
        assert!((p.y - 4.0).abs() < 1e-6);
    }

    #[test]
    fn test_mat3_inverse() {
        let t = Mat3::translation(5.0, 3.0);
        let inv = t.inverse();
        let p1 = [1.0, 2.0];
        let transformed = t.transform_point2(&p1);
        let back = inv.transform_point2(&[transformed.x, transformed.y]);

        assert!((back.x - p1[0]).abs() < 1e-5);
        assert!((back.y - p1[1]).abs() < 1e-5);
    }
}

#[cfg(test)]
mod viewport_transform_tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn test_viewport_transform_new() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let transform =
            ViewportTransform::new(world, (800.0, 600.0), Margins::symmetric(50.0, 50.0));

        assert!(approx_eq(transform.zoom(), 1.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));

        let viewport_size = transform.viewport_size_px();
        assert!(approx_eq(viewport_size.0, 700.0, 1e-6));
        assert!(approx_eq(viewport_size.1, 500.0, 1e-6));
    }

    #[test]
    fn test_viewport_transform_zoom() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut transform =
            ViewportTransform::new(world, (800.0, 600.0), Margins::symmetric(50.0, 50.0));

        let point = Vec2D::zero();
        transform.zoom_at_point(&point, 2.0);

        assert!(approx_eq(transform.zoom(), 2.0, 1e-6));
    }

    #[test]
    fn test_viewport_transform_coordinates() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let transform = ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

        // At 1x zoom, viewport center (200, 200) should map to world center (0, 0)
        let world_pos = transform.px_to_world(&[200.0, 200.0]);
        println!("px (200, 200) -> world ({}, {})", world_pos.x, world_pos.y);
        assert!(approx_eq(world_pos.x, 0.0, 1e-2));
        assert!(approx_eq(world_pos.y, 0.0, 1e-2));

        // Convert back
        let px_pos = transform.world_to_px(&Vec2D::new(0.0, 0.0));
        println!("world (0, 0) -> px ({}, {})", px_pos[0], px_pos[1]);
        assert!(approx_eq(px_pos[0], 200.0, 1e-1));
        assert!(approx_eq(px_pos[1], 200.0, 1e-1));
    }

    #[test]
    fn test_viewport_transform_window_resize() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut transform =
            ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(50.0, 50.0));

        transform.on_window_resize((800.0, 800.0));

        let viewport_size = transform.viewport_size_px();
        assert!(approx_eq(viewport_size.0, 700.0, 1e-6));
        assert!(approx_eq(viewport_size.1, 700.0, 1e-6));
    }

    #[test]
    fn test_world_to_px_center_at_origin() {
        // World bounds: x=-50 to 50, y=-35 to 35 (centered at origin)
        // Window: 1000x700 with no margins
        // At zoom 1.0, the entire world should fit in the viewport
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // World origin (0, 0) should map to viewport center (500, 350)
        let px = transform.world_to_px(&Vec2D::new(0.0, 0.0));
        assert!(approx_eq(px[0], 500.0, 1e-3));
        assert!(approx_eq(px[1], 350.0, 1e-3));

        // Top-left world corner (-50, 35) should map to top-left viewport (0, 0)
        let px = transform.world_to_px(&Vec2D::new(-50.0, 35.0));
        assert!(approx_eq(px[0], 0.0, 1e-3));
        assert!(approx_eq(px[1], 0.0, 1e-3));

        // Bottom-right world corner (50, -35) should map to bottom-right viewport (1000, 700)
        let px = transform.world_to_px(&Vec2D::new(50.0, -35.0));
        assert!(approx_eq(px[0], 1000.0, 1e-3));
        assert!(approx_eq(px[1], 700.0, 1e-3));
    }

    #[test]
    fn test_px_to_world_center_at_origin() {
        // Same setup as above
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // Viewport center (500, 350) should map to world origin (0, 0)
        let w = transform.px_to_world(&[500.0, 350.0]);
        assert!(approx_eq(w.x, 0.0, 1e-3));
        assert!(approx_eq(w.y, 0.0, 1e-3));

        // Top-left viewport (0, 0) should map to top-left world (-50, 35)
        let w = transform.px_to_world(&[0.0, 0.0]);
        assert!(approx_eq(w.x, -50.0, 1e-3));
        assert!(approx_eq(w.y, 35.0, 1e-3));

        // Bottom-right viewport (1000, 700) should map to bottom-right world (50, -35)
        let w = transform.px_to_world(&[1000.0, 700.0]);
        assert!(approx_eq(w.x, 50.0, 1e-3));
        assert!(approx_eq(w.y, -35.0, 1e-3));
    }

    #[test]
    fn test_roundtrip_world_to_px_to_world() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // Test multiple points for roundtrip consistency
        let test_points = [
            Vec2D::new(0.0, 0.0),
            Vec2D::new(-25.0, 17.5),
            Vec2D::new(25.0, -17.5),
            Vec2D::new(-50.0, 35.0),
            Vec2D::new(50.0, -35.0),
        ];

        for orig in &test_points {
            let px = transform.world_to_px(orig);
            let back = transform.px_to_world(&px);
            assert!(
                approx_eq(back.x, orig.x, 1e-3) && approx_eq(back.y, orig.y, 1e-3),
                "Roundtrip failed for ({}, {}): got ({}, {})",
                orig.x,
                orig.y,
                back.x,
                back.y
            );
        }
    }

    #[test]
    fn test_roundtrip_px_to_world_to_px() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // Test multiple pixel positions for roundtrip consistency
        let test_points = [
            [500.0, 350.0],
            [0.0, 0.0],
            [1000.0, 700.0],
            [250.0, 175.0],
            [750.0, 525.0],
        ];

        for orig in &test_points {
            let w = transform.px_to_world(orig);
            let back = transform.world_to_px(&w);
            assert!(
                approx_eq(back[0], orig[0], 1e-3) && approx_eq(back[1], orig[1], 1e-3),
                "Roundtrip failed for ({}, {}): got ({}, {})",
                orig[0],
                orig[1],
                back[0],
                back[1]
            );
        }
    }

    #[test]
    fn test_world_to_px_with_margins() {
        // World: 100x70, Window: 1000x700, Margins: 100x50
        // Viewport region: 800x600 (starts at 100, 50)
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(100.0, 50.0));

        // World origin should map to viewport center, which is at (100+400, 50+300) = (500, 350)
        let px = transform.world_to_px(&Vec2D::new(0.0, 0.0));
        assert!(approx_eq(px[0], 500.0, 1e-3));
        assert!(approx_eq(px[1], 350.0, 1e-3));

        // Top-left world corner should map to top-left of viewport region (100, 50)
        let px = transform.world_to_px(&Vec2D::new(-50.0, 35.0));
        assert!(approx_eq(px[0], 100.0, 1e-3));
        assert!(approx_eq(px[1], 50.0, 1e-3));

        // Bottom-right world corner should map to bottom-right of viewport region (900, 650)
        let px = transform.world_to_px(&Vec2D::new(50.0, -35.0));
        assert!(approx_eq(px[0], 900.0, 1e-3));
        assert!(approx_eq(px[1], 650.0, 1e-3));
    }

    #[test]
    fn test_zoom_at_center() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let mut transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // Zoom 2x at center (0, 0)
        transform.zoom_at_point(&Vec2D::new(0.0, 0.0), 2.0);
        assert!(approx_eq(transform.zoom(), 2.0, 1e-6));

        // World origin should still map to viewport center
        let px = transform.world_to_px(&Vec2D::new(0.0, 0.0));
        assert!(approx_eq(px[0], 500.0, 1e-3));
        assert!(approx_eq(px[1], 350.0, 1e-3));

        // At 2x zoom, only half the world is visible
        // World point at (25, -17.5) should now map to bottom-right (1000, 700)
        let px = transform.world_to_px(&Vec2D::new(25.0, -17.5));
        assert!(approx_eq(px[0], 1000.0, 1e-3));
        assert!(approx_eq(px[1], 700.0, 1e-3));
    }

    #[test]
    fn test_zoom_at_off_center_point() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let mut transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // Zoom 2x at point (25, 17.5) - top-right quadrant
        let zoom_point = Vec2D::new(25.0, 17.5);
        transform.zoom_at_point(&zoom_point, 2.0);

        // The zoom point should remain at the same pixel position
        // At zoom 1.0, (25, 17.5) maps to (750, 175)
        let px = transform.world_to_px(&zoom_point);
        assert!(approx_eq(px[0], 750.0, 1.0)); // Allow some tolerance
        assert!(approx_eq(px[1], 175.0, 1.0));
    }

    #[test]
    fn test_pan() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let mut transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // At zoom 1.0, pan is constrained to (0, 0) because the entire world is visible
        // We need to zoom in first to allow panning
        transform.zoom_at_point(&Vec2D::zero(), 2.0);

        // Now pan to (10, 5) - should be allowed at zoom 2.0
        transform.pan(Vec2D::new(10.0, 5.0));

        // Verify pan offset (may be clamped slightly by constraints)
        let pan = transform.pan_offset();
        assert!(approx_eq(pan.x, 10.0, 1e-3));
        assert!(approx_eq(pan.y, 5.0, 1e-3));

        // The viewport center (500, 350) should show world point (10, 5)
        let w = transform.px_to_world(&[500.0, 350.0]);
        assert!(approx_eq(w.x, 10.0, 1e-3));
        assert!(approx_eq(w.y, 5.0, 1e-3));

        // And world point (10, 5) should be at viewport center
        let px = transform.world_to_px(&Vec2D::new(10.0, 5.0));
        assert!(approx_eq(px[0], 500.0, 1e-3));
        assert!(approx_eq(px[1], 350.0, 1e-3));
    }

    #[test]
    fn test_visible_world_rect_no_zoom() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        let visible = transform.visible_world_rect();
        // At zoom 1.0, entire world should be visible
        assert!(approx_eq(visible.w, 100.0, 1e-3));
        assert!(approx_eq(visible.h.abs(), 70.0, 1e-3));
    }

    #[test]
    fn test_visible_world_rect_with_zoom() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let mut transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        transform.zoom_at_point(&Vec2D::zero(), 2.0);

        let visible = transform.visible_world_rect();
        // At zoom 2.0, half the world should be visible
        assert!(approx_eq(visible.w, 50.0, 1e-3));
        assert!(approx_eq(visible.h.abs(), 35.0, 1e-3));
    }

    #[test]
    fn test_is_circle_visible() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // Circle at center should be visible
        assert!(transform.is_circle_visible(&Vec2D::zero(), 5.0));

        // Circle at edge should be visible
        assert!(transform.is_circle_visible(&Vec2D::new(45.0, 0.0), 5.0));

        // Circle just outside should be visible (overlaps edge)
        assert!(transform.is_circle_visible(&Vec2D::new(52.0, 0.0), 5.0));

        // Circle far outside should not be visible
        assert!(!transform.is_circle_visible(&Vec2D::new(100.0, 0.0), 5.0));
    }

    #[test]
    fn test_reset() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let mut transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // Apply some transforms
        transform.zoom_at_point(&Vec2D::zero(), 3.0);
        transform.pan(Vec2D::new(10.0, 5.0));

        assert!(approx_eq(transform.zoom(), 3.0, 1e-6));

        // Reset
        transform.reset();

        assert!(approx_eq(transform.zoom(), 1.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));
    }

    #[test]
    fn test_adjust_pan() {
        let world = Rect::new(-50.0, 35.0, 100.0, -70.0);
        let mut transform =
            ViewportTransform::new(world, (1000.0, 700.0), Margins::symmetric(0.0, 0.0));

        // At zoom 1.0, pan is constrained to (0, 0) - can't pan when entire world is visible
        // Zoom in first to allow panning
        transform.zoom_at_point(&Vec2D::zero(), 4.0);

        // Start at origin (after zoom, pan is still 0)
        assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));

        // Adjust pan by (5, 3)
        transform.adjust_pan(&Vec2D::new(5.0, 3.0));

        assert!(approx_eq(transform.pan_offset().x, 5.0, 1e-3));
        assert!(approx_eq(transform.pan_offset().y, 3.0, 1e-3));

        // Adjust again
        transform.adjust_pan(&Vec2D::new(-2.0, 1.0));

        assert!(approx_eq(transform.pan_offset().x, 3.0, 1e-3));
        assert!(approx_eq(transform.pan_offset().y, 4.0, 1e-3));
    }

    #[test]
    fn test_compute_world_bounds_from_window_size() {
        // Window 1000x700 with margins 100x50 -> viewport 800x600
        // At LEN_TO_PX = 0.1, world size = 80x60
        let world = ViewportTransform::compute_world_bounds_from_window_size(
            1000,
            700,
            Margins::symmetric(100.0, 50.0),
        );

        assert!(approx_eq(world.w, 80.0, 1e-3));
        assert!(approx_eq(world.h.abs(), 60.0, 1e-3));
        // Centered at origin
        assert!(approx_eq(world.x, -40.0, 1e-3));
        assert!(approx_eq(world.y, 30.0, 1e-3));
    }

    #[test]
    fn test_zoom_limits() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut transform =
            ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

        // Try to zoom out below minimum - should clamp to MIN_ZOOM
        let point = Vec2D::zero();
        transform.zoom_at_point(&point, 0.5);
        assert!(approx_eq(
            transform.zoom(),
            ViewportTransform::MIN_ZOOM,
            1e-6
        ));

        // Zoom in to maximum
        transform.zoom_at_point(&point, 200.0);
        assert!(approx_eq(
            transform.zoom(),
            ViewportTransform::MAX_ZOOM,
            1e-6
        ));

        // Try to zoom beyond maximum - should return false and not change
        let changed = transform.zoom_at_point(&point, 2.0);
        assert!(!changed);
        assert!(approx_eq(
            transform.zoom(),
            ViewportTransform::MAX_ZOOM,
            1e-6
        ));
    }

    #[test]
    fn test_pan_constraints_at_zoom_1() {
        // World bounds: x=-10 to 10, y=-10 to 10 (note: height is negative in rect)
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut transform =
            ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

        // At 1x zoom, the visible area is the same size as the world
        // So pan should be constrained to stay at world center (0, 0)
        transform.pan(Vec2D::new(1000.0, 1000.0));
        assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));

        // Try to pan in negative direction at 1x zoom
        transform.pan(Vec2D::new(-1000.0, -1000.0));
        assert!(approx_eq(transform.pan_offset().x, 0.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().y, 0.0, 1e-6));
    }

    #[test]
    fn test_pan_constraints_at_zoom_2() {
        let world = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let mut transform =
            ViewportTransform::new(world, (400.0, 400.0), Margins::symmetric(0.0, 0.0));

        // At 2x zoom, the visible area is half the world size
        // So we can pan up to ±5 in each direction
        transform.zoom_at_point(&Vec2D::zero(), 2.0);

        transform.pan(Vec2D::new(1000.0, 1000.0));
        assert!(approx_eq(transform.pan_offset().x, 5.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().y, 5.0, 1e-6));

        transform.pan(Vec2D::new(-1000.0, -1000.0));
        assert!(approx_eq(transform.pan_offset().x, -5.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().y, -5.0, 1e-6));

        // Pan to a position within bounds at 2x zoom should work
        transform.pan(Vec2D::new(3.0, -2.0));
        assert!(approx_eq(transform.pan_offset().x, 3.0, 1e-6));
        assert!(approx_eq(transform.pan_offset().y, -2.0, 1e-6));
    }

    #[test]
    fn test_apply_pan_delta() {
        // Test the static method used by event loops to convert pixel deltas to world pans
        let start_offset = Vec2D::new(0.0, 0.0);
        let zoom = 1.0;

        // Move 100 pixels right and 50 pixels down
        // LEN_TO_PX = 0.1, so 100 pixels = 10 world units
        // Pan direction is inverted, so moving right in pixels moves viewport left in world
        let pixel_delta = Vec2D::new(100.0, 50.0);
        let new_pan = ViewportTransform::apply_pan_delta(pixel_delta, start_offset, zoom);

        // world_dx = 100 * 0.1 / 1.0 = 10, inverted = -10
        // world_dy = 50 * 0.1 / 1.0 = 5, inverted with Y flip = +5
        assert!(approx_eq(new_pan.x, -10.0, 1e-6));
        assert!(approx_eq(new_pan.y, 5.0, 1e-6));
    }

    #[test]
    fn test_apply_pan_delta_with_zoom() {
        // At zoom 2.0, pixel movements should result in half the world movement
        let start_offset = Vec2D::new(5.0, 3.0);
        let zoom = 2.0;

        let pixel_delta = Vec2D::new(100.0, 50.0);
        let new_pan = ViewportTransform::apply_pan_delta(pixel_delta, start_offset, zoom);

        // world_dx = 100 * 0.1 / 2.0 = 5, inverted = -5
        // world_dy = 50 * 0.1 / 2.0 = 2.5, Y flip = +2.5
        // Add to start offset: 5 + (-5) = 0, 3 + 2.5 = 5.5
        assert!(approx_eq(new_pan.x, 0.0, 1e-6));
        assert!(approx_eq(new_pan.y, 5.5, 1e-6));
    }
}
