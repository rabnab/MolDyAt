// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Source sphere operator — placement, toggle, and ball emission.
//!
//! A source sphere is a circular emitter that spawns new balls at its
//! perimeter whenever its state is [`OperatorState::On`].  Balls are
//! emitted every [`emit_period`](crate::objects2d::SimSourceSphere::emit_period)
//! simulation steps.

use super::{MainContent, SimulationWorld};
use crate::objects2d::ball_impl::{BallFactory, SpeciesId};
use crate::objects2d::{SOURCE_SPHERE_RADIUS, Vec2D};

/// Parameters collected from a source sphere for one emission tick.
struct EmissionParams {
    center: Vec2D,
    ball_species: SpeciesId,
    emission_speed: f32,
}

impl MainContent {
    /// Place a new source sphere at `center`. It emits the species new
    /// balls get (the one selected with `N` / `Alt + N` or right click).
    pub fn add_source_sphere(&mut self, center: Vec2D) {
        let species = self.new_ball_species();
        self.world.add_source_sphere(center, species);
    }

    /// Toggle the on/off state of an existing source sphere.
    pub fn toggle_source_sphere(&mut self, sphere_id: usize) {
        self.world.toggle_source_sphere(sphere_id);
    }

    /// Cycle the species a source sphere emits.
    pub fn cycle_source_sphere_species(&mut self, sphere_id: usize) {
        self.world.cycle_source_sphere_species(sphere_id);
    }

    /// Adjust the emission speed of a source sphere.
    pub fn adjust_source_sphere_speed(&mut self, sphere_id: usize, delta: f32) {
        self.world.adjust_source_sphere_speed(sphere_id, delta);
    }

    /// Adjust the emission period of a source sphere.
    pub fn adjust_source_sphere_emit_period(&mut self, sphere_id: usize, delta_steps: i32) {
        self.world
            .adjust_source_sphere_emit_period(sphere_id, delta_steps);
    }
}

impl SimulationWorld {
    /// Place a source sphere at `center` that emits `species` (of the
    /// world's table).
    pub(crate) fn add_source_sphere(&mut self, center: Vec2D, species: SpeciesId) {
        self.source_spheres.add_with_species(center, species);
    }

    pub(crate) fn toggle_source_sphere(&mut self, sphere_id: usize) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.state.toggle();
        }
    }

    pub(crate) fn cycle_source_sphere_species(&mut self, sphere_id: usize) {
        if let Some(sphere) = self.source_spheres.find_mut(sphere_id) {
            sphere.cycle_ball_species(self.physics.species());
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
        if emissions.is_empty() {
            return;
        }
        let obstacles = self.collect_solid_obstacles();

        for params in emissions {
            // From the factory's RNG, not the thread RNG: a seeded run must
            // emit the same balls every time.
            let angle = ball_factory.random_angle();
            let velocity = Vec2D::new(
                params.emission_speed * angle.cos(),
                params.emission_speed * angle.sin(),
            );
            let emit_pos = Vec2D::new(
                params.center.x + SOURCE_SPHERE_RADIUS * angle.cos(),
                params.center.y + SOURCE_SPHERE_RADIUS * angle.sin(),
            );

            let candidate = ball_factory.create_with_species_and_velocity(
                emit_pos,
                params.ball_species,
                velocity,
            );
            // Margin: the contact radius of the species emitted, not of the
            // species of new balls (the cursor's).
            let margin = self.species().species(params.ball_species).contact_radius();

            let _ = self.add_candidate_ball_among(candidate, margin, &obstacles);
        }
    }

    fn collect_source_emissions(&self, step: usize) -> Vec<EmissionParams> {
        self.source_spheres
            .spheres()
            .iter()
            .filter(|s| s.is_emitting() && step.is_multiple_of(s.emit_period))
            .map(|s| EmissionParams {
                center: s.center,
                ball_species: s.ball_species,
                emission_speed: s.emission_speed,
            })
            .collect()
    }
}
