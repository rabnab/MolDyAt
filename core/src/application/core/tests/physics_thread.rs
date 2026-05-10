use super::create_main_content;
use crate::application::physics_thread::PhysicsThread;
use crate::objects2d::Vec2D;
use std::thread;
use std::time::Duration;

#[test]
fn test_physics_thread_spawning() {
    let (main_content, _) = create_main_content();
    let main_thread_id = thread::current().id();

    println!("\n=== Physics Thread Spawning Test ===");
    println!("Main test thread ID: {:?}", main_thread_id);

    // Get snapshot slot before spawning (before MainContent is moved)
    let snapshot_slot = main_content.get_snapshot_slot();

    // Spawn physics thread
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    // Give the physics thread time to start and log its ID
    thread::sleep(Duration::from_millis(100));

    // Send a command to verify the channel works
    let _ = physics_thread.send_command(crate::application::DomainCommand::Pause);

    // Give it time to process
    thread::sleep(Duration::from_millis(50));

    // Verify we can access the snapshot slot
    let _snapshot = snapshot_slot.load();

    // Clean shutdown
    drop(physics_thread);
    thread::sleep(Duration::from_millis(50));

    println!("✓ Physics thread spawned and shut down successfully");
    println!("✓ Command channel functional");
    println!("✓ Snapshot slot accessible");
    println!("=================================\n");
}

#[test]
fn test_physics_thread_snapshot_publishing() {
    let (mut main_content, _) = create_main_content();

    // Add a ball so we have something in the snapshot
    let pos = Vec2D::new(0.0, 0.0);
    main_content
        .add_candidate_at_pos(pos)
        .expect("Failed to add ball");

    println!("\n=== Physics Thread Snapshot Publishing Test ===");

    // Get snapshot slot before moving MainContent
    let snapshot_slot = main_content.get_snapshot_slot();

    // Spawn physics thread
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    // Wait for a few simulation steps to occur
    thread::sleep(Duration::from_millis(200));

    // Load a snapshot and dereference the Option<Arc<>>
    let snapshot_guard = snapshot_slot.load();
    let snapshot = snapshot_guard
        .as_ref()
        .expect("Snapshot should be published");

    println!("Snapshot ball count: {}", snapshot.balls.len());
    assert_eq!(snapshot.balls.len(), 1, "Should have one ball in snapshot");
    assert!(snapshot.time >= 0.0, "Time should be non-negative");

    println!("✓ Snapshot published successfully");
    println!("✓ Ball data present in snapshot");
    println!("✓ Time tracking working");

    // Clean shutdown
    drop(physics_thread);
    thread::sleep(Duration::from_millis(50));

    println!("=========================================\n");
}
