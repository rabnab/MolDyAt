//! Shared operator lifecycle: clear and rescale.
//!
//! Uses the `OperatorCollection` trait so each registered collection
//! is handled uniformly.  Adding a new operator type means adding it
//! to `operator_collections_mut()` — nothing else here changes.

use super::MainContent;
use crate::objects2d::Rect;
use crate::objects2d::operator::OperatorCollection;

impl MainContent {
    /// Return mutable references to every operator collection.
    ///
    /// New operator types extend this list.
    fn operator_collections_mut(&mut self) -> [&mut dyn OperatorCollection; 4] {
        [
            &mut self.line_segments,
            &mut self.valve_segments,
            &mut self.source_spheres,
            &mut self.thermostats,
        ]
    }

    /// Clear all placed operators (called on simulation reset).
    pub(crate) fn clear_all_operators(&mut self) {
        for col in self.operator_collections_mut() {
            col.clear();
        }
    }

    /// Rescale all operator positions when the world region changes size.
    pub(crate) fn rescale_all_operators(&mut self, old_rect: &Rect, new_rect: &Rect) {
        for col in self.operator_collections_mut() {
            col.rescale(old_rect, new_rect);
        }
    }
}
