// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::Cell;
use thermo_physics::objects2d::{Rect, Vec2D};

use super::margins::Margins;
use super::mat3::Mat3;

#[cfg(test)]
mod tests;

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
    /// Relative aspect-ratio difference between world and drawing area below
    /// which the world fills the drawing area (no letterbox bars).
    pub const LETTERBOX_TOLERANCE: f32 = 0.005;

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
        Self::compute_world_bounds_at_density(window_width, window_height, margin, Self::LEN_TO_PX)
    }

    /// Like [`Self::compute_world_bounds_from_window_size`], at
    /// `world_per_px` world units per pixel instead of [`Self::LEN_TO_PX`].
    /// A resize passes the current [`Self::world_per_px`], so the world keeps
    /// the scale it is shown at (a `--world-scale` start, a loaded save).
    pub fn compute_world_bounds_at_density(
        window_width: u32,
        window_height: u32,
        margin: Margins,
        world_per_px: f32,
    ) -> Rect {
        let viewport_width = (window_width as f32 - margin.horizontal()).max(1.0);
        let viewport_height = (window_height as f32 - margin.vertical()).max(1.0);

        let world_width = viewport_width * world_per_px;
        let world_height = viewport_height * world_per_px;

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
    pub fn apply_pan_delta(&self, pixel_delta: Vec2D, pan_start_offset: Vec2D) -> Vec2D {
        // Convert pixel delta to world delta with the scale actually drawn
        // (a letterboxed loaded world is not at `LEN_TO_PX`).
        let world_per_px = self.world_per_px() / self.zoom;
        let world_dx = pixel_delta.x * world_per_px;
        let world_dy = pixel_delta.y * world_per_px;

        // Invert delta because panning moves viewport in opposite direction
        let pan_delta = Vec2D::new(-world_dx, world_dy);
        pan_start_offset + pan_delta
    }

    /// World units per pixel at zoom 1: the same on both axes, since the
    /// world is letterboxed rather than stretched. `LEN_TO_PX` for a world
    /// derived from the window size.
    pub fn world_per_px(&self) -> f32 {
        if self.viewport_px.w > 0.0 {
            self.world_bounds.w.abs() / self.viewport_px.w
        } else {
            Self::LEN_TO_PX
        }
    }

    /// Called when window is resized.
    pub fn on_window_resize(&mut self, new_size_px: (f32, f32)) {
        self.window_size_px = new_size_px;
        self.recalculate_viewport_px();
        self.dirty.set(true);
    }

    /// Update viewport margins in pixels.
    pub fn set_margins(&mut self, margin_px: Margins) {
        self.margin_px = margin_px;
        self.recalculate_viewport_px();
        self.dirty.set(true);
    }

    /// Update world bounds (called when simulation box changes)
    pub fn set_world_bounds(&mut self, world_bounds: Rect) {
        self.world_bounds = world_bounds;
        self.recalculate_viewport_px();
        self.constrain_pan();
        self.dirty.set(true);
    }

    /// Ensure transformation matrices are calculated (called by queries)
    fn ensure_matrices_calculated(&self) {
        if self.dirty.get() {
            self.recalculate_matrices();
        }
    }

    /// Recalculate viewport rectangle in pixels: the drawing area (window
    /// minus margins), narrowed to the world's aspect ratio and centred in
    /// it, so the world is never stretched. A world of the drawing area's
    /// shape (every world derived from the window size) fills it exactly;
    /// only a loaded world of another shape gets bars at two sides.
    fn recalculate_viewport_px(&mut self) {
        let (area_w, area_h) = (
            self.window_size_px.0 - self.margin_px.horizontal(),
            self.window_size_px.1 - self.margin_px.vertical(),
        );
        let world_w = self.world_bounds.w.abs();
        let world_h = self.world_bounds.h.abs();
        let (mut w, mut h) = (area_w, area_h);
        if area_w > 0.0 && area_h > 0.0 && world_w > 1e-6 && world_h > 1e-6 {
            let world_aspect = world_w / world_h;
            let area_aspect = area_w / area_h;
            if (world_aspect / area_aspect - 1.0).abs() >= Self::LETTERBOX_TOLERANCE {
                if area_aspect > world_aspect {
                    w = area_h * world_aspect;
                } else {
                    h = area_w / world_aspect;
                }
            }
        }
        self.viewport_px = Rect::new(
            self.margin_px.left + (area_w - w) / 2.0,
            self.margin_px.top + (area_h - h) / 2.0,
            w,
            h,
        );
    }

    /// Margins around [`Self::viewport_px`]: the configured margins plus any
    /// letterbox bars. Everything drawn into the simulation area uses these;
    /// UI placed in the margins (sidebar) keeps [`Self::margins`].
    pub fn inset_margins(&self) -> Margins {
        let vp = self.viewport_px;
        Margins::new(
            vp.x,
            vp.y,
            (self.window_size_px.0 - vp.x - vp.w).max(0.0),
            (self.window_size_px.1 - vp.y - vp.h).max(0.0),
        )
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

        // Step 3: Translate to viewport center (letterboxed viewport rect)
        let viewport_center_x = self.viewport_px.x + viewport_width / 2.0;
        let viewport_center_y = self.viewport_px.y + viewport_height / 2.0;
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
