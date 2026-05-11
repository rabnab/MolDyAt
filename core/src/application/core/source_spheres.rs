//! Source sphere operator — placement, toggle, and ball emission.
//!
//! A source sphere is a circular emitter that spawns new balls at its
//! perimeter whenever its state is [`OperatorState::On`].  Balls are
//! emitted every [`emit_period`](crate::objects2d::SimSourceSphere::emit_period)
//! simulation steps.

use super::{MainContent, SimulationWorld};
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::{SOURCE_SPHERE_RADIUS, Vec2D};
use rand::RngExt;

/// Parameters collected from a source sphere for one emission tick.
struct EmissionParams {
    center: Vec2D,
    ball_size_idx: usize,
    emission_speed: f32,
}

impl MainContent {
    /// Place a new source sphere at `center`.
    pub(crate) fn add_source_sphere(&mut self, center: Vec2D) {
        self.world.add_source_sphere(center);
    }

    /// Toggle the on/off state of an existing source sphere.
    pub(crate) fn toggle_source_sphere(&mut self, sphere_id: usize) {
        self.world.toggle_source_sphere(sphere_id);
    }

    /// Cycle the emitted ball size for a source sphere.
    pub(crate) fn cycle_source_sphere_ball_size(&mut self, sphere_id: usize) {
        self.world.cycle_source_sphere_ball_size(sphere_id);
    }

    /// Adjust the emission speed of a source sphere.
    pub(crate) fn adjust_source_sphere_speed(&mut self, sphere_id: usize, delta: f32) {
        self.world.adjust_source_sphere_speed(sphere_id, delta);
    }

    /// Adjust the emission period of a source sphere.
    pub(crate) fn adjust_source_sphere_emit_period(&mut self, sphere_id: usize, delta_steps: i32) {
        self.world
            .adjust_source_sphere_emit_period(sphere_id, delta_steps);
    }
}

impl SimulationWorld {
    pub(crate) fn add_source_sphere(&mut self, center: Vec2D) {
        self.source_spheres.add(center);
    }

    pub(crate) fn toggle_source_sphere(&mut self, sphere_id: usize) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.state.toggle();
        }
    }

    pub(crate) fn cycle_source_sphere_ball_size(&mut self, sphere_id: usize) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.cycle_ball_size();
        }
    }

    pub(crate) fn adjust_source_sphere_speed(&mut self, sphere_id: usize, delta: f32) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.adjust_emission_speed(delta);
        }
    }

    pub(crate) fn adjust_source_sphere_emit_period(&mut self, sphere_id: usize, delta_steps: i32) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.adjust_emit_period(delta_steps);
        }
    }

    pub(crate) fn emit_from_source_spheres(&mut self, ball_factory: &BallFactory) {
        let step = self.step_count;
        let emissions = self.collect_source_emissions(step);

        let mut rng = rand::rng();
        for params in emissions {
            let angle: f32 = rng.random_range(0.0..std::f32::consts::TAU);
            let velocity = Vec2D::new(
                params.emission_speed * angle.cos(),
                params.emission_speed * angle.sin(),
            );
            let emit_pos = Vec2D::new(
                params.center.x + SOURCE_SPHERE_RADIUS * angle.cos(),
                params.center.y + SOURCE_SPHERE_RADIUS * angle.sin(),
            );

            let candidate = ball_factory.create_with_size_and_velocity(
                emit_pos,
                params.ball_size_idx,
                velocity,
            );

            let _ = self.add_candidate_ball(candidate, ball_factory.get_new_ball_radius());
        }
    }

    fn collect_source_emissions(&self, step: usize) -> Vec<EmissionParams> {
        self.source_spheres
            .spheres()
            .iter()
            .filter(|s| s.is_emitting() && step.is_multiple_of(s.emit_period))
            .map(|s| EmissionParams {
                center: s.center,
                ball_size_idx: s.ball_size_idx,
                emission_speed: s.emission_speed,
            })
            .collect()
    }
}
