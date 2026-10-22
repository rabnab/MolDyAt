// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::SnapshotPublisher;
use crate::application::tests::{sync_commands, wait_for_snapshot};
use crate::application::{PhysicsThread, SimControl};
use crate::objects2d::Vec2D;
use thermo_physics::test_support::create_main_content;

#[test]
fn test_physics_thread_spawning() {
    let publisher = SnapshotPublisher::new();
    let (main_content, _) = create_main_content();
    let snapshot_slot = publisher.slot();

    let physics_thread = PhysicsThread::spawn(main_content, publisher, Box::new(|| {}));

    // The thread runs and publishes.
    wait_for_snapshot(&snapshot_slot, "a first snapshot", |_| true);

    // The command channel works: a command sent now is applied.
    physics_thread
        .send_command(SimControl::Pause)
        .expect("send pause");
    sync_commands(&physics_thread, &snapshot_slot);

    physics_thread
        .shutdown_and_join()
        .expect("physics thread shuts down cleanly");
}

#[test]
fn test_physics_thread_snapshot_publishing() {
    let publisher = SnapshotPublisher::new();
    let (mut main_content, _) = create_main_content();
    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("Failed to add ball");
    let snapshot_slot = publisher.slot();

    let physics_thread = PhysicsThread::spawn(main_content, publisher, Box::new(|| {}));

    let snapshot = wait_for_snapshot(&snapshot_slot, "a snapshot with the ball", |s| {
        s.balls.len() == 1
    });
    assert!(snapshot.time >= 0.0, "Time should be non-negative");

    physics_thread
        .shutdown_and_join()
        .expect("physics thread shuts down cleanly");
}
