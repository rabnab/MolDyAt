use crate::application::renderer_snapshot::RendererSnapshot;
use crate::application::{DomainCommand, MainContent, PhysicsEvent, PhysicsThread};
use crate::objects2d::Rect;
use arc_swap::ArcSwapOption;
use std::sync::Arc;

/// Coordinates communication between render thread and physics thread
pub struct PhysicsCoordinator {
    physics_thread: Option<PhysicsThread>,
    snapshot_slot: Option<Arc<ArcSwapOption<RendererSnapshot>>>,

    // Step tracking
    last_displayed_step: u64,
    /// `world_generation` of the last displayed snapshot.
    displayed_generation: u64,
    /// World bounds of the last displayed snapshot.
    displayed_world_bounds: Option<Rect>,
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
        // Get snapshot slot before moving MainContent
        let snapshot_slot = application.get_snapshot_slot();

        // Spawn physics thread
        let physics_thread = PhysicsThread::spawn(application, wakeup_callback);
        Self {
            physics_thread: Some(physics_thread),
            snapshot_slot: Some(snapshot_slot),
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_world_bounds: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            #[cfg(test)]
            recorded_commands: None,
        }
    }

    /// Send a domain-level command to the physics thread.
    pub fn send_command(&self, command: DomainCommand) {
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

    /// Check if a new simulation step is available: a later step, any step
    /// of a new world (after a load, whose step count starts again), or the
    /// same step of a resized world (the simulation is paused while the
    /// window is resized, so the step does not advance).
    pub fn is_new_step_available(&self) -> bool {
        if let Some(snapshot) = self.get_latest_snapshot() {
            snapshot.world_generation != self.displayed_generation
                || snapshot.simulation_step > self.last_displayed_step
                || self.displayed_world_bounds != Some(snapshot.cell_grid_bounds)
        } else {
            false
        }
    }

    /// Get the last displayed step number
    pub fn last_displayed_step(&self) -> u64 {
        self.last_displayed_step
    }

    /// Mark a step of world `generation`, with bounds `world_bounds`, as
    /// displayed and update metrics.
    pub fn mark_step_displayed(&mut self, step: u64, generation: u64, world_bounds: Rect) {
        self.displayed_world_bounds = Some(world_bounds);
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
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_world_bounds: None,
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
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_world_bounds: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            recorded_commands: Some(Arc::clone(&log)),
        };
        (coordinator, log)
    }

    /// Create a disconnected coordinator with a preloaded snapshot.
    #[cfg(test)]
    pub fn with_snapshot(snapshot: RendererSnapshot) -> Self {
        Self {
            physics_thread: None,
            snapshot_slot: Some(Arc::new(ArcSwapOption::new(Some(Arc::new(snapshot))))),
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_world_bounds: None,
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
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_world_bounds: None,
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
            recorded_commands: None,
        };

        let bounds = Rect::new(0.0, 0.0, 100.0, -100.0);

        // First step initializes
        coord.mark_step_displayed(10, 0, bounds);
        assert_eq!(coord.steps_per_frame, 10.0);

        // Subsequent steps use EMA
        coord.mark_step_displayed(20, 0, bounds);
        // 10 * 0.9 + 10 * 0.1 = 10.0
        assert!((coord.steps_per_frame - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_new_world_keeps_rendering_after_load() {
        let mut snapshot = RendererSnapshot::new();
        snapshot.simulation_step = 3;
        snapshot.world_generation = 1;
        let bounds = snapshot.cell_grid_bounds;
        let mut coord = PhysicsCoordinator::with_snapshot(snapshot);
        // The old world had run much further before the load.
        coord.last_displayed_step = 5000;

        assert!(coord.is_new_step_available(), "a new world must render");
        coord.mark_step_displayed(3, 1, bounds);
        assert_eq!(coord.last_displayed_step(), 3);
        assert!(!coord.is_new_step_available(), "same step is not new");

        // Counting up in the new world works as before.
        coord.mark_step_displayed(13, 1, bounds);
        assert_eq!(coord.last_displayed_step(), 13);
    }

    fn snapshot_at(step: u64, world_bounds: Rect) -> RendererSnapshot {
        let mut snapshot = RendererSnapshot::new();
        snapshot.simulation_step = step;
        snapshot.cell_grid_bounds = world_bounds;
        snapshot
    }

    #[test]
    fn test_resized_world_renders_at_the_same_step() {
        let old = Rect::new(0.0, 0.0, 100.0, -100.0);
        let resized = Rect::new(0.0, 0.0, 150.0, -100.0);
        let mut coord = PhysicsCoordinator::with_snapshot(snapshot_at(7, old));
        coord.mark_step_displayed(7, 0, old);
        assert!(!coord.is_new_step_available());

        // Paused (also while the window is resized): same step and world,
        // but the snapshot shows the resized world. A redraw before it
        // arrived may already have used up `view_dirty`.
        coord
            .snapshot_slot
            .as_ref()
            .expect("test coordinator has a snapshot slot")
            .store(Some(Arc::new(snapshot_at(7, resized))));
        assert!(coord.is_new_step_available(), "a resized world must render");

        coord.mark_step_displayed(7, 0, resized);
        assert!(!coord.is_new_step_available(), "drawn once is enough");
    }

    #[test]
    fn test_render_fps_update() {
        let mut coord = PhysicsCoordinator {
            physics_thread: None,
            snapshot_slot: None,
            last_displayed_step: 0,
            displayed_generation: 0,
            displayed_world_bounds: None,
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
