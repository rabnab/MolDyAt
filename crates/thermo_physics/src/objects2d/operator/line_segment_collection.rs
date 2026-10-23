// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Line-segment operator collection.

use super::{OperatorCollection, OperatorIdCounter, is_degenerate, remap_point, remove_by_id};
use crate::objects2d::{Rect, SimLineSegment, Vec2D};

/// Owns the `Vec<SimLineSegment>` and its ID counter.
#[derive(Debug, Clone, Default)]
pub struct LineSegmentCollection {
    segments: Vec<SimLineSegment>,
    next_id: OperatorIdCounter,
}

impl LineSegmentCollection {
    /// Add a new line segment, returning its assigned ID.
    pub(crate) fn add(&mut self, start: Vec2D, end: Vec2D) -> usize {
        let id = self.next_id.next_id();
        self.segments.push(SimLineSegment::new(id, start, end));
        id
    }

    /// Immutable access to all line segments.
    pub fn segments(&self) -> &[SimLineSegment] {
        &self.segments
    }

    /// Find a segment by ID (immutable).
    pub(crate) fn find(&self, id: usize) -> Option<&SimLineSegment> {
        self.segments.iter().find(|s| s.id == id)
    }

    /// Find a segment by ID (mutable).
    pub(crate) fn find_mut(&mut self, id: usize) -> Option<&mut SimLineSegment> {
        self.segments.iter_mut().find(|s| s.id == id)
    }

    /// Remove the line segment with `id`. Returns `false` if there was none.
    pub(crate) fn remove(&mut self, id: usize) -> bool {
        remove_by_id(&mut self.segments, id, |item| item.id)
    }
}

/// A restored collection: ids are kept, new ids continue after the largest.
impl From<Vec<SimLineSegment>> for LineSegmentCollection {
    fn from(segments: Vec<SimLineSegment>) -> Self {
        Self {
            next_id: OperatorIdCounter::after(segments.iter().map(|item| item.id)),
            segments,
        }
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
