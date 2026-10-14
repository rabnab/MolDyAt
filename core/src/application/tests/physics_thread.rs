// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::application::tests::{sync_commands, wait_for_snapshot};
use crate::application::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
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
    DomainCommand::World(WorldCommand::Resize {
        world_rect: Rect::new(0.0, 0.0, width, -height),
    })
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
    let world_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");
    let snapshot_slot = main_content.get_snapshot_slot();
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    // Running: the step count advances.
    let first = wait_for_snapshot(&snapshot_slot, "a first snapshot", |_| true);
    wait_for_snapshot(&snapshot_slot, "a running step", |s| {
        s.simulation_step > first.simulation_step
    });

    // Paused once the Pause command is applied (sync_commands returns the
    // first snapshot published after it).
    physics_thread
        .send_command(SimControl::Pause)
        .expect("Failed to send pause command");
    let paused = sync_commands(&physics_thread, &snapshot_slot);

    // A second round trip through the physics loop. A running loop steps
    // in every iteration before it publishes, so an unchanged step proves
    // the pause, without a timing window.
    let still_paused = sync_commands(&physics_thread, &snapshot_slot);
    assert_eq!(
        still_paused.simulation_step, paused.simulation_step,
        "Simulation should be paused (step count unchanged)"
    );

    physics_thread
        .send_command(SimControl::Resume)
        .expect("Failed to send resume command");
    wait_for_snapshot(&snapshot_slot, "the simulation to resume", |s| {
        s.simulation_step > paused.simulation_step
    });

    physics_thread
        .shutdown_and_join()
        .expect("Failed to shutdown physics thread");
}

#[test]
fn test_command_channel_speed_control() {
    let world_rect = Rect::new(0.0, 0.0, 1000.0, 1000.0);
    let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");
    let initial_dt = main_content.time_step();
    let snapshot_slot = main_content.get_snapshot_slot();
    let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));

    for _ in 0..3 {
        physics_thread
            .send_command(SimControl::DoubleTimeStep)
            .expect("Failed to send increase speed command");
    }
    let applied = sync_commands(&physics_thread, &snapshot_slot);

    // Simulated time per step is the time step: three doublings make it 8x.
    let later = wait_for_snapshot(&snapshot_slot, "later steps", |s| {
        s.simulation_step >= applied.simulation_step + 10
    });
    let dt = (later.time - applied.time) / (later.simulation_step - applied.simulation_step) as f32;
    let expected = (initial_dt * 8.0).min(0.16);
    assert!(
        (dt - expected).abs() <= expected * 1e-3,
        "time step after three doublings: {dt}, expected {expected}"
    );

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

    let applied = drain_into(
        &mut main_content,
        vec![DomainCommand::Sim(SimControl::ToggleGravity)],
    );

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
            DomainCommand::Balls(BallCommand::Add(in_resized_world_only)),
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
        .send(DomainCommand::World(WorldCommand::Resize {
            world_rect: resized,
        }))
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

/// Pause × rendering (F2) as seen by the physics thread. A running world
/// publishes a snapshot per interval; a paused one publishes nothing until a
/// command changes it, so the renderer stays idle. The layers pick what a
/// snapshot holds, never whether physics runs.
#[test]
fn test_snapshots_follow_changes_for_paused_and_running_by_layers() {
    use crate::application::renderer_snapshot::RendererSnapshot;
    use crate::application::scene_layers::SceneLayers;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const SETTLE: Duration = Duration::from_millis(150);
    let light = SceneLayers::ALL.difference(SceneLayers::HEAVY);
    let no_heatmap = SceneLayers::ALL.difference(SceneLayers::THERMO);

    for paused in [false, true] {
        for layers in [SceneLayers::ALL, no_heatmap, light] {
            let case = format!("paused={paused} layers={layers:?}");
            let mut main_content = MainContent::new(Rect::new(0.0, 0.0, 100.0, -100.0))
                .expect("Failed to create MainContent");
            main_content
                .add_candidate_at_pos(Vec2D::new(30.0, -30.0))
                .expect("add ball");
            let slot = main_content.get_snapshot_slot();
            let physics_thread = PhysicsThread::spawn(main_content, Box::new(|| {}));
            let latest = || slot.load_full();
            let wait_for = |what: &str, done: &dyn Fn(&RendererSnapshot) -> bool| {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if let Some(snapshot) = latest().filter(|s| done(s)) {
                        return snapshot;
                    }
                    assert!(Instant::now() < deadline, "{case}: no snapshot {what}");
                    std::thread::sleep(Duration::from_millis(5));
                }
            };

            let pause = if paused {
                DomainCommand::Sim(SimControl::Pause)
            } else {
                DomainCommand::Sim(SimControl::Resume)
            };
            physics_thread
                .send_command(ViewCommand::SetSnapshotLayers(layers))
                .expect("send layers");
            physics_thread.send_command(pause).expect("send pause");
            wait_for("with the requested layers", &|s| s.layers == layers);
            std::thread::sleep(SETTLE);

            // Paused, the once-per-second step rate report may publish one
            // more snapshot (the rate drops to zero): sample until a window
            // without it.
            let mut samples = 0;
            let (before, after) = loop {
                let before = latest().expect("snapshot");
                std::thread::sleep(SETTLE);
                let after = latest().expect("snapshot");
                samples += 1;
                if !paused || after.physics_fps == before.physics_fps || samples == 4 {
                    break (before, after);
                }
            };
            if paused {
                assert!(
                    Arc::ptr_eq(&before, &after),
                    "{case}: an untouched paused world publishes nothing"
                );
            } else {
                assert!(
                    after.simulation_step > before.simulation_step,
                    "{case}: a running world steps and publishes"
                );
            }

            // The layers decide what a snapshot holds.
            assert_eq!(
                after.balls.len(),
                usize::from(layers.contains(SceneLayers::BALLS)),
                "{case}"
            );
            // Thermo fields are recorded only while the heatmap is shown.
            if !layers.contains(SceneLayers::THERMO) {
                assert!(after.thermo_grid_spec.is_none(), "{case}");
            } else if !paused {
                assert!(after.thermo_grid_spec.is_some(), "{case}");
            }

            // An edit is published also while paused, at the same step.
            physics_thread
                .send_command(BallCommand::NextSize)
                .expect("send edit");
            let edited = wait_for("after the edit", &|s| {
                s.cursor_ball_size_idx != after.cursor_ball_size_idx
            });
            if paused {
                assert_eq!(edited.simulation_step, after.simulation_step, "{case}");
            }

            physics_thread
                .shutdown_and_join()
                .expect("Failed to shutdown physics thread");
        }
    }
}
