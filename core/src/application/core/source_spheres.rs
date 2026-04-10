//! Source sphere operator — placement, toggle, and ball emission.
//!
//! A source sphere is a circular emitter that spawns new balls at its
//! perimeter whenever its state is [`OperatorState::On`].  Balls are
//! emitted every [`emit_period`](crate::objects2d::SimSourceSphere::emit_period)
//! simulation steps.

use super::MainContent;
use crate::objects2d::Vec2D;

impl MainContent {
    /// Place a new source sphere at `center` with `radius`.
    pub(crate) fn add_source_sphere(&mut self, center: Vec2D, radius: f32) {
        self.source_spheres.add(center, radius);
    }

    /// Toggle the on/off state of an existing source sphere.
    pub(crate) fn toggle_source_sphere(&mut self, sphere_id: usize) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.state.toggle();
        }
    }

    /// Emit balls from all active source spheres.
    ///
    /// Should be called once per simulation step (from `update_sim`).
    /// Each sphere emits at most one ball per call, gated by its
    /// [`emit_period`](crate::objects2d::SimSourceSphere::emit_period).
    pub(crate) fn emit_from_source_spheres(&mut self) {
        let step = self.step_count as u32;

        // Collect emission parameters to avoid borrowing self twice.
        let emissions: Vec<Vec2D> = self
            .source_spheres
            .spheres()
            .iter()
            .filter(|s| s.is_emitting() && step.is_multiple_of(s.emit_period))
            .map(|s| Vec2D::new(s.center.x, s.center.y + s.radius))
            .collect();

        for emit_pos in emissions {
            // Ignore errors — if the position is occupied or out of bounds, skip.
            let _ = self.add_candidate_at_pos(emit_pos);
        }
    }
}
