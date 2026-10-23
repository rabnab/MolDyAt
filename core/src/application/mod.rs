// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The simulation world, its stepping, run settings and saves moved to
// `thermo_physics` (#308); re-exported here so the old paths keep working.

// The physics thread, commands and snapshots moved to `thermo_runtime`
// (#309), the physics to `thermo_physics` (#308); re-exported here so the
// old paths keep working for one release.
pub use thermo_physics::application::{
    AppError, AppResult, HIGH_GRAVITY_MPS2, MainContent, NORMAL_GRAVITY_MPS2, SimConfig,
    StepConfig, valve_span_problem,
};
pub use thermo_physics::application::{
    core, log_util, persistence, phase_timings, sim_config, sim_stats, step_config, thermo_field,
    units,
};
pub use thermo_runtime::application::{
    BallCommand, CreateSnapshot, DomainCommand, PhysicsEvent, PhysicsThread, SimControl,
    SnapshotPublisher, ViewCommand, WorldCommand, operator_command, renderer_snapshot,
    scene_layers, valve_spans,
};
