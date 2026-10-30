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

use crate::application::CreateSnapshot;
use crate::application::DomainCommand;
use crate::application::MainContent;
use crate::application::PhysicsEvent;
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::application::scene_layers::SceneLayers;
use crate::metrics::SharedMetricsRegistry;
use arc_swap::ArcSwapOption;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

mod command_handler;
mod runtime;

pub(crate) use command_handler::DomainCommandHandler;
pub(crate) use runtime::CommandHandler;
#[cfg(test)]
pub(crate) use runtime::PhysicsRuntime;

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

/// Where the physics thread publishes its snapshots, and which layers they
/// have. The renderer reads the slot lock-free (see `PhysicsCoordinator`).
///
/// Snapshots are reused (#341): publishing keeps the snapshot it replaces,
/// and the next one refills it if the renderer has dropped it by then, so
/// a steady run allocates no new ball vector or top-level operator
/// vectors (the per-wall id lists and the debug data of verbose balls are
/// still built anew; they are small).
pub struct SnapshotPublisher {
    slot: Arc<ArcSwapOption<RendererSnapshot>>,
    /// Layers the renderer wants in its snapshots (see `SceneLayers`).
    layers: SceneLayers,
    /// The snapshot replaced by the last publish, to be refilled.
    spare: Cell<Option<Arc<RendererSnapshot>>>,
}

impl SnapshotPublisher {
    /// An empty slot; snapshots with every layer.
    pub fn new() -> Self {
        Self {
            slot: Arc::new(ArcSwapOption::from(None)),
            layers: SceneLayers::ALL,
            spare: Cell::new(None),
        }
    }

    /// The slot, shared with the renderer.
    pub fn slot(&self) -> Arc<ArcSwapOption<RendererSnapshot>> {
        Arc::clone(&self.slot)
    }

    pub fn layers(&self) -> SceneLayers {
        self.layers
    }

    /// Build snapshots with only `layers` from now on. Without `THERMO` the
    /// thermo fields are no longer recorded; their history is dropped, so a
    /// heatmap shown again later never mixes in fields from before.
    pub fn set_layers(&mut self, content: &mut MainContent, layers: SceneLayers) {
        if !layers.contains(SceneLayers::THERMO) {
            content.clear_thermo_history();
        }
        self.layers = layers;
    }

    /// A snapshot of `content` with this publisher's layers: the spare
    /// snapshot refilled when no one holds it any more, else a new one.
    pub fn snapshot(&self, content: &MainContent) -> RendererSnapshot {
        let mut snapshot = self
            .spare
            .take()
            .and_then(|spare| Arc::try_unwrap(spare).ok())
            .unwrap_or_default();
        content.fill_snapshot(self.layers, &mut snapshot);
        snapshot
    }

    /// Replace the published snapshot (an atomic `Arc` swap, so the physics
    /// thread never waits for the renderer). The replaced one is kept for
    /// reuse by [`Self::snapshot`].
    pub fn publish(&self, snapshot: RendererSnapshot) {
        self.spare.set(self.slot.swap(Some(Arc::new(snapshot))));
    }
}

impl Default for SnapshotPublisher {
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
    /// * `publisher` - Where the thread publishes its snapshots; take its
    ///   `slot()` before handing it over
    /// * `metrics` - Shared metrics registry for time-series data
    ///
    /// # Returns
    /// A `PhysicsThread` handle that can be used to control the thread
    pub fn spawn(
        mut main_content: MainContent,
        mut publisher: SnapshotPublisher,
        wakeup_callback: Box<dyn Fn() + Send + Sync + 'static>,
        metrics: SharedMetricsRegistry,
    ) -> Self {
        let shutdown_signal = ShutdownSignal::new();
        let thread_signal = shutdown_signal.clone();

        // Commands in, one-shot results (save/load) out.
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();

        let wakeup: Arc<dyn Fn() + Send + Sync> = Arc::from(wakeup_callback);
        let thread_wakeup = Arc::clone(&wakeup);

        let thread_handle = thread::spawn(move || {
            let runtime = runtime::PhysicsRuntime;
            runtime.run(
                &mut main_content,
                &mut publisher,
                &command_receiver,
                &event_sender,
                &thread_signal,
                &thread_wakeup,
                &metrics,
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

    /// Shutdown and wait for the physics thread to complete
    #[cfg(test)]
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
