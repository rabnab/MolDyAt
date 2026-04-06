//! Shared operator lifecycle: clear and rescale.
//!
//! Centralizes the clear-on-reset and rescale-on-resize logic so each
//! new operator type only needs to register here instead of threading
//! calls through `balls.rs` and `accessors.rs`.

use super::MainContent;
use crate::objects2d::Rect;

impl MainContent {
    /// Clear all placed operators (called on simulation reset).
    ///
    /// New operator types should add their `clear_*()` call here.
    pub(crate) fn clear_all_operators(&mut self) {
        self.clear_line_segments();
        // Future: self.clear_valves();
        // Future: self.clear_sources();
        // Future: self.clear_thermostats();
    }

    /// Rescale all operator positions when the world region changes size.
    ///
    /// New operator types should add their `rescale_*()` call here.
    pub(crate) fn rescale_all_operators(&mut self, old_rect: &Rect, new_rect: &Rect) {
        self.rescale_line_segments(old_rect, new_rect);
        // Future: self.rescale_valves(old_rect, new_rect);
        // Future: self.rescale_sources(old_rect, new_rect);
        // Future: self.rescale_thermostats(old_rect, new_rect);
    }
}
