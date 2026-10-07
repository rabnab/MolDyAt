use super::create_main_content;
use crate::application::tests::{sync_commands, wait_for_snapshot};
use crate::application::{DomainCommand, PhysicsThread};
use crate::objects2d::Vec2D;

#[test]
fn test_physics_thread_spawning() {
    let (main_content, _) = create_main_content();
    let snapshot_slot = main_content.get_snapshot_slot();

    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    // The thread runs and publishes.
    wait_for_snapshot(&snapshot_slot, "a first snapshot", |_| true);

    // The command channel works: a command sent now is applied.
    physics_thread
        .send_command(DomainCommand::Pause)
        .expect("send pause");
    sync_commands(&physics_thread, &snapshot_slot);

    physics_thread
        .shutdown_and_join()
        .expect("physics thread shuts down cleanly");
}

#[test]
fn test_physics_thread_snapshot_publishing() {
    let (mut main_content, _) = create_main_content();
    main_content
        .add_candidate_at_pos(Vec2D::new(0.0, 0.0))
        .expect("Failed to add ball");
    let snapshot_slot = main_content.get_snapshot_slot();

    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    let snapshot = wait_for_snapshot(&snapshot_slot, "a snapshot with the ball", |s| {
        s.balls.len() == 1
    });
    assert!(snapshot.time >= 0.0, "Time should be non-negative");

    physics_thread
        .shutdown_and_join()
        .expect("physics thread shuts down cleanly");
}
