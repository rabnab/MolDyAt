// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Operator command trait and per-feature sub-modules.
//!
//! Operator commands are self-contained structs that know how to apply a
//! single operator mutation to [`MainContent`]. Commands are routed via
//! [`crate::application::DomainCommand::Operator`] so adding
//! a new operator type requires only:
//!
//! 1. Define a new struct in the appropriate sub-module.
//! 2. Implement [`OperatorCommand`] for it.
//!
//! No changes to `DomainCommand`, `InputAction`, or the action processor
//! are needed.
//!
//! # Sub-modules
//!
//! | Module                    | Operator type   |
//! |---------------------------|-----------------|
//! | [`line`]                  | Line segments   |
//! | [`valve`]                 | Valve segments  |
//! | [`source`]                | Source spheres  |
//! | [`thermostat`]            | Thermostats     |
//! | [`delete`]                | Any of the above |

use crate::application::MainContent;

pub mod delete;
pub mod line;
pub mod source;
pub mod thermostat;
pub mod valve;

/// A self-contained operator command that knows how to apply itself.
///
/// Operator commands encapsulate all data needed for a single operator
/// mutation. New operator types define concrete structs implementing this
/// trait — no changes to `DomainCommand`, `InputAction`, or the action
/// processor are required.
pub trait OperatorCommand: Send + std::fmt::Debug {
    fn execute(self: Box<Self>, content: &mut MainContent);
}
