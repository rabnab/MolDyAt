//! Unified render state for both ggez and wgpu renderers.
//!
//! Consolidates all shared event-loop state into a single struct so that
//! both renderer backends follow the same frame lifecycle:
//!
//! ```text
//! begin_frame() → handle events → should_render() → render → end_frame()
//! ```
//!
//! # Migrating from `EventLoopState`
//!
//! `UnifiedRenderState` is a superset of `EventLoopState`.  It additionally
//! owns the `ViewportTransform`, `EnergyWindowManager`, cursor state, ball-size
//! tracking, and per-step energy bookkeeping that were previously scattered
//! across renderer-specific locals.

use crate::application::MainContent;
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::objects2d::{Rect, Vec2D};
use std::sync::{Arc, Mutex};

use super::energy_window_manager::EnergyWindowManager;
use super::event_loop_state::process_action_common;
use super::input_handler::{InputAction, InputHandler};
use super::physics_coordinator::PhysicsCoordinator;

use crate::application::physics_thread::PhysicsCommand;

/// Unified state shared by every renderer backend.
///
/// Owns input handling, physics coordination, viewport transformation,
/// energy-window management, and per-frame bookkeeping.  Renderer-specific
/// code (meshes, pipelines, GPU resources) stays outside this struct.
pub struct UnifiedRenderState {
    // ── Input ──────────────────────────────────────────────────────────
    pub input: InputHandler,

    // ── Physics thread bridge ──────────────────────────────────────────
    pub physics: PhysicsCoordinator,

    // ── Viewport ──────────────────────────────────────────────────────
    pub viewport: ViewportTransform,

    // ── Energy window ─────────────────────────────────────────────────
    pub energy_manager: EnergyWindowManager,

    // ── Frame flags ───────────────────────────────────────────────────
    /// Whether rendering is enabled (toggled by F2).
    pub render_drawing: bool,
    /// Set whenever the viewport changes; cleared after a frame is drawn.
    pub view_dirty: bool,

    // ── Snapshot bookkeeping ──────────────────────────────────────────
    /// The most recent snapshot handed off by the physics thread.
    pub last_snapshot: Option<Arc<RendererSnapshot>>,

    // ── Cursor / ball-size ────────────────────────────────────────────
    /// Current cursor position in world coordinates.
    pub cursor_world_pos: Vec2D,
    /// Active ball-size index (cycled with right-click).
    pub current_ball_size_idx: usize,

    // ── Render-FPS tracking ──────────────────────────────────────────
    render_frame_count: u64,
    render_fps_timer: std::time::Instant,
    render_fps: f32,
}

impl UnifiedRenderState {
    /// Create a new unified state.
    ///
    /// This spawns the physics thread (via `PhysicsCoordinator`).
    ///
    /// # Arguments
    /// * `application` – moved into the physics thread.
    /// * `world_bounds` – initial world rectangle for `ViewportTransform`.
    /// * `window_size` – initial window size in pixels `(width, height)`.
    /// * `margin_px` – pixel margins `(x, y)`.
    pub fn new(
        application: MainContent,
        world_bounds: Rect,
        window_size: (f32, f32),
        margin_px: (f32, f32),
    ) -> Self {
        let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));

        let physics = PhysicsCoordinator::new(application);
        let viewport = ViewportTransform::new(world_bounds, window_size, margin_px);
        let energy_manager = EnergyWindowManager::new(energy_data);

        Self {
            input: InputHandler::new(),
            physics,
            viewport,
            energy_manager,
            render_drawing: true,
            view_dirty: true,
            last_snapshot: None,
            cursor_world_pos: Vec2D::zero(),
            current_ball_size_idx: 2, // default mid-size
            render_frame_count: 0,
            render_fps_timer: std::time::Instant::now(),
            render_fps: 0.0,
        }
    }

    // ── Frame lifecycle ──────────────────────────────────────────────

    /// Call at the start of each frame.
    ///
    /// Pulls the latest physics snapshot and updates render-FPS tracking.
    pub fn begin_frame(&mut self) {
        // Grab latest snapshot from the physics thread.
        if let Some(snapshot) = self.physics.get_latest_snapshot() {
            self.last_snapshot = Some(snapshot);
        }

        // Render-FPS counter.
        self.render_frame_count += 1;
        let elapsed = self.render_fps_timer.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.render_fps = self.render_frame_count as f32 / elapsed;
            self.render_frame_count = 0;
            self.render_fps_timer = std::time::Instant::now();
        }
    }

    /// Whether the renderer should draw a frame right now.
    pub fn should_render(&self) -> bool {
        self.render_drawing && (self.physics.is_new_step_available() || self.view_dirty)
    }

    /// Call after a frame has been drawn.
    ///
    /// Clears the `view_dirty` flag and marks the current simulation step
    /// as displayed so `should_render` won't re-trigger for the same data.
    pub fn end_frame(&mut self) {
        self.view_dirty = false;
        if let Some(snapshot) = &self.last_snapshot {
            self.physics.mark_step_displayed(snapshot.simulation_step);
        }
    }

    // ── Snapshot & energy bookkeeping ────────────────────────────────

    /// Feed the latest snapshot into the energy window.
    ///
    /// Safe to call every frame — `EnergyWindowManager` internally
    /// deduplicates by simulation step.
    pub fn update_energy_from_snapshot(&mut self) {
        if let Some(snapshot) = &self.last_snapshot {
            self.energy_manager.update_from_snapshot(
                snapshot,
                self.render_fps,
                self.physics.steps_per_frame(),
            );
        }
    }

    /// Current render FPS (updated once per second).
    pub fn render_fps(&self) -> f32 {
        self.render_fps
    }

    // ── Input-action dispatch ────────────────────────────────────────

    /// Process a user action that is common to all renderers.
    ///
    /// Delegates to the single canonical `process_action_common` so there
    /// is only one copy of the action→command mapping.
    ///
    /// Returns `true` if the action affects the viewport (caller may still
    /// need to apply zoom/pan to `self.viewport` before the flag matters).
    pub fn process_action(&mut self, action: &InputAction) -> bool {
        let affects_view = process_action_common(action, &self.physics, &mut self.render_drawing);
        if affects_view {
            self.view_dirty = true;
        }
        affects_view
    }

    // ── Viewport helpers ─────────────────────────────────────────────

    /// Zoom the viewport so that `world_point` stays fixed on screen.
    pub fn zoom_at_point(&mut self, world_point: &Vec2D, factor: f32) {
        self.viewport.zoom_at_point(world_point, factor);
        self.view_dirty = true;
    }

    /// Reset viewport to default zoom and pan.
    pub fn reset_view(&mut self) {
        self.viewport.reset();
        self.view_dirty = true;
    }

    /// Handle a window resize.
    ///
    /// Updates the viewport **and** notifies the physics thread so the
    /// simulation world matches the new window dimensions.
    pub fn handle_resize(&mut self, new_width: f32, new_height: f32, margin_px: (f32, f32)) {
        let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
            new_width as u32,
            new_height as u32,
            margin_px,
        );
        self.physics
            .send_command(PhysicsCommand::Resize { world_rect });
        self.viewport.set_world_bounds(world_rect);
        self.viewport.on_window_resize((new_width, new_height));
        self.view_dirty = true;
    }

    /// Apply a pan delta (in pixels) originating from a drag gesture.
    pub fn apply_pan_from_input(&mut self, delta: Vec2D) {
        let start_offset = self.input.pan_start_offset();
        let zoom = self.viewport.zoom();
        let new_pan = ViewportTransform::apply_pan_delta(delta, start_offset, zoom);
        self.viewport.pan(new_pan);
        self.view_dirty = true;
    }

    /// Convert a screen-pixel position to world coordinates and update the
    /// stored cursor position.
    pub fn update_cursor_world_pos(&mut self, px: &[f32; 2]) {
        let world = self.viewport.px_to_world(px);
        self.cursor_world_pos = world;
        self.input.set_cursor_pos(world, *px);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────

    /// Build a minimal `UnifiedRenderState` without spawning a real
    /// physics thread (which needs a full `MainContent`).  We test the
    /// pure-logic helpers here; integration tests cover the full path.
    fn dummy_viewport() -> ViewportTransform {
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        ViewportTransform::new(world, (1280.0, 720.0), (50.0, 50.0))
    }

    // ── process_action_common ────────────────────────────────────────

    #[test]
    fn zoom_in_marks_view_dirty() {
        let mut drawing = true;
        let physics = make_disconnected_coordinator();
        let action = InputAction::ZoomIn {
            center: Vec2D::zero(),
        };
        assert!(process_action_common(&action, &physics, &mut drawing));
    }

    #[test]
    fn zoom_out_marks_view_dirty() {
        let mut drawing = true;
        let physics = make_disconnected_coordinator();
        let action = InputAction::ZoomOut {
            center: Vec2D::zero(),
        };
        assert!(process_action_common(&action, &physics, &mut drawing));
    }

    #[test]
    fn pan_update_marks_view_dirty() {
        let mut drawing = true;
        let physics = make_disconnected_coordinator();
        let action = InputAction::PanUpdate {
            delta: Vec2D::new(5.0, 3.0),
        };
        assert!(process_action_common(&action, &physics, &mut drawing));
    }

    #[test]
    fn reset_view_marks_view_dirty() {
        let mut drawing = true;
        let physics = make_disconnected_coordinator();
        let action = InputAction::ResetView;
        assert!(process_action_common(&action, &physics, &mut drawing));
    }

    #[test]
    fn toggle_rendering_flips_flag() {
        let mut drawing = true;
        let physics = make_disconnected_coordinator();
        let action = InputAction::ToggleRendering;
        let affects = process_action_common(&action, &physics, &mut drawing);
        assert!(!affects);
        assert!(!drawing);
        // Toggle back.
        process_action_common(&action, &physics, &mut drawing);
        assert!(drawing);
    }

    #[test]
    fn renderer_specific_actions_do_not_affect_view() {
        let mut drawing = true;
        let physics = make_disconnected_coordinator();
        let actions = [
            InputAction::CyclePlotContent,
            InputAction::ToggleEnergyWindow,
            InputAction::ToggleCutoffRings,
            InputAction::DisableCursor,
        ];
        for action in &actions {
            assert!(
                !process_action_common(action, &physics, &mut drawing),
                "action {:?} should not affect view",
                action
            );
        }
    }

    // ── Viewport helper tests ───────────────────────────────────────

    #[test]
    fn zoom_at_point_marks_dirty() {
        let mut vp = dummy_viewport();
        let initial_zoom = vp.zoom();
        vp.zoom_at_point(&Vec2D::zero(), 2.0);
        assert!((vp.zoom() - initial_zoom * 2.0).abs() < 0.01);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut vp = dummy_viewport();
        vp.zoom_at_point(&Vec2D::zero(), 5.0);
        vp.reset();
        assert!((vp.zoom() - 1.0).abs() < 0.01);
        assert!(vp.pan_offset().norm_sq() < 0.01);
    }

    // ── Stub coordinator (no real physics thread) ───────────────────

    /// A `PhysicsCoordinator` whose command channel is disconnected so
    /// that `send_command` silently drops.  Good enough for unit tests
    /// that only exercise action-dispatch logic.
    fn make_disconnected_coordinator() -> PhysicsCoordinator {
        PhysicsCoordinator::new_disconnected()
    }
}
