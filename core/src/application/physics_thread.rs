// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Physics Thread Module
//!
//! This module handles running the physics simulation in a separate thread,
//! independent from the rendering thread. The physics loop runs as fast as
//! possible and publishes snapshots for the renderer to consume.
//!
//! ARCHITECTURAL CONSTRAINT: This module must not depend on ggez.
//! All ggez-specific code belongs in src/application/ggez/.

use crate::application::DomainCommand;
use crate::application::MainContent;
use crate::application::PhysicsEvent;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

mod command_handler;
mod runtime;

#[cfg(test)]
pub(crate) use command_handler::DomainCommandHandler;
#[cfg(test)]
pub(crate) use runtime::{CommandHandler, PhysicsRuntime};

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
    command_sender: Sender<DomainCommand>,
    event_receiver: Receiver<PhysicsEvent>,
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

        // Commands in, one-shot results (save/load) out.
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();

        let wakeup: Arc<dyn Fn() + Send + Sync> = Arc::from(wakeup_callback);
        let thread_wakeup = Arc::clone(&wakeup);

        let thread_handle = thread::spawn(move || {
            let runtime = runtime::PhysicsRuntime::default();
            runtime.run(
                &mut main_content,
                &command_receiver,
                &event_sender,
                &thread_signal,
                &thread_wakeup,
            );
        });

        Self {
            thread_handle: Some(thread_handle),
            shutdown_signal,
            command_sender,
            event_receiver,
        }
    }

    /// The next result reported by the physics thread, if any.
    pub fn try_recv_event(&self) -> Option<PhysicsEvent> {
        self.event_receiver.try_recv().ok()
    }

    /// Send a command to the physics thread
    ///
    /// # Arguments
    /// * `command` - The command to send
    ///
    /// # Returns
    /// `Ok(())` if the command was sent successfully, `Err` if the channel is disconnected
    pub fn send_command(&self, command: impl Into<DomainCommand>) -> Result<(), String> {
        self.command_sender
            .send(command.into())
            .map_err(|_| "Physics thread command channel disconnected".to_string())
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

    #[cfg(test)]
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
