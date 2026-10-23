// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Snapshot tests moved from `thermo_physics` `core/tests/simulation_engine.rs` (#308).

use crate::application::CreateSnapshot;
use crate::objects2d::Vec2D;
use thermo_physics::test_support::create_main_content;

/// Pause freezes physics only: step calls and snapshots leave positions,
/// velocities and energy exactly as they were.
#[test]
fn test_paused_physics_state_is_unchanged_by_steps_and_snapshots() {
    let (mut main_content, _) = create_main_content();
    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("failed to add first ball");
    main_content
        .add_candidate_at_pos(Vec2D::new(1.5, 0.0))
        .expect("failed to add second ball");
    main_content.add_thermostat(Vec2D::new(0.5, 0.0));
    main_content.world_mut().balls_mut()[0].v = Vec2D::new(1.0, 0.5);
    main_content.world_mut().balls_mut()[1].v = Vec2D::new(-0.5, 0.25);
    main_content.world_mut().sim_mut().set_pause(true);

    let state = |mc: &crate::application::MainContent| {
        let world = mc.world();
        (
            world
                .balls()
                .iter()
                .map(|b| (*b.get_position(), b.v))
                .collect::<Vec<_>>(),
            world.sim().get_kinetic_energy(),
            world.sim().get_potential_energy(),
            world.sim().get_time(),
        )
    };
    let before = state(&main_content);

    for _ in 0..20 {
        // The physics thread's step, as in `PhysicsRuntime::run`.
        main_content
            .step_recording_thermo_fields(false)
            .expect("paused step");
        let _ = main_content.create_snapshot();
    }

    assert_eq!(state(&main_content), before);
}
