// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The simulation world, its stepping, run settings and saves moved to
// `thermo_physics` (#308); re-exported here so the old paths keep working.
pub use thermo_physics::application::{
    core, log_util, persistence, phase_timings, sim_config, sim_stats, step_config, thermo_field,
    units,
};

mod domain_command;
pub mod operator_command;
mod physics_event;
mod physics_thread;
pub mod renderer_snapshot;
pub mod scene_layers;
mod snapshot;

#[cfg(test)]
mod tests;

pub use domain_command::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
pub use physics_event::PhysicsEvent;
pub(crate) use physics_thread::{PhysicsThread, SnapshotPublisher};
pub use snapshot::{CreateSnapshot, valve_spans};
pub use thermo_physics::application::{
    AppError, AppResult, HIGH_GRAVITY_MPS2, MainContent, NORMAL_GRAVITY_MPS2, SimConfig,
    StepConfig, valve_span_problem,
};

#[cfg(test)]
pub(crate) use physics_thread::{
    CommandHandler, DomainCommandHandler, PhysicsRuntime, ShutdownSignal,
};
