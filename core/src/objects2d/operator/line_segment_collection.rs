//! Line-segment operator collection.

use super::{OperatorCollection, OperatorIdCounter, is_degenerate, remap_point};
use crate::objects2d::{Rect, SimLineSegment, Vec2D};

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
