//! Source sphere operator — placement, toggle, and ball emission.
//!
//! A source sphere is a circular emitter that spawns new balls at its
//! perimeter whenever its state is [`OperatorState::On`].  Balls are
//! emitted every [`emit_period`](crate::objects2d::SimSourceSphere::emit_period)
//! simulation steps.

use super::MainContent;
use crate::{graphics::renderer::event_loop::DEFAULT_SOURCE_RADIUS, objects2d::Vec2D};
use rand::RngExt;

/// Parameters collected from a source sphere for one emission tick.
struct EmissionParams {
    center: Vec2D,
    ball_size_idx: usize,
    emission_speed: f32,
}

impl MainContent {
    /// Place a new source sphere at `center` with `radius`.
    pub(crate) fn add_source_sphere(&mut self, center: Vec2D) {
        self.source_spheres.add(center);
    }

    /// Toggle the on/off state of an existing source sphere.
    pub(crate) fn toggle_source_sphere(&mut self, sphere_id: usize) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.state.toggle();
        }
    }

    /// Cycle the emitted ball size for a source sphere.
    pub(crate) fn cycle_source_sphere_ball_size(&mut self, sphere_id: usize) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.cycle_ball_size();
        }
    }

    /// Adjust the emission speed of a source sphere.
    pub(crate) fn adjust_source_sphere_speed(&mut self, sphere_id: usize, delta: f32) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.adjust_emission_speed(delta);
        }
    }

    /// Adjust the emission period of a source sphere.
    pub(crate) fn adjust_source_sphere_emit_period(&mut self, sphere_id: usize, delta_steps: i32) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.adjust_emit_period(delta_steps);
        }
    }

    /// Emit balls from all active source spheres.
    ///
    /// Should be called once per simulation step (from `update_sim`).
    /// Each sphere emits at most one ball per call, gated by its
    /// [`emit_period`](crate::objects2d::SimSourceSphere::emit_period).
    /// Emitted balls have a random direction and the sphere's configured
    /// speed magnitude and ball size class.
    pub(crate) fn emit_from_source_spheres(&mut self) {
        let step = self.step_count as u32;

        // Collect emission parameters to avoid borrowing self twice.
        let emissions: Vec<EmissionParams> = self
            .source_spheres
            .spheres()
            .iter()
            .filter(|s| s.is_emitting() && step.is_multiple_of(s.emit_period))
            .map(|s| EmissionParams {
                center: s.center,
                ball_size_idx: s.ball_size_idx,
                emission_speed: s.emission_speed,
            })
            .collect();

        let mut rng = rand::rng();
        for params in emissions {
            let angle: f32 = rng.random_range(0.0..std::f32::consts::TAU);
            let velocity = Vec2D::new(
                params.emission_speed * angle.cos(),
                params.emission_speed * angle.sin(),
            );
            let emit_pos = Vec2D::new(
                params.center.x + DEFAULT_SOURCE_RADIUS * angle.cos(),
                params.center.y + DEFAULT_SOURCE_RADIUS * angle.sin(),
            );

            let candidate = self.get_ball_factory().create_with_size_and_velocity(
                emit_pos,
                params.ball_size_idx,
                velocity,
            );
            // Ignore errors — if the position is occupied or out of bounds, skip.
            let _ = self.add_candidate_ball(candidate, 0.);
        }
    }
}
