//! Common types for simulation operators.
//!
//! All operators placed via the sidebar panel share a lifecycle:
//!
//! 1. **Selection** from the sidebar (see `OperatorTool` in the graphics layer).
//! 2. **Placement** on the simulation grid (operator-specific interaction).
//! 3. **Toggle** ON/OFF (all except line segments).
//! 4. **Rescale** positions when the world region is resized.
//! 5. **Clear** on simulation reset (`ClearBalls`).
//!
//! This module defines shared abstractions so each operator type reuses
//! the same patterns for ID management, toggle state, and position
//! rescaling.

use super::{Rect, SimLineSegment, ValveSegment, Vec2D};

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
}

impl SimSourceSphere {
    /// Create a new source sphere starting in the `On` (emitting) state.
    pub fn new(id: usize, center: Vec2D) -> Self {
        Self {
            id,
            center,
            state: OperatorState::On,
            emit_period: 30,
        }
    }

    /// Whether this sphere is currently emitting.
    pub fn is_emitting(&self) -> bool {
        self.state.is_on()
    }
}

/// ON/OFF state for toggleable operators (all except line segments).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperatorState {
    /// Operator is active — applies its physics effect.
    #[default]
    On,
    /// Operator is inactive — passable / dormant.
    Off,
}

impl OperatorState {
    /// Toggle between On and Off.
    pub fn toggle(&mut self) {
        *self = match *self {
            Self::On => Self::Off,
            Self::Off => Self::On,
        };
    }

    /// Whether the operator is currently active.
    pub fn is_on(self) -> bool {
        self == Self::On
    }

    /// RGBA color for the state indicator.
    ///
    /// - **On** → green circle
    /// - **Off** → red cross
    pub fn indicator_color(self) -> [f32; 4] {
        match self {
            Self::On => [0.0, 1.0, 0.0, 1.0],  // green
            Self::Off => [1.0, 0.0, 0.0, 1.0], // red
        }
    }
}

/// Remap a world-coordinate point from `old_rect` to `new_rect`,
/// preserving its fractional position (0..1) within the rect.
///
/// This is the shared rescaling formula used by every operator when the
/// simulation region is resized.
pub fn remap_point(p: Vec2D, old: &Rect, new: &Rect) -> Vec2D {
    let frac_x = (p.x - old.x) / old.w;
    let frac_y = (p.y - old.y) / old.h;
    Vec2D::new(new.x + frac_x * new.w, new.y + frac_y * new.h)
}

/// Check whether a rect is degenerate (zero-area), used to guard
/// against division-by-zero in [`remap_point`].
pub fn is_degenerate(rect: &Rect) -> bool {
    rect.w.abs() < 1e-12 || rect.h.abs() < 1e-12
}

/// Monotonically increasing ID counter, reset on clear.
///
/// Shared by all operator collections so each type uses the same
/// pattern for assigning and resetting IDs.
#[derive(Debug, Clone, Default)]
pub struct OperatorIdCounter {
    next: usize,
}

impl OperatorIdCounter {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Allocate the next ID and advance the counter.
    pub fn next_id(&mut self) -> usize {
        let id = self.next;
        self.next += 1;
        id
    }

    /// Reset the counter to zero (called on simulation clear).
    pub fn reset(&mut self) {
        self.next = 0;
    }
}

// ── Operator Collection Trait ────────────────────────────────────────

/// Common lifecycle operations shared by all operator storage types.
///
/// Implementations own a `Vec<T>` + an `OperatorIdCounter` for one kind
/// of operator.  `MainContent` calls these methods during clear/rescale
/// so new operator types only extend the collection list — no manual
/// wiring in `operators.rs`.
pub trait OperatorCollection: Send {
    /// Remove all operators and reset the ID counter.
    fn clear(&mut self);

    /// Adjust operator positions for a world resize.
    fn rescale(&mut self, old_rect: &Rect, new_rect: &Rect);
}

// ── Line-Segment Collection ─────────────────────────────────────────

/// Owns the `Vec<SimLineSegment>` and its ID counter.
#[derive(Debug, Clone, Default)]
pub struct LineSegmentCollection {
    segments: Vec<SimLineSegment>,
    next_id: OperatorIdCounter,
}

impl LineSegmentCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new line segment, returning its assigned ID.
    pub fn add(&mut self, start: Vec2D, end: Vec2D) -> usize {
        let id = self.next_id.next_id();
        self.segments.push(SimLineSegment::new(id, start, end));
        id
    }

    /// Immutable access to all line segments.
    pub fn segments(&self) -> &[SimLineSegment] {
        &self.segments
    }

    /// Mutable access to all line segments.
    pub fn segments_mut(&mut self) -> &mut Vec<SimLineSegment> {
        &mut self.segments
    }

    /// Find a segment by ID (immutable).
    pub fn find(&self, id: usize) -> Option<&SimLineSegment> {
        self.segments.iter().find(|s| s.id == id)
    }

    /// Find a segment by ID (mutable).
    pub fn find_mut(&mut self, id: usize) -> Option<&mut SimLineSegment> {
        self.segments.iter_mut().find(|s| s.id == id)
    }
}

impl OperatorCollection for LineSegmentCollection {
    fn clear(&mut self) {
        self.segments.clear();
        self.next_id.reset();
    }

    fn rescale(&mut self, old_rect: &Rect, new_rect: &Rect) {
        if is_degenerate(old_rect) {
            return;
        }
        for seg in &mut self.segments {
            seg.start = remap_point(seg.start, old_rect, new_rect);
            seg.end = remap_point(seg.end, old_rect, new_rect);
        }
    }
}

// ── Valve Collection ────────────────────────────────────────────────

/// Owns the `Vec<ValveSegment>` and its ID counter.
#[derive(Debug, Clone, Default)]
pub struct ValveCollection {
    valves: Vec<ValveSegment>,
    next_id: OperatorIdCounter,
}

impl ValveCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new valve, returning its assigned ID.
    pub fn add(&mut self, line_id: usize, t_start: f32, t_end: f32, is_closed: bool) -> usize {
        let id = self.next_id.next_id();
        self.valves
            .push(ValveSegment::new(id, line_id, t_start, t_end, is_closed));
        id
    }

    /// Immutable access to all valves.
    pub fn valves(&self) -> &[ValveSegment] {
        &self.valves
    }

    /// Find a valve by ID (immutable).
    pub fn find(&self, id: usize) -> Option<&ValveSegment> {
        self.valves.iter().find(|v| v.id == id)
    }

    /// Find a valve by ID (mutable).
    pub fn find_mut(&mut self, id: usize) -> Option<&mut ValveSegment> {
        self.valves.iter_mut().find(|v| v.id == id)
    }

    /// Index of a valve by ID (for indexed access after lookup).
    pub fn position(&self, id: usize) -> Option<usize> {
        self.valves.iter().position(|v| v.id == id)
    }

    /// Direct indexed access (immutable).
    pub fn get(&self, idx: usize) -> &ValveSegment {
        &self.valves[idx]
    }

    /// Direct indexed access (mutable).
    pub fn get_mut(&mut self, idx: usize) -> &mut ValveSegment {
        &mut self.valves[idx]
    }
}

impl OperatorCollection for ValveCollection {
    fn clear(&mut self) {
        self.valves.clear();
        self.next_id.reset();
    }

    fn rescale(&mut self, _old_rect: &Rect, _new_rect: &Rect) {
        // Valve geometry is parametric on line segments (t_start/t_end),
        // so line endpoint rescaling is sufficient.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_state_toggle() {
        let mut s = OperatorState::On;
        assert!(s.is_on());
        s.toggle();
        assert!(!s.is_on());
        s.toggle();
        assert!(s.is_on());
    }

    #[test]
    fn operator_state_default_is_on() {
        let s = OperatorState::default();
        assert!(s.is_on());
    }

    #[test]
    fn indicator_color_green_when_on() {
        let c = OperatorState::On.indicator_color();
        assert!((c[1] - 1.0).abs() < 1e-5); // green channel
        assert!(c[0].abs() < 1e-5); // no red
    }

    #[test]
    fn indicator_color_red_when_off() {
        let c = OperatorState::Off.indicator_color();
        assert!((c[0] - 1.0).abs() < 1e-5); // red channel
        assert!(c[1].abs() < 1e-5); // no green
    }

    #[test]
    fn remap_point_preserves_midpoint() {
        let old = Rect::new(-20.0, 20.0, 40.0, -40.0);
        let new = Rect::new(-10.0, 10.0, 20.0, -20.0);
        let p = remap_point(Vec2D::new(0.0, 0.0), &old, &new);
        assert!((p.x - 0.0).abs() < 1e-5);
        assert!((p.y - 0.0).abs() < 1e-5);
    }

    #[test]
    fn remap_point_preserves_corner() {
        let old = Rect::new(-20.0, 20.0, 40.0, -40.0);
        let new = Rect::new(-40.0, 40.0, 80.0, -80.0);
        let p = remap_point(Vec2D::new(-20.0, 20.0), &old, &new);
        assert!((p.x - (-40.0)).abs() < 1e-5);
        assert!((p.y - 40.0).abs() < 1e-5);
    }

    #[test]
    fn is_degenerate_zero_width() {
        assert!(is_degenerate(&Rect::new(0.0, 0.0, 0.0, 10.0)));
    }

    #[test]
    fn is_degenerate_zero_height() {
        assert!(is_degenerate(&Rect::new(0.0, 0.0, 10.0, 0.0)));
    }

    #[test]
    fn is_degenerate_normal_rect() {
        assert!(!is_degenerate(&Rect::new(-10.0, 10.0, 20.0, -20.0)));
    }

    #[test]
    fn id_counter_increments() {
        let mut c = OperatorIdCounter::new();
        assert_eq!(c.next_id(), 0);
        assert_eq!(c.next_id(), 1);
        assert_eq!(c.next_id(), 2);
    }

    #[test]
    fn id_counter_reset() {
        let mut c = OperatorIdCounter::new();
        c.next_id();
        c.next_id();
        c.reset();
        assert_eq!(c.next_id(), 0);
    }
}
