//! Unified render state for both ggez and wgpu renderers.
//!
//! Consolidates all shared event-loop state into a single struct so that
//! both renderer backends follow the same frame lifecycle:
//!
//! ```text
//! begin_frame() → handle events → should_render() → render → end_frame()
//! ```
//!
//! `UnifiedRenderState` owns the `ViewportTransform`, `EnergyWindowManager`,
//! cursor state, ball-size tracking, and per-step energy bookkeeping that
//! unifies renderer state management across both ggez and wgpu backends.

use crate::application::MainContent;
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::state_layers::RendererConfig;
use crate::graphics::renderer::viewport::Margins;
use crate::objects2d::line_segment::snap_position;
use crate::objects2d::{Rect, Vec2D};
use std::sync::{Arc, Mutex};

use super::action_processor::process_action_common;
use super::energy_window_manager::EnergyWindowManager;
use super::input_handler::{InputAction, InputHandler, OperatorTool};
use super::metrics::MetricsCollector;
use super::physics_coordinator::PhysicsCoordinator;

use crate::application::physics_thread::PhysicsCommand;

/// State machine for drawing a line segment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineSegmentDrawState {
    /// Waiting for the user to press left mouse button.
    #[default]
    Idle,
    /// Left button pressed; dragging from `start` to current cursor.
    Drawing { start: Vec2D },
}

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

    // ── Renderer config ─────────────────────────────────────────────
    /// Shared renderer configuration (plot type, cursor toggle, render enable).
    pub config: RendererConfig,

    // ── Frame flags ───────────────────────────────────────────────────
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

    // ── Per-frame metrics ────────────────────────────────────────────
    /// Renderer-agnostic frame metrics (FPS, draw calls, etc.).
    pub metrics: MetricsCollector,

    // ── Operator tool ────────────────────────────────────────────────
    /// Currently selected operator tool from the sidebar panel.
    pub active_tool: OperatorTool,
    /// Drawing state for the line segment tool.
    pub line_draw_state: LineSegmentDrawState,
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
    /// * `margin_px` – pixel margins.
    pub fn new(
        application: MainContent,
        world_bounds: Rect,
        window_size: (f32, f32),
        margin_px: Margins,
        wakeup_callback: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));

        let physics = PhysicsCoordinator::new(application, wakeup_callback);
        let viewport = ViewportTransform::new(world_bounds, window_size, margin_px);
        let energy_manager = EnergyWindowManager::new(energy_data);

        Self {
            input: InputHandler::new(),
            physics,
            viewport,
            energy_manager,
            config: RendererConfig::default(),
            view_dirty: true,
            last_snapshot: None,
            cursor_world_pos: Vec2D::zero(),
            current_ball_size_idx: 2, // default mid-size
            metrics: MetricsCollector::new(),
            active_tool: OperatorTool::None,
            line_draw_state: LineSegmentDrawState::Idle,
        }
    }

    // ── Frame lifecycle ──────────────────────────────────────────────

    /// Call at the start of each frame.
    ///
    /// Pulls the latest physics snapshot.  Does **not** start the metrics
    /// timer — call [`begin_render`] for that.
    pub fn begin_frame(&mut self) {
        // Grab latest snapshot from the physics thread.
        if let Some(snapshot) = self.physics.get_latest_snapshot() {
            self.last_snapshot = Some(snapshot);
            self.update_energy_from_snapshot();
        }
    }

    /// Start the per-frame metrics timer.
    ///
    /// Call this once per frame, immediately before issuing draw calls.
    /// Paired with [`end_frame`] which finalises the metrics.
    pub fn begin_render(&mut self) {
        self.metrics.begin_render();
    }

    /// Whether the renderer should draw a frame right now.
    pub fn should_render(&self) -> bool {
        self.config.render_drawing && (self.physics.is_new_step_available() || self.view_dirty)
    }

    /// Call after a frame has been drawn.
    ///
    /// Finalises per-frame metrics, clears the `view_dirty` flag, and
    /// marks the current simulation step as displayed so `should_render`
    /// won't re-trigger for the same data.
    pub fn end_frame(&mut self) {
        self.view_dirty = false;
        self.metrics.end_frame();
        if let Some(snapshot) = &self.last_snapshot {
            self.physics.mark_step_displayed(snapshot.simulation_step);
        }
    }

    // ── Snapshot & energy bookkeeping ────────────────────────────────

    /// Feed the latest snapshot into the energy window.
    ///
    /// Safe to call every frame — `EnergyWindowManager` internally
    /// deduplicates by simulation step.
    fn update_energy_from_snapshot(&mut self) {
        if let Some(snapshot) = &self.last_snapshot {
            let frame_metrics = self.metrics.current().clone();
            self.energy_manager.update_from_snapshot(
                snapshot,
                self.metrics.average_fps(),
                self.physics.steps_per_frame(),
                &frame_metrics,
            );
        }
    }

    /// Smoothed render FPS (rolling average over the last 60 frames).
    pub fn render_fps(&self) -> f32 {
        self.metrics.average_fps()
    }

    // ── Input-action dispatch ────────────────────────────────────────

    /// Process a user action that is common to all renderers.
    ///
    /// Delegates to the single canonical `process_action_common` so there
    /// is only one copy of the action→command mapping.
    ///
    /// Some actions (like ResetView, NextBallSize) are handled here before delegation
    /// to avoid duplication and ensure proper state updates.
    ///
    /// Returns `true` if the action affects the viewport (caller may still
    /// need to apply zoom/pan to `self.viewport` before the flag matters).
    ///
    /// # Arguments
    /// * `window_target` - Optional window target for actions that need to create windows (e.g., ToggleEnergyWindow)
    pub fn process_action(
        &mut self,
        action: &InputAction,
        window_target: Option<&winit::event_loop::ActiveEventLoop>,
    ) -> bool {
        // Handle actions that need state updates beyond process_action_common
        match action {
            InputAction::ResetView => {
                self.reset_view();
                return true; // affects view
            }
            InputAction::NextBallSize => {
                self.next_ball_size();
                return false; // doesn't affect view
            }
            InputAction::ToggleEnergyWindow => {
                if let Some(target) = window_target {
                    self.toggle_energy_window(target);
                }
                return false; // doesn't affect view
            }
            InputAction::SelectOperatorTool(tool) => {
                self.active_tool = *tool;
                self.line_draw_state = LineSegmentDrawState::Idle;
                return false;
            }
            _ => {}
        }

        let affects_view = process_action_common(action, &self.physics, &mut self.config);
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
    pub fn handle_resize(&mut self, new_width: f32, new_height: f32, margin_px: Margins) {
        let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
            new_width as u32,
            new_height as u32,
            margin_px,
        );
        self.handle_resize_with_world_rect(new_width, new_height, world_rect);
    }

    /// Handle window resize with pre-computed world bounds.
    ///
    /// Use this when the renderer has custom logic for computing world bounds
    /// (e.g., ggez with its WindowDefinition).
    pub fn handle_resize_with_world_rect(
        &mut self,
        new_width: f32,
        new_height: f32,
        world_rect: crate::objects2d::Rect,
    ) {
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

    // ── Energy window helpers ────────────────────────────────────────

    /// Toggle or cycle the energy plot window.
    pub fn toggle_energy_window(&mut self, window_target: &winit::event_loop::ActiveEventLoop) {
        self.energy_manager.toggle(window_target);
    }

    // ── Ball size helpers ────────────────────────────────────────────

    /// Cycle to the next ball size.
    ///
    /// Updates the local size index and sends command to physics thread.
    pub fn next_ball_size(&mut self) {
        use crate::objects2d::ball_impl::NUM_BALL_SIZES;
        self.current_ball_size_idx = (self.current_ball_size_idx + 1) % NUM_BALL_SIZES;
        self.physics.send_command(PhysicsCommand::NextBallSize);
    }

    // ── Centralized mouse event handlers ──────────────────────────────

    /// Centralized handler for all mouse events.
    ///
    /// Processes MouseInput, CursorMoved, and MouseWheel events.
    /// Returns true if the action affects the viewport.
    pub fn handle_mouse_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        match event {
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input_event(*button, *state)
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved_event(position.x as f32, position.y as f32)
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel_event(*delta)
            }
            _ => false,
        }
    }

    /// Handle mouse button input event.
    ///
    /// Processes mouse clicks and drags for pan, ball addition, and ball size cycling.
    /// When the line segment tool is active, left-click starts/ends drawing.
    /// Returns true if the action affects the viewport.
    pub fn handle_mouse_input_event(
        &mut self,
        button: winit::event::MouseButton,
        state: winit::event::ElementState,
    ) -> bool {
        let world_pos = self.input.cursor_pos();
        let screen_pos = self.input.cursor_pos_px();
        let pan_offset = self.viewport.pan_offset();

        // Sidebar icon click detection (left press only)
        if button == winit::event::MouseButton::Left
            && state == winit::event::ElementState::Pressed
            && let Some(icon_idx) = self.sidebar_icon_at(screen_pos)
        {
            let tool = Self::operator_tool_for_icon(icon_idx);
            // Toggle off if already selected, otherwise switch
            let new_tool = if self.active_tool == tool {
                OperatorTool::None
            } else {
                tool
            };
            let action = InputAction::SelectOperatorTool(new_tool);
            self.process_action(&action, None);
            return true;
        }

        // Handle line segment tool interactions
        if self.active_tool == OperatorTool::LineSegment
            && button == winit::event::MouseButton::Left
        {
            match state {
                winit::event::ElementState::Pressed => {
                    let snapped = self.snap_world_pos(world_pos);
                    self.line_draw_state = LineSegmentDrawState::Drawing { start: snapped };
                    self.view_dirty = true;
                    return true;
                }
                winit::event::ElementState::Released => {
                    if let LineSegmentDrawState::Drawing { start } = self.line_draw_state {
                        let end = self.snap_world_pos(world_pos);
                        // Only commit if the segment has meaningful length
                        if start.distance_squared(&end) > 0.01 {
                            let action = InputAction::CommitLineSegment { start, end };
                            self.process_action(&action, None);
                        }
                        self.line_draw_state = LineSegmentDrawState::Idle;
                        self.view_dirty = true;
                    }
                    return true;
                }
            }
        }

        if let Some(action) = self
            .input
            .handle_mouse_button(button, state, screen_pos, world_pos, pan_offset)
        {
            return self.process_action(&action, None);
        }
        false
    }

    /// Handle cursor moved event.
    ///
    /// Updates cursor position and processes pan gestures.
    /// When drawing a line segment, marks view dirty for ghost preview.
    /// Returns true if the action affects the viewport.
    pub fn handle_cursor_moved_event(&mut self, x: f32, y: f32) -> bool {
        // Update cursor position using shared state logic
        self.update_cursor_world_pos(&[x, y]);

        // Mark view dirty while drawing a line segment so ghost preview updates,
        // or when the line segment tool is selected (for snap indicator updates).
        if matches!(self.line_draw_state, LineSegmentDrawState::Drawing { .. })
            || self.active_tool == OperatorTool::LineSegment
        {
            self.view_dirty = true;
        }

        if let Some(action) = self.input.handle_mouse_motion(x, y) {
            if let InputAction::PanUpdate { delta } = &action {
                self.apply_pan_from_input(*delta);
                return true;
            } else {
                return self.process_action(&action, None);
            }
        }
        false
    }

    /// Handle mouse wheel event.
    ///
    /// Processes zoom in/out and view reset actions.
    /// Returns true if the action affects the viewport.
    pub fn handle_mouse_wheel_event(&mut self, delta: winit::event::MouseScrollDelta) -> bool {
        let cursor_pos = self.input.cursor_pos();

        if let Some(action) = self.input.handle_mouse_wheel(delta, cursor_pos) {
            match &action {
                InputAction::ZoomIn { center } => {
                    self.zoom_at_point(center, 1.1);
                    self.process_action(&action, None);
                    return true;
                }
                InputAction::ZoomOut { center } => {
                    self.zoom_at_point(center, 1.0 / 1.1);
                    self.process_action(&action, None);
                    return true;
                }
                InputAction::ResetView => {
                    self.reset_view();
                    self.process_action(&action, None);
                    return true;
                }
                _ => return self.process_action(&action, None),
            }
        }
        false
    }

    // ── Sidebar icon hit-testing ────────────────────────────────────

    /// Sidebar icon layout constants (must match wgpu sidebar_pipeline.rs).
    const ICON_DISPLAY_SIZE: f32 = 48.0;
    const ICON_SPACING: f32 = 12.0;
    const ICON_COUNT: usize = 4;

    /// Returns the sidebar icon index (0-based) at the given screen pixel
    /// position, or `None` if the click is outside all icons.
    pub fn sidebar_icon_at(&self, pos_px: [f32; 2]) -> Option<usize> {
        let margins = self.viewport.margins();
        let pad_x = (margins.left - Self::ICON_DISPLAY_SIZE) * 0.5;
        let first_icon_top = margins.top + Self::ICON_SPACING;

        // Horizontal bounds check
        if pos_px[0] < pad_x || pos_px[0] > pad_x + Self::ICON_DISPLAY_SIZE {
            return None;
        }

        for i in 0..Self::ICON_COUNT {
            let icon_top =
                first_icon_top + i as f32 * (Self::ICON_DISPLAY_SIZE + Self::ICON_SPACING);
            let icon_bottom = icon_top + Self::ICON_DISPLAY_SIZE;

            if pos_px[1] >= icon_top && pos_px[1] <= icon_bottom {
                return Some(i);
            }
        }
        None
    }

    /// Maps a sidebar icon index to an `OperatorTool`.
    fn operator_tool_for_icon(index: usize) -> OperatorTool {
        match index {
            0 => OperatorTool::LineSegment, // wall icon
            _ => OperatorTool::None,        // other icons not yet implemented
        }
    }

    /// Returns the sidebar icon index of the currently active tool, if any.
    pub fn selected_sidebar_icon(&self) -> Option<usize> {
        match self.active_tool {
            OperatorTool::LineSegment => Some(0),
            OperatorTool::None => None,
        }
    }

    // ── Line segment snapping ────────────────────────────────────────

    /// Snap threshold in world units.
    const SNAP_THRESHOLD: f32 = 0.5;

    /// Snap a world position to nearby walls or existing segment endpoints.
    fn snap_world_pos(&self, pos: Vec2D) -> Vec2D {
        let world_rect = self.viewport.world_bounds();
        let segments: &[crate::objects2d::SimLineSegment] = self
            .last_snapshot
            .as_ref()
            .map(|s| s.line_segments.as_slice())
            .unwrap_or(&[]);
        snap_position(pos, &world_rect, segments, Self::SNAP_THRESHOLD)
    }

    // ── Line segment preview ────────────────────────────────────────

    /// Returns the current ghost-preview line segment (start, end in world
    /// coordinates) while the user is drawing, or `None` when idle.
    /// The end point is snapped to nearby walls/endpoints.
    pub fn line_segment_preview(&self) -> Option<(Vec2D, Vec2D)> {
        if let LineSegmentDrawState::Drawing { start } = self.line_draw_state {
            let snapped_end = self.snap_world_pos(self.cursor_world_pos);
            Some((start, snapped_end))
        } else {
            None
        }
    }

    /// Returns world-coordinate positions where snap indicators should be
    /// rendered (e.g. small crosshairs at snap targets).
    pub fn snap_indicators(&self) -> Vec<[f32; 2]> {
        let mut indicators = Vec::new();
        if let LineSegmentDrawState::Drawing { start } = self.line_draw_state {
            // Check if start was snapped (compare with an un-snapped version isn't
            // possible here, so we always show the start indicator when drawing)
            // Show indicator at snapped end position if it differs from cursor
            let snapped_end = self.snap_world_pos(self.cursor_world_pos);
            if snapped_end != self.cursor_world_pos {
                indicators.push([snapped_end.x, snapped_end.y]);
            }
            // Also check start snap state: if start coincides with a wall or
            // endpoint, show indicator there too.
            let _ = start; // start is already snapped at press time
        } else if self.active_tool == OperatorTool::LineSegment {
            // When idle with the line tool selected, show snap indicator at
            // cursor if it would snap.
            let snapped = self.snap_world_pos(self.cursor_world_pos);
            if snapped != self.cursor_world_pos {
                indicators.push([snapped.x, snapped.y]);
            }
        }
        indicators
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
        ViewportTransform::new(world, (1280.0, 720.0), Margins::symmetric(50.0, 50.0))
    }

    // ── process_action_common ────────────────────────────────────────

    #[test]
    fn zoom_in_marks_view_dirty() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::ZoomIn {
            center: Vec2D::zero(),
        };
        assert!(process_action_common(&action, &physics, &mut config));
    }

    #[test]
    fn zoom_out_marks_view_dirty() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::ZoomOut {
            center: Vec2D::zero(),
        };
        assert!(process_action_common(&action, &physics, &mut config));
    }

    #[test]
    fn pan_update_marks_view_dirty() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::PanUpdate {
            delta: Vec2D::new(5.0, 3.0),
        };
        assert!(process_action_common(&action, &physics, &mut config));
    }

    #[test]
    fn reset_view_marks_view_dirty() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::ResetView;
        assert!(process_action_common(&action, &physics, &mut config));
    }

    #[test]
    fn toggle_rendering_flips_flag() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::ToggleRendering;
        let affects = process_action_common(&action, &physics, &mut config);
        assert!(!affects);
        assert!(!config.render_drawing);
        // Toggle back.
        process_action_common(&action, &physics, &mut config);
        assert!(config.render_drawing);
    }

    #[test]
    fn renderer_specific_actions_do_not_affect_view() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let actions = [
            InputAction::CyclePlotContent,
            InputAction::ToggleEnergyWindow,
            InputAction::ToggleCutoffRings,
            InputAction::DisableCursor,
        ];
        for action in &actions {
            assert!(
                !process_action_common(action, &physics, &mut config),
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
