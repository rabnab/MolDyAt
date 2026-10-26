// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use arc_swap::ArcSwapOption;
use std::sync::{Arc, RwLock};
use thermo_physics::application::MainContent;
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
use thermo_runtime::application::{DomainCommand, PhysicsEvent, PhysicsThread, SnapshotPublisher};
use thermo_runtime::metrics::{MetricsRegistry, SharedMetricsRegistry};

/// Coordinates communication between render thread and physics thread
pub struct PhysicsCoordinator {
    physics_thread: Option<PhysicsThread>,
    snapshot_slot: Option<Arc<ArcSwapOption<RendererSnapshot>>>,

    /// Shared metrics registry for time-series data.
    metrics: SharedMetricsRegistry,

    // Step tracking
    last_displayed_step: u64,
    /// `world_generation` of the last displayed snapshot.
    displayed_generation: u64,
    /// The last displayed snapshot. The physics thread publishes a snapshot
    /// only when the world changed, so any other one is new.
    displayed_snapshot: Option<Arc<RendererSnapshot>>,
    steps_per_frame: f32,

    // Performance metrics
    render_fps: u32,
    last_reported_render_fps: f32,
    render_report_interval: std::time::Instant,

    /// Test-only: captures the debug representation of every domain command sent.
    #[cfg(test)]
    recorded_commands: Option<Arc<std::sync::Mutex<Vec<String>>>>,
}

impl PhysicsCoordinator {
    /// Create new coordinator and spawn physics thread
    ///
    /// # Arguments
    /// * `application` - MainContent to move to physics thread
    pub fn new(
        application: MainContent,
        wakeup_callback: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        let publisher = SnapshotPublisher::new();
        let snapshot_slot = publisher.slot();

        // Create shared metrics registry
        let metrics = Arc::new(RwLock::new(MetricsRegistry::default()));

        let physics_thread = PhysicsThread::spawn(
            application,
            publisher,
            wakeup_callback,
            Arc::clone(&metrics),
        );
        Self {
            physics_thread: Some(physics_thread),
            snapshot_slot: Some(snapshot_slot),
            metrics,
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_snapshot: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            #[cfg(test)]
            recorded_commands: None,
        }
    }

    /// Send a domain-level command to the physics thread.
    pub fn send_command(&self, command: impl Into<DomainCommand>) {
        let command = command.into();
        #[cfg(test)]
        if let Some(recorded) = &self.recorded_commands {
            recorded
                .lock()
                .expect("recorded_commands lock should succeed")
                .push(format!("{command:?}"));
        }
        if let Some(thread) = self.physics_thread.as_ref() {
            let _ = thread.send_command(command);
        }
    }

    /// The next one-shot result (save/load) reported by the physics thread.
    pub fn try_recv_event(&self) -> Option<PhysicsEvent> {
        self.physics_thread.as_ref()?.try_recv_event()
    }

    /// Get latest snapshot if available
    /// Returns None if no snapshot slot or no snapshot published yet
    pub fn get_latest_snapshot(&self) -> Option<Arc<RendererSnapshot>> {
        let slot = self.snapshot_slot.as_ref()?;
        let guard = slot.load();
        guard.as_ref().map(Arc::clone)
    }

    /// Whether a snapshot was published that was not displayed yet. The
    /// physics thread publishes only when the world changed (a step, an edit,
    /// a load, a resize), so a paused, untouched world has none.
    pub fn is_new_snapshot_available(&self) -> bool {
        match (self.get_latest_snapshot(), &self.displayed_snapshot) {
            (Some(latest), Some(displayed)) => !Arc::ptr_eq(&latest, displayed),
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    /// Get the last displayed step number
    pub fn last_displayed_step(&self) -> u64 {
        self.last_displayed_step
    }

    /// Mark `snapshot` as displayed and update the step metrics.
    pub fn mark_snapshot_displayed(&mut self, snapshot: &Arc<RendererSnapshot>) {
        self.track_steps(snapshot.simulation_step, snapshot.world_generation);
        self.displayed_snapshot = Some(Arc::clone(snapshot));
    }

    /// Steps-per-frame metric for a displayed step of world `generation`.
    fn track_steps(&mut self, step: u64, generation: u64) {
        if generation != self.displayed_generation {
            // A new world: track its steps from here.
            self.displayed_generation = generation;
            self.last_displayed_step = step;
            return;
        }
        let steps_this_frame = step.saturating_sub(self.last_displayed_step);

        if steps_this_frame > 0 {
            // Exponential moving average for steps per frame
            if self.steps_per_frame == 0.0 {
                self.steps_per_frame = steps_this_frame as f32;
            } else {
                self.steps_per_frame = self.steps_per_frame * 0.9 + steps_this_frame as f32 * 0.1;
            }
            self.last_displayed_step = step;
        }
    }

    /// Increment render frame counter
    pub fn increment_render_count(&mut self) {
        self.render_fps += 1;
    }

    /// Update and get render FPS (call once per second)
    pub fn update_render_fps(&mut self) -> Option<f32> {
        if self.render_report_interval.elapsed().as_secs_f32() >= 1.0 {
            self.last_reported_render_fps =
                self.render_fps as f32 / self.render_report_interval.elapsed().as_secs_f32();
            self.render_fps = 0;
            self.render_report_interval = std::time::Instant::now();
            Some(self.last_reported_render_fps)
        } else {
            None
        }
    }

    /// Get current render FPS
    pub fn render_fps(&self) -> f32 {
        self.last_reported_render_fps
    }

    /// Get steps per frame metric
    pub fn steps_per_frame(&self) -> f32 {
        self.steps_per_frame
    }

    /// Get a clone of the shared metrics registry.
    pub fn metrics(&self) -> SharedMetricsRegistry {
        Arc::clone(&self.metrics)
    }

    /// Check if physics thread is running
    pub fn is_running(&self) -> bool {
        self.physics_thread.is_some()
    }

    /// Create a coordinator with no backing physics thread.
    ///
    /// `send_command` silently drops.  Useful for unit tests that only
    /// exercise action-dispatch or state-management logic.
    #[cfg(test)]
    pub fn new_disconnected() -> Self {
        Self {
            physics_thread: None,
            snapshot_slot: None,
            metrics: Arc::new(RwLock::new(MetricsRegistry::default())),
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_snapshot: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            recorded_commands: None,
        }
    }

    /// Create a disconnected coordinator that records every domain command sent.
    ///
    /// Returns the coordinator and a shared handle to the recorded command log.
    /// Each entry is the `Debug` representation of the `DomainCommand`.
    #[cfg(test)]
    pub fn new_recording() -> (Self, Arc<std::sync::Mutex<Vec<String>>>) {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let coordinator = Self {
            physics_thread: None,
            snapshot_slot: None,
            metrics: Arc::new(RwLock::new(MetricsRegistry::default())),
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_snapshot: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            recorded_commands: Some(Arc::clone(&log)),
        };
        (coordinator, log)
    }

    /// Publish `snapshot` as if the physics thread had (needs a coordinator
    /// made by [`Self::with_snapshot`]).
    #[cfg(test)]
    pub fn publish_for_test(&self, snapshot: RendererSnapshot) {
        self.snapshot_slot
            .as_ref()
            .expect("test coordinator has a snapshot slot")
            .store(Some(Arc::new(snapshot)));
    }

    /// Create a disconnected coordinator with a preloaded snapshot.
    #[cfg(test)]
    pub fn with_snapshot(snapshot: RendererSnapshot) -> Self {
        Self {
            physics_thread: None,
            snapshot_slot: Some(Arc::new(ArcSwapOption::new(Some(Arc::new(snapshot))))),
            metrics: Arc::new(RwLock::new(MetricsRegistry::default())),
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_snapshot: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            recorded_commands: None,
        }
    }
}

impl Drop for PhysicsCoordinator {
    fn drop(&mut self) {
        // Send quit command to physics thread
        // Note: PhysicsThread uses shutdown signal, not a Quit command
        // We need to call shutdown() on the physics thread
        if let Some(mut thread) = self.physics_thread.take() {
            thread.shutdown();
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steps_per_frame_ema() {
        let mut coord = PhysicsCoordinator {
            physics_thread: None,
            snapshot_slot: None,
            metrics: Arc::new(RwLock::new(MetricsRegistry::default())),
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_snapshot: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            recorded_commands: None,
        };

        // First step initializes
        coord.track_steps(10, 0);
        assert_eq!(coord.steps_per_frame, 10.0);

        // Subsequent steps use EMA
        coord.track_steps(20, 0);
        // 10 * 0.9 + 10 * 0.1 = 10.0
        assert!((coord.steps_per_frame - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_each_published_snapshot_is_new_once() {
        let mut coord = PhysicsCoordinator::with_snapshot(RendererSnapshot::new());
        assert!(coord.is_new_snapshot_available());
        let shown = coord.get_latest_snapshot().expect("snapshot");
        coord.mark_snapshot_displayed(&shown);
        assert!(!coord.is_new_snapshot_available(), "drawn once is enough");

        // Paused: same step, but the physics thread published an edit (or a
        // resized world). It is new, whatever its step.
        coord.publish_for_test(RendererSnapshot::new());
        assert!(coord.is_new_snapshot_available());
    }

    #[test]
    fn test_new_world_restarts_step_tracking() {
        let mut snapshot = RendererSnapshot::new();
        snapshot.simulation_step = 3;
        snapshot.world_generation = 1;
        let mut coord = PhysicsCoordinator::with_snapshot(snapshot);
        // The old world had run much further before the load.
        coord.last_displayed_step = 5000;

        let shown = coord.get_latest_snapshot().expect("snapshot");
        coord.mark_snapshot_displayed(&shown);
        assert_eq!(coord.last_displayed_step(), 3);

        // Counting up in the new world works as before.
        coord.track_steps(13, 1);
        assert_eq!(coord.last_displayed_step(), 13);
    }

    #[test]
    fn test_render_fps_update() {
        let mut coord = PhysicsCoordinator {
            physics_thread: None,
            snapshot_slot: None,
            metrics: Arc::new(RwLock::new(MetricsRegistry::default())),
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_snapshot: None,
            steps_per_frame: 0.0,
            render_fps: 60,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now() - std::time::Duration::from_secs(2),
            recorded_commands: None,
        };

        let fps = coord.update_render_fps();
        assert!(fps.is_some());
        assert!(fps.unwrap() > 0.0);
    }
}
