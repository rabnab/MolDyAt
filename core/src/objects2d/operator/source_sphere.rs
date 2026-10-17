// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Source sphere operator — data model, defaults, and collection.

use super::{
    OperatorCollection, OperatorIdCounter, OperatorState, Rect, Vec2D, is_degenerate, remap_point,
    remove_by_id,
};
use crate::objects2d::ball_impl::{DEFAULT_SPECIES, SpeciesId, SpeciesTable};

// ── Source Sphere ────────────────────────────────────────────────────

/// A circular emitter placed in the simulation world.
///
/// When its state is [`OperatorState::On`] the sphere spawns new balls
/// every [`emit_period`](SimSourceSphere::emit_period) simulation steps.
/// When [`OperatorState::Off`] it is dormant.
#[derive(Debug, Clone, Copy)]
pub struct SimSourceSphere {
    /// Unique identifier assigned by [`SourceSphereCollection`].
    pub id: usize,
    /// Center of the sphere in world coordinates.
    pub center: Vec2D,
    /// ON/OFF emission state.
    pub state: OperatorState,
    /// Emit one ball every this many simulation steps (when On).
    pub emit_period: usize,
    /// Species of the emitted balls.
    pub ball_species: SpeciesId,
    /// Speed magnitude for emitted balls (world units per step).
    pub emission_speed: f32,
}

/// Shared source sphere radius in world units.
pub const SOURCE_SPHERE_RADIUS: f32 = 1.2;
/// Default emission speed magnitude for new source spheres.
const DEFAULT_EMISSION_SPEED: f32 = 0.05;
/// Default emission period for new source spheres (in simulation steps).
const DEFAULT_EMIT_PERIOD: usize = 3000;
/// Minimum emission period in simulation steps.
pub const MIN_EMIT_PERIOD: usize = 5;
/// Maximum emission period in simulation steps.
pub const MAX_EMIT_PERIOD: usize = 30000;
/// Step size for emission speed adjustments via scroll wheel.
pub const EMISSION_SPEED_STEP: f32 = 0.05;
/// Minimum emission speed magnitude.
pub const MIN_EMISSION_SPEED: f32 = 0.0;
/// Maximum emission speed magnitude.
pub const MAX_EMISSION_SPEED: f32 = 5.0;

impl SimSourceSphere {
    /// Create a new source sphere starting in the `Off` (dormant) state.
    pub fn new(id: usize, center: Vec2D) -> Self {
        Self {
            id,
            center,
            state: OperatorState::Off,
            emit_period: DEFAULT_EMIT_PERIOD,
            ball_species: DEFAULT_SPECIES,
            emission_speed: DEFAULT_EMISSION_SPEED,
        }
    }

    /// Whether this sphere is currently emitting.
    pub fn is_emitting(&self) -> bool {
        self.state.is_on()
    }

    /// Cycle to the next species, wrapping around.
    pub fn cycle_ball_species(&mut self) {
        self.ball_species = SpeciesTable::builtin().next(self.ball_species);
    }

    /// Adjust emission speed by `delta` (positive = faster), clamped.
    pub fn adjust_emission_speed(&mut self, delta: f32) {
        self.emission_speed =
            (self.emission_speed + delta).clamp(MIN_EMISSION_SPEED, MAX_EMISSION_SPEED);
    }

    /// Adjust emission period by `delta_steps` (positive = slower), clamped.
    pub fn adjust_emit_period(&mut self, delta_steps: i32) {
        let new_period = (self.emit_period as i32 + delta_steps)
            .clamp(MIN_EMIT_PERIOD as i32, MAX_EMIT_PERIOD as i32);
        self.emit_period = new_period as usize;
    }

    /// Emission speed as a fraction in `[0, 1]` of the allowed range.
    ///
    /// Uses a logarithmic mapping so that the low end of the speed range
    /// (where most interactive adjustment happens) is visually distinguishable.
    pub fn speed_fraction(&self) -> f32 {
        if self.emission_speed <= MIN_EMISSION_SPEED {
            return 0.0;
        }
        // Map [MIN+ε, MAX] → [0, 1] via log scale.
        // offset by 1.0 to avoid log(0): log(1 + speed) / log(1 + max)
        let log_val = (1.0 + self.emission_speed).ln();
        let log_max = (1.0 + MAX_EMISSION_SPEED).ln();
        (log_val / log_max).clamp(0.0, 1.0)
    }

    /// Emission rate as a fraction in `[0, 1]` of the allowed range.
    ///
    /// Uses a logarithmic mapping so that changes at the fast-emission
    /// end (small period) remain visible.  A higher rate (shorter period)
    /// yields a larger fraction.
    pub fn rate_fraction(&self) -> f32 {
        if MAX_EMIT_PERIOD <= MIN_EMIT_PERIOD {
            return 0.0;
        }
        // Linear position: 0 = fastest (MIN_EMIT_PERIOD), 1 = slowest
        let range = (MAX_EMIT_PERIOD - MIN_EMIT_PERIOD) as f32;
        let linear = (self.emit_period - MIN_EMIT_PERIOD) as f32 / range;
        // Invert (1 = fastest) then apply log for perceptual spread
        let inverted = 1.0 - linear;
        let log_val = (1.0 + inverted * 99.0).log10(); // maps [0,1] → [0, 2]
        (log_val / 2.0_f32).clamp(0.0, 1.0)
    }
}

// ── Source Sphere Collection ─────────────────────────────────────────

/// Owns the `Vec<SimSourceSphere>` and its ID counter.
#[derive(Debug, Clone, Default)]
pub struct SourceSphereCollection {
    spheres: Vec<SimSourceSphere>,
    next_id: OperatorIdCounter,
}

impl SourceSphereCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new source sphere, returning its assigned ID.
    pub fn add(&mut self, center: Vec2D) -> usize {
        let id = self.next_id.next_id();
        self.spheres.push(SimSourceSphere::new(id, center));
        id
    }

    /// Immutable access to all source spheres.
    pub fn spheres(&self) -> &[SimSourceSphere] {
        &self.spheres
    }

    /// Find a sphere by ID (immutable).
    pub fn find(&self, id: usize) -> Option<&SimSourceSphere> {
        self.spheres.iter().find(|s| s.id == id)
    }

    /// Find a sphere by ID (mutable).
    pub fn find_mut(&mut self, id: usize) -> Option<&mut SimSourceSphere> {
        self.spheres.iter_mut().find(|s| s.id == id)
    }

    /// Remove the source sphere with `id`. Returns `false` if there was none.
    pub fn remove(&mut self, id: usize) -> bool {
        remove_by_id(&mut self.spheres, id, |item| item.id)
    }
}

/// A restored collection: ids are kept, new ids continue after the largest.
impl From<Vec<SimSourceSphere>> for SourceSphereCollection {
    fn from(spheres: Vec<SimSourceSphere>) -> Self {
        Self {
            next_id: OperatorIdCounter::after(spheres.iter().map(|item| item.id)),
            spheres,
        }
    }
}

impl OperatorCollection for SourceSphereCollection {
    fn clear(&mut self) {
        self.spheres.clear();
        self.next_id.reset();
    }

    fn rescale(&mut self, old_rect: &Rect, new_rect: &Rect) {
        if is_degenerate(old_rect) {
            return;
        }
        for sphere in &mut self.spheres {
            sphere.center = remap_point(sphere.center, old_rect, new_rect);
        }
    }
}
