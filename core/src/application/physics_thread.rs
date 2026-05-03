//! Physics Thread Module
//!
//! This module handles running the physics simulation in a separate thread,
//! independent from the rendering thread. The physics loop runs as fast as
//! possible and publishes snapshots for the renderer to consume.
//!
//! ARCHITECTURAL CONSTRAINT: This module must not depend on ggez.
//! All ggez-specific code belongs in src/application/ggez/.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

mod command_handler;
mod runtime;

pub(crate) use runtime::{CommandHandler, PhysicsRuntime};

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
    /// Unpause the simulation after a resize, triggered by mouse button release.
    ResumeAfterResize,
    /// A self-contained operator command (line segment, valve, etc.).
    Operator(Box<dyn OperatorCommand>),
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
    pub fn spawn(
        mut main_content: MainContent,
        wakeup_callback: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        let shutdown_signal = ShutdownSignal::new();
        let thread_signal = shutdown_signal.clone();

        // Create command channel
        let (command_sender, command_receiver) = mpsc::channel();

        let wakeup: Arc<dyn Fn() + Send + Sync> = Arc::from(wakeup_callback);
        let thread_wakeup = Arc::clone(&wakeup);

        let thread_handle = thread::spawn(move || {
            let runtime = PhysicsRuntime::default();
            runtime.run(
                &mut main_content,
                &command_receiver,
                &thread_signal,
                &thread_wakeup,
            );
        });

        Self {
            thread_handle: Some(thread_handle),
            shutdown_signal,
            command_sender,
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
