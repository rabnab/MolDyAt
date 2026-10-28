// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The physics under the paths the moved code uses (`crate::application::X`).
// Private: users of this crate take them from `thermo_physics`.
use thermo_physics::application::{
    MainContent, core, log_util, persistence, sim_stats, thermo_field,
};
// Only the tests use these through `crate::application::`.
#[cfg(test)]
use thermo_physics::application::{SimConfig, sim_config};

pub mod command_table;
mod domain_command;
pub mod operator_command;
mod physics_event;
mod physics_thread;
pub mod renderer_snapshot;
pub mod scene_layers;
mod snapshot;

#[cfg(test)]
mod tests;

pub use command_table::{CommandParam, DOMAIN_COMMANDS, DomainCommandSpec, domain_command_spec};
pub use domain_command::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
pub use physics_event::PhysicsEvent;
pub use physics_thread::{PhysicsThread, ShutdownSignal, SnapshotPublisher};
pub use snapshot::CreateSnapshot;

// Headless scripts (#340) run domain commands through the same handler.
#[cfg(test)]
pub(crate) use physics_thread::PhysicsRuntime;
pub(crate) use physics_thread::{CommandHandler, DomainCommandHandler};
