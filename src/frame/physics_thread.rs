//! Physics Thread Module
//!
//! This module handles running the physics simulation in a separate thread,
//! independent from the rendering thread. The physics loop runs as fast as
//! possible and publishes snapshots for the renderer to consume.
//!
//! ARCHITECTURAL CONSTRAINT: This module must not depend on ggez.
//! All ggez-specific code belongs in src/frame/ggez/.

use crate::frame::{GameLogic, MainContent};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Commands that can be sent from the main thread to the physics thread
#[derive(Debug)]
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
}

/// Shutdown signal shared between threads
#[derive(Clone)]
pub struct ShutdownSignal {
    should_stop: Arc<AtomicBool>,
}

impl ShutdownSignal {
    /// Create a new shutdown signal
    pub fn new() -> Self {
        Self {
            should_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal that the thread should shutdown
    pub fn signal_shutdown(&self) {
        self.should_stop.store(true, Ordering::Release);
    }

    /// Check if shutdown has been signaled
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
}

impl PhysicsThread {
    /// Spawn a new physics thread that runs the simulation loop
    ///
    /// # Arguments
    /// * `main_content` - The simulation state to run in the physics thread
    ///
    /// # Returns
    /// A `PhysicsThread` handle that can be used to control the thread
    pub fn spawn(mut main_content: MainContent) -> Self {
        let shutdown_signal = ShutdownSignal::new();
        let thread_signal = shutdown_signal.clone_signal();

        // Create command channel
        let (command_sender, command_receiver) = mpsc::channel();

        let thread_handle = thread::spawn(move || {
            // Log physics thread startup
            println!(
                "Physics thread ID: {:?} - Started!",
                std::thread::current().id()
            );
            println!("Entering physics loop (running as fast as possible)...");

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
                let _ = main_content.simulation_step();

                // Publish snapshot for renderer
                main_content.publish_snapshot();

                // Optional: Add a small yield to prevent CPU starvation
                // This can be tuned based on performance requirements
                std::thread::yield_now();
            }
        });

        Self {
            thread_handle: Some(thread_handle),
            shutdown_signal,
            command_sender,
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
                let _ = main_content.add_candidate(pos);
            }
            PhysicsCommand::AddBallWithVelocity(ball) => {
                let _ = main_content.add_candidate_ball(ball);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::MainContent;
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
        use std::time::Duration;

        // Create MainContent
        let world_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let main_content = MainContent::new(world_rect).expect("Failed to create MainContent");

        // Get snapshot slot before moving
        let snapshot_slot = main_content.get_snapshot_slot();

        // Spawn physics thread
        let physics_thread = PhysicsThread::spawn(main_content);

        // Wait a bit for thread to start
        std::thread::sleep(Duration::from_millis(10));

        // Send pause command
        physics_thread
            .send_command(PhysicsCommand::Pause)
            .expect("Failed to send pause command");

        // Wait for command to be processed
        std::thread::sleep(Duration::from_millis(10));

        // Get initial step count
        let initial_step = snapshot_slot
            .load()
            .as_ref()
            .map(|s| s.simulation_step)
            .unwrap_or(0);

        // Wait and verify simulation is paused (step count shouldn't change)
        std::thread::sleep(Duration::from_millis(50));
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

        // Wait for simulation to run
        std::thread::sleep(Duration::from_millis(50));
        let resumed_step = snapshot_slot
            .load()
            .as_ref()
            .map(|s| s.simulation_step)
            .unwrap_or(0);

        assert!(
            resumed_step > paused_step,
            "Simulation should be running (step count increased)"
        );

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
        let physics_thread = PhysicsThread::spawn(main_content);

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
}
