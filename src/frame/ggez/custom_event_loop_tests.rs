//! Tests for CustomEventLoop refactoring to use physics thread
//!
//! These tests verify the transition from direct MainContent ownership
//! to physics thread + snapshot consumption architecture.

#[cfg(test)]
mod tests {
    use crate::frame::MainContent;
    use crate::frame::physics_thread::PhysicsThread;
    use crate::objects2d::Rect;
    use std::sync::mpsc;

    #[test]
    fn test_can_spawn_physics_thread_separately() {
        // Verify we can create MainContent and spawn physics thread
        // This is the basic building block for CustomEventLoop refactoring
        let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        // Get snapshot slot before moving to thread
        let snapshot_slot = main_content.get_snapshot_slot();

        // Spawn physics thread
        let physics_thread = PhysicsThread::spawn(main_content);

        // Verify we can still access snapshots
        std::thread::sleep(std::time::Duration::from_millis(50));
        let snapshot = snapshot_slot.load();
        assert!(
            snapshot.is_some(),
            "Should have snapshot from physics thread"
        );

        // Cleanup
        physics_thread
            .shutdown_and_join()
            .expect("Failed to shutdown");
    }

    #[test]
    fn test_can_get_command_sender_for_event_loop() {
        // Verify we can get a command sender for forwarding user input
        let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        let physics_thread = PhysicsThread::spawn(main_content);

        // Get command sender (this would be stored in CustomEventLoop)
        let command_sender = physics_thread.get_command_sender();

        // Verify we can send commands through the sender
        use crate::frame::physics_thread::PhysicsCommand;
        command_sender
            .send(PhysicsCommand::Pause)
            .expect("Failed to send command");

        // Cleanup
        physics_thread
            .shutdown_and_join()
            .expect("Failed to shutdown");
    }

    #[test]
    fn test_snapshot_slot_shareable_across_threads() {
        // Verify snapshot slot can be shared between physics thread and event loop
        let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        // Clone snapshot slot before moving MainContent
        let snapshot_slot1 = main_content.get_snapshot_slot();
        let snapshot_slot2 = main_content.get_snapshot_slot();

        // Spawn physics thread (moves main_content)
        let physics_thread = PhysicsThread::spawn(main_content);

        // Wait for physics to publish
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Both slots should see the same snapshot
        let snapshot1 = snapshot_slot1.load();
        let snapshot2 = snapshot_slot2.load();

        assert!(snapshot1.is_some());
        assert!(snapshot2.is_some());

        // Verify they're the same Arc
        if let (Some(s1), Some(s2)) = (snapshot1.as_ref(), snapshot2.as_ref()) {
            assert_eq!(s1.simulation_step, s2.simulation_step);
        }

        // Cleanup
        physics_thread
            .shutdown_and_join()
            .expect("Failed to shutdown");
    }

    #[test]
    fn test_multiple_command_senders() {
        // Verify we can clone command sender for different event handlers
        let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        let physics_thread = PhysicsThread::spawn(main_content);

        // Get multiple senders (simulating different event handlers)
        let sender1 = physics_thread.get_command_sender();
        let sender2 = sender1.clone();

        // Both can send commands
        use crate::frame::physics_thread::PhysicsCommand;
        sender1
            .send(PhysicsCommand::IncreaseSpeed)
            .expect("sender1 failed");
        sender2
            .send(PhysicsCommand::DecreaseSpeed)
            .expect("sender2 failed");

        // Cleanup
        physics_thread
            .shutdown_and_join()
            .expect("Failed to shutdown");
    }

    #[test]
    fn test_event_loop_would_have_these_components() {
        // This test documents what CustomEventLoop will need after refactoring
        let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        // Component 1: Snapshot slot for rendering
        let snapshot_slot = main_content.get_snapshot_slot();

        // Component 2: Physics thread handle
        let physics_thread = PhysicsThread::spawn(main_content);

        // Component 3: Command sender for user input
        let command_sender = physics_thread.get_command_sender();

        // Simulate event loop operation:
        // 1. Send user command
        use crate::frame::physics_thread::PhysicsCommand;
        command_sender
            .send(PhysicsCommand::TogglePause)
            .expect("Failed to send");

        // 2. Wait for physics to process
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 3. Render from snapshot
        let snapshot = snapshot_slot.load();
        assert!(snapshot.is_some(), "Should have snapshot for rendering");

        // Cleanup
        physics_thread
            .shutdown_and_join()
            .expect("Failed to shutdown");
    }
}
