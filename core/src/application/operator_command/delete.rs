// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deleting any operator object.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::objects2d::wall_chain::ObjectRef;

/// Remove one object (a wall with everything on it, a valve, a source
/// sphere or a thermostat). See `SimulationWorld::delete_object`.
#[derive(Debug)]
pub struct DeleteObject {
    pub target: ObjectRef,
}

impl OperatorCommand for DeleteObject {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.delete_object(self.target);
    }
}
