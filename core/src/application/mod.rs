// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod core;
mod domain_command;
pub(crate) mod log_util;
pub mod operator_command;
pub mod persistence;
pub mod phase_timings;
mod physics_event;
mod physics_thread;
pub mod renderer_snapshot;
pub mod scene_layers;
pub mod sim_config;
mod sim_stats;
mod step_config;
pub mod thermo_field;

#[cfg(test)]
mod tests;

pub use core::{MainContent, valve_span_problem};
pub use domain_command::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
pub use physics_event::PhysicsEvent;
pub(crate) use physics_thread::PhysicsThread;
pub use sim_config::SimConfig;
pub(crate) use step_config::StepConfig;

#[cfg(test)]
pub(crate) use physics_thread::{
    CommandHandler, DomainCommandHandler, PhysicsRuntime, ShutdownSignal,
};

// Re-export error types so existing `crate::application::AppResult` paths still work
pub use crate::error::AppError;
pub use crate::error::AppResult;
