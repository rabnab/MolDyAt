// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared operator lifecycle: clear and rescale.
//!
//! Uses the `OperatorCollection` trait so each registered collection
//! is handled uniformly.  Adding a new operator type means adding it
//! to `operator_collections_mut()` — nothing else here changes.

use super::{MainContent, SimulationWorld};
use crate::objects2d::Rect;
use crate::objects2d::operator::OperatorCollection;

impl SimulationWorld {
    /// Return mutable references to every operator collection.
    ///
    /// New operator types extend this list.
    fn operator_collections_mut(&mut self) -> [&mut dyn OperatorCollection; 5] {
        [
            &mut self.line_segments,
            &mut self.valve_segments,
            &mut self.wall_chains,
            &mut self.source_spheres,
            &mut self.thermostats,
        ]
    }

    /// Clear all placed operators (called on simulation reset).
    pub(crate) fn clear_all_operators(&mut self) {
        for col in self.operator_collections_mut() {
            col.clear();
        }
        self.ghost_wall = None;
    }

    /// Rescale all operator positions when the world region changes size.
    pub(crate) fn rescale_all_operators(&mut self, old_rect: &Rect, new_rect: &Rect) {
        for col in self.operator_collections_mut() {
            col.rescale(old_rect, new_rect);
        }
        // Stale after a resize; the wall tool sends a fresh one on the next
        // cursor move.
        self.ghost_wall = None;
    }
}

impl MainContent {
    pub(crate) fn clear_all_operators(&mut self) {
        self.world.clear_all_operators();
    }

    pub(crate) fn rescale_all_operators(&mut self, old_rect: &Rect, new_rect: &Rect) {
        self.world.rescale_all_operators(old_rect, new_rect);
    }
}
