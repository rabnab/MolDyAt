// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod core;
pub mod log_util;
pub mod persistence;
pub mod phase_timings;
pub mod sim_config;
pub mod sim_stats;
pub mod step_config;
pub mod thermo_field;
pub mod units;

#[cfg(test)]
mod tests;

pub use core::{MainContent, valve_span_problem};
pub use sim_config::SimConfig;
pub use sim_stats::{HIGH_GRAVITY_MPS2, NORMAL_GRAVITY_MPS2};
pub use step_config::StepConfig;

pub use crate::error::AppError;
pub use crate::error::AppResult;
