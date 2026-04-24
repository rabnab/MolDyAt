//! Valve operator collection.

use super::{OperatorCollection, OperatorIdCounter};
use crate::objects2d::{Rect, ValveSegment};

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
