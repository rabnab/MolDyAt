use crate::application::DomainCommand;
use crate::application::GameLogic;
use crate::application::MainContent;
use crate::application::{
    CommandHandler, DomainCommandHandler, PhysicsRuntime, PhysicsThread, ShutdownSignal,
};
use crate::objects2d::{Rect, Vec2D};

struct RecordingHandler<T> {
    commands: Vec<T>,
    flushes: usize,
}

impl<T> RecordingHandler<T> {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
            flushes: 0,
        }
    }
}

impl<T> CommandHandler<T> for RecordingHandler<T> {
    fn handle_command(&mut self, command: T) {
        self.commands.push(command);
    }

    fn flush(&mut self) {
        self.flushes += 1;
    }
}

/// Drains `commands` into `main_content` as the physics thread would and
/// returns how many resizes were applied.
fn drain_into(main_content: &mut MainContent, commands: Vec<DomainCommand>) -> u32 {
    let runtime = PhysicsRuntime::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    let (events, _event_receiver) = std::sync::mpsc::channel();
    for command in commands {
        sender.send(command).expect("send should succeed");
    }

    let mut handler = DomainCommandHandler::new(main_content, &events);
    runtime.drain_commands(&receiver, &mut handler);
    handler.resizes_applied()
}

fn resize_to(width: f32, height: f32) -> DomainCommand {
    DomainCommand::Resize {
        world_rect: Rect::new(0.0, 0.0, width, -height),
    }
}

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
        .send_command(DomainCommand::Pause)
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
        .send_command(DomainCommand::Resume)
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
            .send_command(DomainCommand::DoubleTimeStep)
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

#[test]
fn test_runtime_drains_all_available_commands() {
    let runtime = PhysicsRuntime::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut handler = RecordingHandler::new();

    sender.send(1usize).expect("send should succeed");
    sender.send(2usize).expect("send should succeed");
    sender.send(3usize).expect("send should succeed");

    runtime.drain_commands(&receiver, &mut handler);

    assert_eq!(handler.commands, vec![1, 2, 3]);
}

#[test]
fn test_runtime_flushes_the_handler_once_after_draining() {
    let runtime = PhysicsRuntime::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut handler = RecordingHandler::new();

    sender.send(1usize).expect("send should succeed");
    sender.send(2usize).expect("send should succeed");

    runtime.drain_commands(&receiver, &mut handler);

    assert_eq!(handler.flushes, 1);
    assert_eq!(handler.commands, vec![1, 2]);
}

#[test]
fn test_consecutive_resizes_apply_only_the_last() {
    let mut main_content =
        MainContent::new(Rect::new(0.0, 0.0, 100.0, -100.0)).expect("Failed to create MainContent");

    let applied = drain_into(
        &mut main_content,
        vec![
            resize_to(150.0, 120.0),
            resize_to(200.0, 180.0),
            resize_to(300.0, 250.0),
        ],
    );

    assert_eq!(applied, 1);
    assert_eq!(
        main_content.world().world_region(),
        Rect::new(0.0, 0.0, 300.0, -250.0)
    );
}

#[test]
fn test_no_resize_is_applied_without_a_resize_command() {
    let mut main_content =
        MainContent::new(Rect::new(0.0, 0.0, 100.0, -100.0)).expect("Failed to create MainContent");

    let applied = drain_into(&mut main_content, vec![DomainCommand::ToggleGravity]);

    assert_eq!(applied, 0);
}

#[test]
fn test_resize_keeps_its_place_among_other_commands() {
    let mut main_content =
        MainContent::new(Rect::new(0.0, 0.0, 100.0, -100.0)).expect("Failed to create MainContent");

    // Only inside the world of the first resize: the ball is added only if
    // that resize is applied before `AddBall`, not after it or at the end.
    let in_resized_world_only = Vec2D::new(250.0, -250.0);
    let applied = drain_into(
        &mut main_content,
        vec![
            resize_to(300.0, 300.0),
            DomainCommand::AddBall(in_resized_world_only),
            resize_to(320.0, 320.0),
        ],
    );

    assert_eq!(applied, 2);
    assert_eq!(main_content.world().balls.len(), 1);
    assert_eq!(
        main_content.world().world_region(),
        Rect::new(0.0, 0.0, 320.0, -320.0)
    );
}

#[test]
fn test_runtime_publishes_a_snapshot_right_after_a_resize() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    let mut main_content =
        MainContent::new(Rect::new(0.0, 0.0, 100.0, -100.0)).expect("Failed to create MainContent");
    let snapshot_slot = main_content.get_snapshot_slot();
    let resized = Rect::new(0.0, 0.0, 300.0, -200.0);

    let (sender, receiver) = std::sync::mpsc::channel();
    let (events, _event_receiver) = std::sync::mpsc::channel();
    let signal = ShutdownSignal::new();
    let wakeups = Arc::new(AtomicUsize::new(0));
    let wakeup: Arc<dyn Fn() + Send + Sync> = {
        let wakeups = Arc::clone(&wakeups);
        Arc::new(move || {
            wakeups.fetch_add(1, Ordering::SeqCst);
        })
    };

    // No regular snapshot comes within this interval: only a resize can
    // publish one.
    let runtime = PhysicsRuntime::new(Duration::from_secs(3600));
    let thread_signal = signal.clone();
    let handle = std::thread::spawn(move || {
        runtime.run(
            &mut main_content,
            &receiver,
            &events,
            &thread_signal,
            &wakeup,
        );
    });

    sender
        .send(DomainCommand::Resize {
            world_rect: resized,
        })
        .expect("send should succeed");

    let deadline = Instant::now() + Duration::from_secs(5);
    let snapshot = loop {
        if let Some(snapshot) = snapshot_slot.load_full() {
            break snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for the snapshot after the resize"
        );
        std::thread::sleep(Duration::from_millis(5));
    };

    signal.signal_shutdown();
    handle.join().expect("physics loop should not panic");

    assert_eq!(snapshot.cell_grid_bounds, resized);
    assert!(wakeups.load(Ordering::SeqCst) >= 1);
}

#[test]
fn test_physics_thread_shutdown_and_join_is_graceful() {
    use std::time::{Duration, Instant};

    let world_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    let start = Instant::now();
    physics_thread
        .shutdown_and_join()
        .expect("Failed to shutdown physics thread");

    assert!(
        start.elapsed() < Duration::from_secs(2),
        "physics thread shutdown should complete promptly"
    );
}

#[test]
fn test_paused_world_keeps_its_render_revision_until_a_command() {
    use std::time::{Duration, Instant};

    let world_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");
    let snapshot_slot = main_content.get_snapshot_slot();
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));
    let revision = || snapshot_slot.load().as_ref().map(|s| s.revision);
    let created_at = || snapshot_slot.load().as_ref().map(|s| s.created_at);

    physics_thread
        .send_command(DomainCommand::Pause)
        .expect("send pause");
    std::thread::sleep(Duration::from_millis(150));

    // Paused and untouched: snapshots keep coming, the revision does not move.
    let paused_revision = revision();
    let paused_at = created_at();
    std::thread::sleep(Duration::from_millis(150));
    assert_ne!(created_at(), paused_at, "snapshots are still published");
    assert_eq!(revision(), paused_revision, "an idle paused world is idle");

    // An edit while paused is a new revision.
    physics_thread
        .send_command(DomainCommand::AddBall(Vec2D::new(10.0, 10.0)))
        .expect("send add ball");
    let deadline = Instant::now() + Duration::from_secs(5);
    while revision() == paused_revision {
        assert!(Instant::now() < deadline, "edit never reached a snapshot");
        std::thread::sleep(Duration::from_millis(5));
    }

    // F2 off: light snapshots, and full again after F2 on.
    physics_thread
        .send_command(DomainCommand::SetFullSnapshots(false))
        .expect("send light");
    let deadline = Instant::now() + Duration::from_secs(5);
    while snapshot_slot.load().as_ref().is_none_or(|s| s.full_scene) {
        assert!(Instant::now() < deadline, "no light snapshot");
        std::thread::sleep(Duration::from_millis(5));
    }
    physics_thread
        .send_command(DomainCommand::SetFullSnapshots(true))
        .expect("send full");
    let deadline = Instant::now() + Duration::from_secs(5);
    while snapshot_slot.load().as_ref().is_none_or(|s| !s.full_scene) {
        assert!(Instant::now() < deadline, "no full snapshot");
        std::thread::sleep(Duration::from_millis(5));
    }
}
