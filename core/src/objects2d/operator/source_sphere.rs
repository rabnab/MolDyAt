//! Source sphere operator — data model, defaults, and collection.

use super::{
    OperatorCollection, OperatorIdCounter, OperatorState, Rect, Vec2D, is_degenerate, remap_point,
};

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
    pub emit_period: u32,
    /// Ball size class index (0–4) for emitted balls.
    pub ball_size_idx: usize,
    /// Speed magnitude for emitted balls (world units per step).
    pub emission_speed: f32,
}

/// Default ball size class index for new source spheres (medium).
const DEFAULT_BALL_SIZE_IDX: usize = 2;
/// Default emission speed magnitude for new source spheres.
const DEFAULT_EMISSION_SPEED: f32 = 0.05;
/// Default emission period for new source spheres (in simulation steps).
const DEFAULT_EMIT_PERIOD: u32 = 3000;
/// Minimum emission period in simulation steps.
const MIN_EMIT_PERIOD: u32 = 5;
/// Maximum emission period in simulation steps.
const MAX_EMIT_PERIOD: u32 = 30000;
/// Step size for emission speed adjustments via scroll wheel.
const EMISSION_SPEED_STEP: f32 = 0.005;
/// Minimum emission speed magnitude.
const MIN_EMISSION_SPEED: f32 = 0.0;
/// Maximum emission speed magnitude.
const MAX_EMISSION_SPEED: f32 = 5.0;

impl SimSourceSphere {
    /// Create a new source sphere starting in the `Off` (dormant) state.
    pub fn new(id: usize, center: Vec2D) -> Self {
        Self {
            id,
            center,
            state: OperatorState::Off,
            emit_period: DEFAULT_EMIT_PERIOD,
            ball_size_idx: DEFAULT_BALL_SIZE_IDX,
            emission_speed: DEFAULT_EMISSION_SPEED,
        }
    }

    /// Whether this sphere is currently emitting.
    pub fn is_emitting(&self) -> bool {
        self.state.is_on()
    }

    /// Cycle to the next ball size class, wrapping around.
    pub fn cycle_ball_size(&mut self) {
        self.ball_size_idx = (self.ball_size_idx + 1) % crate::objects2d::ball_impl::NUM_BALL_SIZES;
    }

    /// Adjust emission speed by `delta` (positive = faster), clamped.
    pub fn adjust_emission_speed(&mut self, delta: f32) {
        self.emission_speed = (self.emission_speed + EMISSION_SPEED_STEP * delta.signum())
            .clamp(MIN_EMISSION_SPEED, MAX_EMISSION_SPEED);
    }

    /// Adjust emission period by `delta_steps` (positive = slower), clamped.
    pub fn adjust_emit_period(&mut self, delta_steps: i32) {
        let new_period = (self.emit_period as i32 + delta_steps)
            .clamp(MIN_EMIT_PERIOD as i32, MAX_EMIT_PERIOD as i32);
        self.emit_period = new_period as u32;
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
