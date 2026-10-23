// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Removing a single operator object (the one the user pressed Delete on).

use crate::application::MainContent;
use crate::application::core::SimulationWorld;
use crate::objects2d::wall_chain::{Joint, ObjectRef};

impl MainContent {
    pub fn delete_object(&mut self, target: ObjectRef) {
        self.world.delete_object(target);
    }
}

impl SimulationWorld {
    /// Remove `target` from the world. A stale reference (already deleted,
    /// or cleared since the renderer saw it) is ignored.
    ///
    /// - A wall goes with everything on it: every piece and every valve.
    /// - A valve leaves a permanent `Joint::Gap` in its wall (same chain
    ///   fractions, so the wall still stretches correctly around the hole);
    ///   the pieces on either side are not merged back together.
    pub(crate) fn delete_object(&mut self, target: ObjectRef) {
        match target {
            ObjectRef::Wall(chain_id) => {
                let Some(chain) = self.wall_chains.remove(chain_id) else {
                    return;
                };
                for piece in &chain.pieces {
                    self.line_segments.remove(piece.line_id);
                }
                for valve_id in chain.valve_ids() {
                    self.valve_segments.remove(valve_id);
                }
            }
            ObjectRef::Valve(valve_id) => {
                if let Some((chain, index)) = self.wall_chains.find_valve_joint_mut(valve_id) {
                    let joint = chain.joints[index];
                    chain.joints[index] = Joint::Gap {
                        t_start: joint.t_start(),
                        t_end: joint.t_end(),
                    };
                }
                self.valve_segments.remove(valve_id);
            }
            ObjectRef::SourceSphere(id) => {
                self.source_spheres.remove(id);
            }
            ObjectRef::Thermostat(id) => {
                self.thermostats.remove(id);
            }
        }
    }
}
