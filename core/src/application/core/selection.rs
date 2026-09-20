//! Object hover/selection, and the prepared (not yet implemented) deletion
//! hook. See `objects2d::wall_chain::{ObjectRef, Selection}` for the general
//! concept: one flat reference type covering wall pieces, valves, source
//! spheres, and thermostats, so every tool and the renderer share one source
//! of truth instead of each tool inventing its own hover tracking.

use crate::application::MainContent;
use crate::application::core::SimulationWorld;
use crate::objects2d::wall_chain::ObjectRef;

impl MainContent {
    pub(crate) fn set_hovered(&mut self, obj: Option<ObjectRef>) {
        self.world.set_hovered(obj);
    }

    pub(crate) fn set_selected(&mut self, obj: Option<ObjectRef>) {
        self.world.set_selected(obj);
    }

    /// Delete whatever object is currently selected. NOT YET IMPLEMENTED --
    /// see `SimulationWorld::delete_selected`.
    pub(crate) fn delete_selected(&mut self) {
        self.world.delete_selected();
    }
}

impl SimulationWorld {
    pub(crate) fn set_hovered(&mut self, obj: Option<ObjectRef>) {
        self.selection.hovered = obj;
    }

    pub(crate) fn set_selected(&mut self, obj: Option<ObjectRef>) {
        self.selection.selected = obj;
    }

    /// Delete the currently selected object, if any and if deletion is
    /// implemented for its kind. This is the prepared hook, not a working
    /// implementation -- every arm below is a TODO.
    ///
    /// The one piece of behavior worth deciding now, since it constrains the
    /// data model: deleting a VALVE should turn its `Joint::Valve` into a
    /// `Joint::Gap { t_start, t_end }` on the owning chain (same fractions,
    /// so the chain keeps stretching correctly around the hole) and remove
    /// the `ValveSegment` record -- NOT touch the neighbouring pieces' own
    /// t_start/t_end, and NOT attempt to merge them back into one piece
    /// (per the "deleting a valve should leave a gap" decision).
    pub(crate) fn delete_selected(&mut self) {
        let Some(target) = self.selection.selected else {
            return;
        };

        match target {
            ObjectRef::Valve(_valve_id) => {
                // TODO:
                // 1. self.wall_chains: find the chain containing a joint
                //    with this valve_id (needs a
                //    WallChainCollection::find_containing_valve, not yet
                //    written -- mirrors find_containing_piece).
                // 2. Replace that Joint::Valve with
                //    Joint::Gap { t_start, t_end } (same fractions).
                // 3. self.valve_segments: remove the ValveSegment record
                //    (needs ValveCollection::remove, not yet confirmed to
                //    exist).
                // 4. self.selection.selected = None.
            }
            ObjectRef::WallPiece(_line_id) => {
                // TODO: deleting a whole wall (not just one piece) needs a
                // decision this conversation hasn't reached yet: can a
                // single piece of an already-split chain be deleted
                // independently, leaving the chain's other pieces
                // orphaned/disconnected? Or does deleting any piece delete
                // the whole chain (every piece + every valve on it)?
            }
            ObjectRef::SourceSphere(_id) => {
                // TODO
            }
            ObjectRef::Thermostat(_id) => {
                // TODO
            }
        }
    }
}
