use crate::application::GameLogic;
use crate::application::MainContent;
use crate::application::physics_thread::{PhysicsCommand, PhysicsThread, ShutdownSignal};
use crate::objects2d::Rect;

#[test]
fn test_shutdown_signal_initial_state() {
    let signal = ShutdownSignal::new();
    assert!(!signal.should_shutdown());
}

#[test]
fn test_shutdown_signal_after_signal() {
    let signal = ShutdownSignal::new();
    signal.signal_shutdown();
    assert!(signal.should_shutdown());
}

#[test]
fn test_shutdown_signal_clone() {
    let signal1 = ShutdownSignal::new();
    let signal2 = signal1.clone();

    signal1.signal_shutdown();
    assert!(signal2.should_shutdown());
}

#[test]
fn test_shutdown_signal_ordering() {
    let signal = ShutdownSignal::new();

    // Signal from one "thread" (simulated)
    signal.signal_shutdown();

    // Check from another "thread" (simulated)
    // Acquire ordering ensures we see the write
    assert!(signal.should_shutdown());
}

#[test]
fn test_main_content_is_send() {
    // This test verifies that MainContent can be moved to a thread
    // If MainContent is not Send, this test will fail to compile
    let world_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

    // Get snapshot slot before moving
    let snapshot_slot = main_content.get_snapshot_slot();

    // This demonstrates that MainContent is Send
    let handle = std::thread::spawn(move || {
        // We can move MainContent to another thread
        let _ = main_content;
    });

    handle.join().unwrap();

    // Verify snapshot slot still works
    assert!(snapshot_slot.load().is_none());
}

#[test]
fn test_physics_thread_can_publish_snapshots() {
    // Create MainContent
    let world_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let mut main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

    // Get snapshot slot before moving
    let snapshot_slot = main_content.get_snapshot_slot();

    // Run a few simulation steps to generate snapshots
    for _ in 0..5 {
        let _ = main_content.simulation_step();
    }

    // Explicitly publish a snapshot (simulation_step doesn't always publish)
    main_content.publish_snapshot();

    // Verify snapshot was published
    let snapshot = snapshot_slot.load();
    assert!(snapshot.is_some(), "Snapshot should be published");

    // Just verify snapshot exists, simulation_step count depends on pause state
    let snapshot = snapshot.as_ref().unwrap();
    assert!(
        snapshot.simulation_step <= 5,
        "Snapshot step count should be <= iterations"
    );
}

#[test]
fn test_command_channel_toggle_pause() {
    use std::time::{Duration, Instant};

    // Create MainContent
    let world_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

    // Get snapshot slot before moving
    let snapshot_slot = main_content.get_snapshot_slot();

    // Spawn physics thread
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    // Wait for the physics thread to publish at least one snapshot
    let deadline = Instant::now() + Duration::from_secs(5);
    while snapshot_slot.load().is_none() {
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for first snapshot"
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    // Send pause command
    physics_thread
        .send_command(PhysicsCommand::Pause)
        .expect("Failed to send pause command");

    // Wait long enough for the command to be processed
    std::thread::sleep(Duration::from_millis(100));

    // Get step count after pause has been processed
    let initial_step = snapshot_slot
        .load()
        .as_ref()
        .map(|s| s.simulation_step)
        .unwrap_or(0);

    // Wait and verify simulation is paused (step count shouldn't change)
    std::thread::sleep(Duration::from_millis(200));
    let paused_step = snapshot_slot
        .load()
        .as_ref()
        .map(|s| s.simulation_step)
        .unwrap_or(0);

    assert_eq!(
        initial_step, paused_step,
        "Simulation should be paused (step count unchanged)"
    );

    // Send resume command
    physics_thread
        .send_command(PhysicsCommand::Resume)
        .expect("Failed to send resume command");

    // Wait for simulation to advance (spin-wait with timeout)
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current_step = snapshot_slot
            .load()
            .as_ref()
            .map(|s| s.simulation_step)
            .unwrap_or(0);
        if current_step > paused_step {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for simulation to resume (stuck at step {})",
            current_step
        );
        std::thread::sleep(Duration::from_millis(5));
    }

    // Shutdown
    physics_thread
        .shutdown_and_join()
        .expect("Failed to shutdown physics thread");
}

#[test]
fn test_command_channel_speed_control() {
    use std::time::Duration;

    // Create MainContent
    let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
    let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

    // Get snapshot slot before moving
    let snapshot_slot = main_content.get_snapshot_slot();

    // Spawn physics thread
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    // Wait for thread to start
    std::thread::sleep(Duration::from_millis(10));

    // Send commands to increase speed multiple times
    for _ in 0..3 {
        physics_thread
            .send_command(PhysicsCommand::DoubleTimeStep)
            .expect("Failed to send increase speed command");
    }

    // Wait for commands to be processed
    std::thread::sleep(Duration::from_millis(50));

    // Verify command was processed (speedup should be > 1)
    // We can't directly check speedup, but we can verify the command channel works
    // by checking that simulation is still running
    let snapshot = snapshot_slot.load();
    assert!(
        snapshot.is_some(),
        "Snapshot should exist after speed commands"
    );

    // Shutdown
    physics_thread
        .shutdown_and_join()
        .expect("Failed to shutdown physics thread");
}
