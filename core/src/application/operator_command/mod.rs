//! Operator command trait and per-feature sub-modules.
//!
//! Operator commands are self-contained structs that know how to apply a
//! single operator mutation to [`MainContent`]. Commands are routed via
//! [`crate::application::physics_thread::PhysicsCommand::Operator`] so adding
//! a new operator type requires only:
//!
//! 1. Define a new struct in the appropriate sub-module.
//! 2. Implement [`OperatorCommand`] for it.
//! 3. Re-export it below.
//!
//! No changes to `PhysicsCommand`, `InputAction`, or the action processor
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

use crate::application::MainContent;

pub mod line;
pub mod source;
pub mod thermostat;
pub mod valve;

// ── Re-exports for call-site compatibility ────────────────────────────

pub use line::{AddLineSegment, MoveLineSegmentEndpoint};
pub use source::{
    AdjustSourceSphereEmitPeriod, AdjustSourceSphereSpeed, CycleSourceSphereBallSize,
    PlaceSourceSphere, ToggleSourceSphere,
};
pub use thermostat::{
    AdjustThermostatRadius, AdjustThermostatTargetTemperature, MoveThermostat, PlaceThermostat,
    ToggleThermostat,
};
pub use valve::{AddValveSegment, MoveValveSegment, ToggleValve};

/// A self-contained operator command that knows how to apply itself.
///
/// Operator commands encapsulate all data needed for a single operator
/// mutation. New operator types define concrete structs implementing this
/// trait — no changes to `PhysicsCommand`, `InputAction`, or the action
/// processor are required.
pub trait OperatorCommand: Send + std::fmt::Debug {
    fn execute(self: Box<Self>, content: &mut MainContent);
}
