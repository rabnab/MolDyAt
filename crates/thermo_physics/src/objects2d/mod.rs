// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub(crate) mod ball_cell;
pub mod ball_impl;
pub mod line_segment;
pub(crate) mod operator;
mod rect_2d;
mod vec_2d;
pub(crate) mod verbose_info;
pub mod wall_chain;

pub use line_segment::{SimLineSegment, ValveSegment};
pub use operator::{
    DEFAULT_THERMOSTAT_CHAIN_LENGTH, DEFAULT_THERMOSTAT_RADIUS, EMISSION_SPEED_STEP,
    LineSegmentCollection, OperatorCollection, OperatorIdCounter, OperatorState,
    SOURCE_SPHERE_RADIUS, SimSourceSphere, SimThermostat, SourceSphereCollection,
    THERMOSTAT_CHAIN_MAX, THERMOSTAT_CHAIN_MIN, THERMOSTAT_RADIUS_STEP,
    THERMOSTAT_TARGET_TEMP_STEP, ThermostatCollection, ValveCollection,
};
pub use rect_2d::Rect;
pub use vec_2d::Vec2D;
pub(crate) use verbose_info::VerboseInfoMode;
