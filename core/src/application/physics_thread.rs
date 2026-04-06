//! Physics Thread Module
//!
//! This module handles running the physics simulation in a separate thread,
//! independent from the rendering thread. The physics loop runs as fast as
//! possible and publishes snapshots for the renderer to consume.
//!
//! ARCHITECTURAL CONSTRAINT: This module must not depend on ggez.
//! All ggez-specific code belongs in src/application/ggez/.

use crate::application::{GameLogic, MainContent};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Commands that can be sent from the main thread to the physics thread
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum PhysicsCommand {
    /// Pause the simulation
    Pause,
    /// Resume the simulation
    Resume,
    /// Toggle pause state
    TogglePause,
    /// Add a ball at the specified position with zero velocity
    AddBall(Vec2D),
    /// Add a specific ball (with position and velocity)
    AddBallWithVelocity(Ball2D),
    /// Add multiple random balls
    AddRandomBalls(usize),
    /// Clear all balls
    ClearBalls,
    /// Stop all ball movement (set velocities to zero)
    StopAllBalls,
    /// Randomize ball velocities
    RandomizeSpeeds,
    /// Enable random ball speed mode
    EnableRandomBallSpeed,
    /// Cycle to next ball size
    NextBallSize,
    /// Double the simulation time step (faster, less accurate)
    DoubleTimeStep,
    /// Halve the simulation time step (slower, more accurate)
    HalveTimeStep,
    /// Increase wait time between simulation steps
    IncreaseWaitTime,
    /// Decrease wait time between simulation steps
    DecreaseWaitTime,
    /// Toggle real data display
    ToggleRealData,
    /// Toggle step-by-step mode
    ToggleStepwise,
    /// Toggle timing display
    ToggleTimingDisplay,
    /// Toggle ball stats display
    ToggleBallStats,
    /// Start handling verbosity input
    StartVerbosityInput,
    /// Resize the world region
    Resize { world_rect: crate::objects2d::Rect },
    /// Add a line segment barrier with start and end positions
    AddLineSegment { start: Vec2D, end: Vec2D },
    /// Move a specific endpoint of a line segment to a new position.
    /// `move_start`: `true` → start, `false` → end.
    MoveLineSegmentEndpoint {
        segment_id: usize,
        new_pos: Vec2D,
        move_start: bool,
    },
}

/// Shutdown signal shared between threads
#[derive(Clone)]
pub struct ShutdownSignal {
    should_stop: Arc<AtomicBool>,
}

impl ShutdownSignal {
    /// Create new shutdown signal for coordinating thread termination.
    pub fn new() -> Self {
        Self {
            should_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal that the thread should shutdown.
    /// Uses Release ordering to ensure all prior writes are visible to the thread reading this signal.
    pub fn signal_shutdown(&self) {
        self.should_stop.store(true, Ordering::Release);
    }

    /// Check if shutdown has been signaled.
    /// Uses Acquire ordering to ensure all writes from signaling thread are visible.
    pub fn should_shutdown(&self) -> bool {
        self.should_stop.load(Ordering::Acquire)
    }

    /// Clone the signal for sharing across threads
    pub fn clone_signal(&self) -> Self {
        Self {
            should_stop: Arc::clone(&self.should_stop),
        }
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to the physics simulation thread
pub struct PhysicsThread {
    thread_handle: Option<JoinHandle<()>>,
    shutdown_signal: ShutdownSignal,
    command_sender: Sender<PhysicsCommand>,
    wakeup_callback: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl PhysicsThread {
    /// Spawn a new physics thread that runs the simulation loop
    ///
    /// # Arguments
    /// * `main_content` - The simulation state to run in the physics thread
    ///
    /// # Returns
    /// A `PhysicsThread` handle that can be used to control the thread
    pub fn spawn(
        mut main_content: MainContent,
        wakeup_callback: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        let shutdown_signal = ShutdownSignal::new();
        let thread_signal = shutdown_signal.clone_signal();

        // Create command channel
        let (command_sender, command_receiver) = mpsc::channel();

        let wakeup = Arc::new(wakeup_callback);
        let thread_wakeup = Arc::clone(&wakeup);

        let thread_handle = thread::spawn(move || {
            // Log physics thread startup
            println!(
                "Physics thread ID: {:?} - Started!",
                std::thread::current().id()
            );
            println!("Entering physics loop (running as fast as possible)...");

            // Snapshot throttling: only publish at ~60 Hz to match typical vsync
            // This prevents expensive snapshot creation from slowing down physics
            // 60 Hz provides smooth rendering while minimizing CPU overhead
            let snapshot_interval = std::time::Duration::from_micros(16667); // ~60 Hz
            let mut last_snapshot_time = std::time::Instant::now();

            // Physics performance tracking
            let mut physics_step_count: u64 = 0;
            let mut last_perf_report = std::time::Instant::now();
            let mut snapshot_time_total = std::time::Duration::ZERO;
            let mut snapshot_count = 0u64;

            // Physics loop: run simulation as fast as possible
            while !thread_signal.should_shutdown() {
                // Process all pending commands
                Self::process_commands(&mut main_content, &command_receiver);

                // Get current wait time
                let wait_time_ms = main_content.get_wait_time();
                if wait_time_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(wait_time_ms));
                }
                // Run one simulation step per iteration
                let was_paused = main_content.sim.is_paused();
                let _ = main_content.simulation_step();
                // Only count steps where physics actually ran (not paused idle loops
                // which would otherwise inflate the reported step rate).
                if !was_paused {
                    physics_step_count += 1;
                }

                // Publish snapshot for renderer only at throttled rate (~60 Hz)
                // This prevents expensive snapshot creation from slowing down physics
                let now = std::time::Instant::now();
                if now.duration_since(last_snapshot_time) >= snapshot_interval {
                    let snapshot_start = std::time::Instant::now();
                    main_content.publish_snapshot();
                    // Call wakeup callback to notify renderer of new snapshot
                    thread_wakeup();

                    snapshot_time_total += snapshot_start.elapsed();
                    snapshot_count += 1;
                    last_snapshot_time = now;
                }

                // Update physics FPS counter (lock-free atomic write)
                if now.duration_since(last_perf_report).as_secs() >= 1 {
                    let elapsed = now.duration_since(last_perf_report).as_secs_f32();
                    let physics_fps = physics_step_count as f32 / elapsed;

                    main_content.set_physics_fps(physics_fps);

                    // Calculate snapshot overhead
                    if snapshot_count > 0 {
                        let avg_snapshot_us =
                            snapshot_time_total.as_micros() / snapshot_count as u128;
                        let snapshot_hz = snapshot_count as f32 / elapsed;
                        let total_overhead_pct =
                            (snapshot_time_total.as_secs_f32() / elapsed) * 100.0;
                        log::debug!(
                            "Physics thread: {:.0} steps/sec | Snapshots: {:.1} Hz, {:.0} µs avg ({:.1}% overhead)",
                            physics_fps,
                            snapshot_hz,
                            avg_snapshot_us,
                            total_overhead_pct
                        );
                    } else {
                        log::debug!("Physics thread: {:.0} steps/sec", physics_fps);
                    }

                    physics_step_count = 0;
                    snapshot_time_total = std::time::Duration::ZERO;
                    snapshot_count = 0;
                    last_perf_report = now;
                }

                // Optional: Add a small yield to prevent CPU starvation
                // This can be tuned based on performance requirements
                std::thread::yield_now();
            }
        });

        Self {
            thread_handle: Some(thread_handle),
            shutdown_signal,
            command_sender,
            wakeup_callback: Some(wakeup),
        }
    }

    /// Process commands from the command channel
    fn process_commands(main_content: &mut MainContent, receiver: &Receiver<PhysicsCommand>) {
        // Process all available commands without blocking
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    Self::execute_command(main_content, command);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    /// Execute a single command
    fn execute_command(main_content: &mut MainContent, command: PhysicsCommand) {
        match command {
            PhysicsCommand::Pause => {
                if !main_content.sim.is_paused() {
                    main_content.toggle_pause();
                }
            }
            PhysicsCommand::Resume => {
                if main_content.sim.is_paused() {
                    main_content.toggle_pause();
                }
            }
            PhysicsCommand::TogglePause => {
                main_content.toggle_pause();
            }
            PhysicsCommand::AddBall(pos) => {
                let _ = main_content.add_candidate_at_pos(pos);
            }
            PhysicsCommand::AddBallWithVelocity(ball) => {
                let _ = main_content.add_candidate_ball(ball, 0.);
            }
            PhysicsCommand::AddRandomBalls(count) => {
                main_content.add_random_candidates(count);
            }
            PhysicsCommand::ClearBalls => {
                main_content.clear_points();
            }
            PhysicsCommand::StopAllBalls => {
                main_content.stop_all_balls();
            }
            PhysicsCommand::RandomizeSpeeds => {
                main_content.randomize_speeds();
            }
            PhysicsCommand::EnableRandomBallSpeed => {
                main_content.enable_random_ball_speed();
            }
            PhysicsCommand::NextBallSize => {
                main_content.next_ball_size();
            }
            PhysicsCommand::IncreaseWaitTime => {
                main_content.increase_wait_time();
            }
            PhysicsCommand::DecreaseWaitTime => {
                main_content.decrease_wait_time();
            }
            PhysicsCommand::DoubleTimeStep => {
                main_content.double_time_step();
            }
            PhysicsCommand::HalveTimeStep => {
                main_content.halve_time_step();
            }
            PhysicsCommand::ToggleRealData => {
                main_content.toggle_real_data();
            }
            PhysicsCommand::ToggleStepwise => {
                main_content.toggle_stepwise();
            }
            PhysicsCommand::ToggleTimingDisplay => {
                main_content.toggle_timing_display();
            }
            PhysicsCommand::ToggleBallStats => {
                main_content.toggle_show_ball_stats();
            }
            PhysicsCommand::StartVerbosityInput => {
                main_content.start_handling_verbosity_input();
            }
            PhysicsCommand::Resize { world_rect } => {
                main_content.resize(world_rect);
            }
            PhysicsCommand::AddLineSegment { start, end } => {
                main_content.add_line_segment(start, end);
            }
            PhysicsCommand::MoveLineSegmentEndpoint {
                segment_id,
                new_pos,
                move_start,
            } => {
                main_content.move_line_segment_endpoint(segment_id, new_pos, move_start);
            }
        }
    }

    /// Send a command to the physics thread
    ///
    /// # Arguments
    /// * `command` - The command to send
    ///
    /// # Returns
    /// `Ok(())` if the command was sent successfully, `Err` if the channel is disconnected
    pub fn send_command(&self, command: PhysicsCommand) -> Result<(), String> {
        self.command_sender
            .send(command)
            .map_err(|_| "Physics thread command channel disconnected".to_string())
    }

    /// Get a clone of the command sender for sharing across threads
    pub fn get_command_sender(&self) -> Sender<PhysicsCommand> {
        self.command_sender.clone()
    }

    /// Signal the physics thread to shutdown
    pub fn shutdown(&mut self) {
        self.shutdown_signal.signal_shutdown();
    }

    /// Wait for the physics thread to complete
    ///
    /// This will block until the thread exits. Call `shutdown()` first
    /// to signal the thread to stop.
    pub fn join(mut self) -> Result<(), String> {
        if let Some(handle) = self.thread_handle.take() {
            handle
                .join()
                .map_err(|_| "Physics thread panicked".to_string())?;
        }
        Ok(())
    }

    /// Shutdown and wait for the physics thread to complete
    pub fn shutdown_and_join(mut self) -> Result<(), String> {
        self.shutdown();
        self.join()
    }
}

impl Drop for PhysicsThread {
    fn drop(&mut self) {
        // Always signal shutdown when dropped
        self.shutdown();

        // Wait for thread to finish if it hasn't been joined yet
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
