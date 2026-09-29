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

mod line_segment_collection;
mod source_sphere;
mod thermostat;
mod valve_collection;

pub use line_segment_collection::LineSegmentCollection;
pub use source_sphere::{
    EMISSION_SPEED_STEP, SOURCE_SPHERE_RADIUS, SimSourceSphere, SourceSphereCollection,
};
pub use thermostat::{
    DEFAULT_THERMOSTAT_CHAIN_LENGTH, DEFAULT_THERMOSTAT_RADIUS, SimThermostat,
    THERMOSTAT_CHAIN_MAX, THERMOSTAT_CHAIN_MIN, THERMOSTAT_RADIUS_STEP,
    THERMOSTAT_TARGET_TEMP_STEP, ThermostatCollection,
};
pub use valve_collection::ValveCollection;

use super::{Rect, Vec2D};

// ── Operator State ───────────────────────────────────────────────────

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

// ── Shared Helpers ───────────────────────────────────────────────────

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

// ── ID Counter ───────────────────────────────────────────────────────

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
/// Remove the item with `id` from `items`; `false` if there was none.
///
/// Ids are unique while the collection lives, but `clear()` resets the
/// counter to zero, so an id held from before a clear may name a different
/// object afterwards.
pub(crate) fn remove_by_id<T>(items: &mut Vec<T>, id: usize, id_of: fn(&T) -> usize) -> bool {
    let before = items.len();
    items.retain(|item| id_of(item) != id);
    items.len() != before
}

pub trait OperatorCollection: Send {
    /// Remove all operators and reset the ID counter.
    fn clear(&mut self);

    /// Adjust operator positions for a world resize.
    fn rescale(&mut self, old_rect: &Rect, new_rect: &Rect);
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
