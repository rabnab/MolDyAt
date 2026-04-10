//! Source sphere operator tool — placing and toggling source spheres.
//!
//! Extends [`UnifiedRenderState`] with the source-sphere interaction
//! state machine.  Click on an empty world position to place a new
//! source sphere; click on an existing one to toggle it.

use crate::objects2d::Vec2D;

use super::unified_render_state::UnifiedRenderState;

/// How close the cursor must be to a sphere centre (in world units) to
/// register a hit for toggle/interaction.
const SPHERE_HIT_RADIUS: f32 = 1.5;
/// Default radius for newly placed source spheres (in world units).
pub const DEFAULT_SOURCE_RADIUS: f32 = 1.2;

impl UnifiedRenderState {
    /// Find a placed source sphere whose centre is within
    /// [`SPHERE_HIT_RADIUS`] of `pos`.
    ///
    /// Returns the sphere's `id` if a hit is found.
    pub(super) fn find_source_sphere_at(&self, pos: Vec2D) -> Option<usize> {
        let snapshot = self.last_snapshot.as_ref()?;
        let threshold_sq = SPHERE_HIT_RADIUS * SPHERE_HIT_RADIUS;
        snapshot
            .operators
            .source_spheres
            .iter()
            .find(|s| pos.distance_squared(&s.center) <= threshold_sq)
            .map(|s| s.id)
    }
}
