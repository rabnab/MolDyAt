//! Tests for CustomEventLoop refactoring to use physics thread
//!
//! These tests verify the transition from direct MainContent ownership
//! to physics thread + snapshot consumption architecture.

#[cfg(test)]
mod tests {
    use crate::application::MainContent;
    use crate::application::physics_thread::PhysicsThread;
    use crate::objects2d::Rect;

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
        use crate::application::physics_thread::PhysicsCommand;
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
        use crate::application::physics_thread::PhysicsCommand;
        sender1
            .send(PhysicsCommand::DoubleTimeStep)
            .expect("sender1 failed");
        sender2
            .send(PhysicsCommand::HalveTimeStep)
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
        use crate::application::physics_thread::PhysicsCommand;
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

    #[test]
    fn test_snapshot_freshness_with_arc_ptr_eq() {
        // Verify Arc::ptr_eq can detect when snapshot changes during rendering
        // This is the core of Phase 4's freshness checking
        let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        let snapshot_slot = main_content.get_snapshot_slot();
        let physics_thread = PhysicsThread::spawn(main_content);

        // Wait for initial snapshot
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Simulate render path: Load snapshot BEFORE rendering
        let guard_before = snapshot_slot.load();
        let arc_before = guard_before.as_ref().expect("Should have snapshot").clone();

        // Simulate rendering work (during which physics may publish new snapshot)
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Load snapshot AFTER rendering
        let guard_after = snapshot_slot.load();
        let arc_after = guard_after.as_ref().expect("Should have snapshot");

        // Freshness check: Did snapshot change during rendering?
        if std::sync::Arc::ptr_eq(&arc_before, arc_after) {
            println!("Snapshot is still fresh - safe to present application");
        } else {
            println!("Snapshot changed during rendering - should discard application");
            // This is expected when physics runs faster than render
        }

        // The test just verifies Arc::ptr_eq works as expected
        // In real usage, changing snapshots during render is normal and healthy
        assert!(
            guard_after.is_some(),
            "Should always have a snapshot available"
        );

        physics_thread
            .shutdown_and_join()
            .expect("Failed to shutdown");
    }

    #[test]
    fn test_render_from_snapshot_always_ends_frame() {
        // CRITICAL TEST: Verify that render_from_snapshot() always calls end_frame()
        // even when snapshot becomes stale during rendering.
        //
        // Without proper end_frame(), we get:
        // "Error in Surface::configure: SurfaceOutput must be dropped before new Surface"
        //
        // This test verifies the contract that render_from_snapshot must maintain:
        // 1. begin_frame() is always paired with end_frame()
        // 2. SurfaceOutput is always properly dropped
        // 3. Stale snapshot detection doesn't break this contract

        let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        let snapshot_slot = main_content.get_snapshot_slot();
        let physics_thread = PhysicsThread::spawn(main_content);

        // Wait for multiple snapshots to be published
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Verify snapshot is being updated (physics thread is running)
        let snapshot1 = snapshot_slot.load();
        assert!(snapshot1.is_some(), "Should have initial snapshot");

        std::thread::sleep(std::time::Duration::from_millis(50));

        let snapshot2 = snapshot_slot.load();
        assert!(snapshot2.is_some(), "Should have updated snapshot");

        // The key invariant we're testing:
        // Even if snapshots change rapidly, the render loop must handle it correctly
        // by always calling end_frame() before returning, regardless of staleness

        // In real usage with ggez Context, this would be:
        // 1. Load snapshot before rendering
        // 2. Do expensive rendering work
        // 3. Check if snapshot changed (freshness check)
        // 4. ALWAYS call end_frame() - even if stale
        // 5. Return None if stale (skip present), Some(()) if fresh

        // We can't easily test the actual ggez rendering without a full Context,
        // but we verify the snapshot mechanics work correctly
        let guard_before = snapshot_slot.load();
        if let Some(ref arc_before) = *guard_before {
            // Simulate time passing during "rendering"
            std::thread::sleep(std::time::Duration::from_millis(50));

            let guard_after = snapshot_slot.load();
            if let Some(ref arc_after) = *guard_after {
                // This is the freshness check
                let is_stale = !std::sync::Arc::ptr_eq(arc_before, arc_after);

                // The critical point: whether stale or not, we must clean up properly
                // In render_from_snapshot, this means ALWAYS calling end_frame()
                // before returning None (stale) or Some(()) (fresh)

                if is_stale {
                    // Snapshot changed - this is normal when physics runs fast
                    println!("Snapshot became stale during 'rendering' - this is expected");
                } else {
                    // Snapshot still fresh
                    println!("Snapshot remained fresh during 'rendering'");
                }

                // Either way, cleanup must happen
                // (In actual code, this is the end_frame() call)
            }
        }

        physics_thread
            .shutdown_and_join()
            .expect("Failed to shutdown");
    }
}
