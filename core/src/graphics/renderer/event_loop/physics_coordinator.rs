use crate::application::MainContent;
use crate::application::domain_command::DomainCommand;
use crate::application::physics_thread::{PhysicsCommand, PhysicsThread};
use crate::application::renderer_snapshot::RendererSnapshot;
use arc_swap::ArcSwapOption;
use std::sync::Arc;

/// Coordinates communication between render thread and physics thread
pub struct PhysicsCoordinator {
    physics_thread: Option<PhysicsThread>,
    snapshot_slot: Option<Arc<ArcSwapOption<RendererSnapshot>>>,

    // Step tracking
    last_displayed_step: u64,
    steps_per_frame: f32,

    // Performance metrics
    render_fps: u32,
    last_reported_render_fps: f32,
    render_report_interval: std::time::Instant,
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
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
        }
    }

    /// Send command to physics thread
    pub fn send_command(&self, command: PhysicsCommand) {
        if let Some(thread) = self.physics_thread.as_ref() {
            let _ = thread.send_command(command);
        }
    }

    /// Send a domain-level command through the existing runtime adapter.
    pub fn send_domain_command(&self, command: DomainCommand) {
        self.send_command(command.into());
    }

    /// Get latest snapshot if available
    /// Returns None if no snapshot slot or no snapshot published yet
    pub fn get_latest_snapshot(&self) -> Option<Arc<RendererSnapshot>> {
        let slot = self.snapshot_slot.as_ref()?;
        let guard = slot.load();
        guard.as_ref().map(Arc::clone)
    }

    /// Check if a new simulation step is available
    pub fn is_new_step_available(&self) -> bool {
        if let Some(snapshot) = self.get_latest_snapshot() {
            snapshot.simulation_step > self.last_displayed_step
        } else {
            false
        }
    }

    /// Get the last displayed step number
    pub fn last_displayed_step(&self) -> u64 {
        self.last_displayed_step
    }

    /// Mark a step as displayed and update metrics
    pub fn mark_step_displayed(&mut self, step: u64) {
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
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
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
            steps_per_frame: 0.0,
            render_fps: 0,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now(),
        };

        // First step initializes
        coord.mark_step_displayed(10);
        assert_eq!(coord.steps_per_frame, 10.0);

        // Subsequent steps use EMA
        coord.mark_step_displayed(20);
        // 10 * 0.9 + 10 * 0.1 = 10.0
        assert!((coord.steps_per_frame - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_render_fps_update() {
        let mut coord = PhysicsCoordinator {
            physics_thread: None,
            snapshot_slot: None,
            last_displayed_step: 0,
            steps_per_frame: 0.0,
            render_fps: 60,
            last_reported_render_fps: 0.0,
            render_report_interval: std::time::Instant::now() - std::time::Duration::from_secs(2),
        };

        let fps = coord.update_render_fps();
        assert!(fps.is_some());
        assert!(fps.unwrap() > 0.0);
    }
}
