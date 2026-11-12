// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod alloc_count;
mod ball_management;
mod ball_reorder;
mod barrier_energy;
mod drift_harness;
mod energy_conservation;
mod external_fields;
mod gpu_pass;
mod gravity;
mod overlap_resolution;
mod placement;
mod pressure;
mod resize;
mod rotation;
mod simulation;
mod simulation_engine;
mod soft_walls;
mod wall_backstop;
mod wall_handover;
mod wall_momentum;

mod ghost_wall;
mod line_segments;
mod molecules;
mod object_deletion;
mod source_spheres;
mod species;
mod species_ui;
mod thermostats;
mod valves;
mod verbose;
mod water;

// The world builders are shared with the tests of `thermo_runtime` (feature
// `test-support`).
pub(crate) use crate::test_support::{
    DT_GOAL, create_main_content, create_main_content_for_region, new_main_content,
    new_main_content_seeded, test_config,
};

#[cfg(gpu_drift)]
mod gpu_bridge;

/// With `--cfg gpu_drift` (`scripts/drift_gpu.sh`): put `content` on the
/// GPU force pass, if there is an adapter. Otherwise nothing.
pub(crate) fn gpu_forces_if_asked(_content: &mut crate::application::MainContent) {
    #[cfg(gpu_drift)]
    gpu_bridge::switch_on(_content);
}

/// The derived time step limit (#427) of the built-in species `name` on its
/// own, with the default caps.
pub(crate) fn time_step_limit_of(name: &str) -> f32 {
    use crate::objects2d::ball_impl::{BallPhysics, SpeciesTable};
    let id = SpeciesTable::builtin().by_name(name).unwrap();
    BallPhysics::default()
        .time_step_limit([id].into_iter().collect())
        .unwrap()
        .0
}
