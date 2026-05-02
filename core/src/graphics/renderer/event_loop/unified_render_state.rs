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
use crate::application::renderer_snapshot::{OperatorSnapshot, RendererSnapshot};
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::graphics::renderer::state_layers::RendererConfig;
use crate::objects2d::{Rect, Vec2D};
use std::sync::{Arc, Mutex};

use super::action_processor::process_action_common;
use super::energy_window_manager::EnergyWindowManager;
use super::input_handler::{InputAction, InputHandler, OperatorTool};
use super::line_segment_tool::LineSegmentToolHandler;
use super::metrics::MetricsCollector;
use super::operator_tool_handler::{
    OperatorToolHandler, ToolContext, ToolPreview, scroll_y_from_delta,
};
use super::physics_coordinator::PhysicsCoordinator;
use super::source_sphere_tool::SourceSphereToolHandler;
use super::thermostat_tool::ThermostatToolHandler;
use super::valve_segment_tool::ValveSegmentToolHandler;

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
    /// Active tool handler (trait dispatch).  `None` when no tool is selected.
    pub tool_handler: Option<Box<dyn OperatorToolHandler>>,
    /// Default empty operator snapshot, used when no physics snapshot is
    /// available yet.  Avoids per-frame heap allocation and `LazyLock`.
    pub(super) default_operator_snapshot: OperatorSnapshot,

    // ── Resize tracking ───────────────────────────────────────────────
    /// Set when a window resize occurs; cleared when any mouse button is released.
    /// While set, the simulation is paused; releasing the mouse unpauses it.
    pub(super) resize_pending: bool,
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
            tool_handler: None,
            default_operator_snapshot: OperatorSnapshot::default(),
            resize_pending: false,
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

    /// Collect all operator preview geometry for the current frame.
    ///
    /// Delegates to the active [`OperatorToolHandler`] for preview and
    /// snap indicators, then bundles them with the selected sidebar icon.
    pub fn operator_previews(&self) -> super::FramePreviews {
        let ctx = self.make_tool_context();

        let (line_segment, valve_segment, snap_indicators) =
            if let Some(handler) = &self.tool_handler {
                let preview = handler.preview(&ctx);
                let snaps = handler.snap_indicators(&ctx);
                match preview {
                    ToolPreview::LineSegment { start, end } => (Some((start, end)), None, snaps),
                    ToolPreview::ValveSegment {
                        line_id,
                        t_start,
                        t_end,
                    } => (None, Some((line_id, t_start, t_end)), snaps),
                    ToolPreview::None => (None, None, snaps),
                }
            } else {
                (None, None, Vec::new())
            };

        super::FramePreviews {
            line_segment,
            valve_segment,
            snap_indicators,
            selected_sidebar_icon: self.selected_sidebar_icon(),
        }
    }

    /// Build a [`ToolContext`] from the current snapshot.
    ///
    /// The returned context borrows `snapshot` (if `Some`) or the provided
    /// `fallback` default.  Callers that also need `&mut self.tool_handler`
    /// should invoke this with explicit field borrows to avoid conflicting
    /// `&self` / `&mut self` borrows.
    fn make_tool_context_from<'a>(
        snapshot: Option<&'a RendererSnapshot>,
        fallback: &'a OperatorSnapshot,
        viewport: &crate::graphics::renderer::ViewportTransform,
        cursor_world_pos: Vec2D,
    ) -> ToolContext<'a> {
        let operators = snapshot.map(|s| &s.operators).unwrap_or(fallback);
        ToolContext {
            operators,
            world_bounds: viewport.world_bounds(),
            cursor_world_pos,
        }
    }

    /// Convenience wrapper when no mutable self-borrow is needed.
    fn make_tool_context(&self) -> ToolContext<'_> {
        Self::make_tool_context_from(
            self.last_snapshot.as_deref(),
            &self.default_operator_snapshot,
            &self.viewport,
            self.cursor_world_pos,
        )
    }

    /// Create a handler for the given tool, or `None` if no tool selected.
    fn handler_for_tool(tool: OperatorTool) -> Option<Box<dyn OperatorToolHandler>> {
        match tool {
            OperatorTool::Ball => None,
            OperatorTool::LineSegment => Some(Box::new(LineSegmentToolHandler::new())),
            OperatorTool::ValveSegment => Some(Box::new(ValveSegmentToolHandler::new())),
            OperatorTool::SourceSphere => Some(Box::new(SourceSphereToolHandler::new())),
            OperatorTool::Thermostat => Some(Box::new(ThermostatToolHandler::new())),
            OperatorTool::None => None,
        }
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
        action: InputAction,
        window_target: Option<&winit::event_loop::ActiveEventLoop>,
        main_window: Option<&winit::window::Window>,
    ) -> bool {
        match action {
            InputAction::ResetView => {
                self.reset_view();
                true
            }
            InputAction::NextBallSize => {
                self.next_ball_size();
                if self.active_tool == OperatorTool::Ball {
                    self.view_dirty = true;
                }
                false
            }
            InputAction::ToggleEnergyWindow => {
                if let Some(target) = window_target {
                    self.toggle_energy_window(target, main_window);
                }
                false
            }
            InputAction::SelectOperatorTool(tool) => {
                if let Some(handler) = self.tool_handler.as_mut() {
                    handler.reset();
                }
                self.active_tool = tool;
                self.tool_handler = Self::handler_for_tool(tool);
                self.config.draw_cursor = tool == OperatorTool::Ball;
                self.view_dirty = true;
                false
            }
            InputAction::AddBall { position } => {
                // Suppress ball placement when the cursor is near an operator
                // so Ctrl-release doesn't accidentally create a ball on top of
                // a line segment, valve, or source sphere.
                let operators = self
                    .last_snapshot
                    .as_deref()
                    .map(|s| &s.operators)
                    .unwrap_or(&self.default_operator_snapshot);
                if operators.is_near_any_operator(position) {
                    return false;
                }
                process_action_common(
                    InputAction::AddBall { position },
                    &self.physics,
                    &mut self.config,
                );
                false
            }
            other => {
                let affects_view = process_action_common(other, &self.physics, &mut self.config);
                if affects_view {
                    self.view_dirty = true;
                }
                affects_view
            }
        }
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
        self.viewport.set_margins(margin_px);
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
        self.resize_pending = true;
    }

    /// Resume physics after resize if a resize completion is pending.
    pub fn resume_after_resize_if_pending(&mut self) {
        if self.resize_pending {
            self.resize_pending = false;
            self.physics.send_command(PhysicsCommand::ResumeAfterResize);
        }
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
    pub fn toggle_energy_window(
        &mut self,
        window_target: &winit::event_loop::ActiveEventLoop,
        main_window: Option<&winit::window::Window>,
    ) {
        self.energy_manager.toggle(window_target, main_window);
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
    /// Dispatches to the active [`OperatorToolHandler`] for operator
    /// interactions, then falls through to pan/ball/size-cycle input.
    /// Returns true if the action affects the viewport.
    pub fn handle_mouse_input_event(
        &mut self,
        button: winit::event::MouseButton,
        state: winit::event::ElementState,
    ) -> bool {
        // On any mouse button release, check if we are waiting to resume after resize.
        if state == winit::event::ElementState::Released {
            self.resume_after_resize_if_pending();
        }

        let world_pos = self.input.cursor_pos();
        let screen_pos = self.input.cursor_pos_px();
        let pan_offset = self.viewport.pan_offset();

        // Sidebar icon click detection (left press only)
        if button == winit::event::MouseButton::Left
            && state == winit::event::ElementState::Pressed
            && let Some(icon_idx) = self.sidebar_icon_at(screen_pos)
        {
            let tool = Self::operator_tool_for_icon(icon_idx);
            let new_tool = if self.active_tool == tool {
                OperatorTool::None
            } else {
                tool
            };
            let action = InputAction::SelectOperatorTool(new_tool);
            self.process_action(action, None, None);
            return true;
        }

        // Ball operator tool: left release places one ball at cursor.
        if self.active_tool == OperatorTool::Ball
            && button == winit::event::MouseButton::Left
            && state == winit::event::ElementState::Released
        {
            return self.process_action(
                InputAction::AddBall {
                    position: world_pos,
                },
                None,
                None,
            );
        }

        // Delegate to active tool handler
        if let Some(handler) = &mut self.tool_handler {
            let ctx = Self::make_tool_context_from(
                self.last_snapshot.as_deref(),
                &self.default_operator_snapshot,
                &self.viewport,
                self.cursor_world_pos,
            );
            let resp = handler.handle_mouse_button(button, state, world_pos, &ctx);
            if resp.consumed {
                if let Some(cmd) = resp.action {
                    let action = InputAction::OperatorAction(cmd);
                    self.process_action(action, None, None);
                }
                if resp.view_dirty {
                    self.view_dirty = true;
                }
                return true;
            }
        }

        if let Some(action) = self
            .input
            .handle_mouse_button(button, state, screen_pos, world_pos, pan_offset)
        {
            return self.process_action(action, None, None);
        }
        false
    }

    /// Handle cursor moved event.
    ///
    /// Updates cursor position and processes pan gestures.
    /// When an operator tool is active, marks view dirty so previews and
    /// snap indicators update continuously.
    /// Returns true if the action affects the viewport.
    pub fn handle_cursor_moved_event(&mut self, x: f32, y: f32) -> bool {
        // Update cursor position using shared state logic
        self.update_cursor_world_pos(&[x, y]);

        // Delegate to active tool handler.
        if let Some(handler) = &mut self.tool_handler {
            let world_pos = self.input.cursor_pos();
            let ctx = Self::make_tool_context_from(
                self.last_snapshot.as_deref(),
                &self.default_operator_snapshot,
                &self.viewport,
                self.cursor_world_pos,
            );
            let resp = handler.handle_cursor_move(world_pos, &ctx);
            if resp.consumed {
                if let Some(cmd) = resp.action {
                    let action = InputAction::OperatorAction(cmd);
                    self.process_action(action, None, None);
                }
                if resp.view_dirty {
                    self.view_dirty = true;
                }
            }
        }

        if let Some(action) = self.input.handle_mouse_motion(x, y) {
            if let InputAction::PanUpdate { delta } = &action {
                self.apply_pan_from_input(*delta);
                return true;
            } else {
                return self.process_action(action, None, None);
            }
        }
        false
    }

    /// Handle mouse wheel event.
    ///
    /// Delegates to the active tool handler first. If the tool consumes
    /// the scroll (e.g. source sphere speed/period adjustment), the
    /// event stops. Otherwise falls through to zoom.
    /// Returns true if the action affects the viewport.
    pub fn handle_mouse_wheel_event(&mut self, delta: winit::event::MouseScrollDelta) -> bool {
        let cursor_pos = self.input.cursor_pos();

        // Delegate to active tool handler
        if let Some(handler) = &mut self.tool_handler {
            let scroll_y = scroll_y_from_delta(delta);
            if scroll_y.abs() > f32::EPSILON {
                let ctrl_held = self
                    .input
                    .is_key_pressed(winit::keyboard::KeyCode::ControlLeft)
                    || self
                        .input
                        .is_key_pressed(winit::keyboard::KeyCode::ControlRight);
                let fallback = &self.default_operator_snapshot;
                let ctx = Self::make_tool_context_from(
                    self.last_snapshot.as_deref(),
                    fallback,
                    &self.viewport,
                    self.cursor_world_pos,
                );
                let resp = handler.handle_scroll(scroll_y, cursor_pos, ctrl_held, &ctx);
                if resp.consumed {
                    if let Some(cmd) = resp.action {
                        let action = InputAction::OperatorAction(cmd);
                        self.process_action(action, None, None);
                    }
                    if resp.view_dirty {
                        self.view_dirty = true;
                    }
                    return true;
                }
            }
        }

        if let Some(action) = self.input.handle_mouse_wheel(delta, cursor_pos) {
            match &action {
                InputAction::ZoomIn { center } => {
                    self.zoom_at_point(center, 1.1);
                    self.process_action(action, None, None);
                    return true;
                }
                InputAction::ZoomOut { center } => {
                    self.zoom_at_point(center, 1.0 / 1.1);
                    self.process_action(action, None, None);
                    return true;
                }
                InputAction::ResetView => {
                    self.reset_view();
                    self.process_action(action, None, None);
                    return true;
                }
                _ => return self.process_action(action, None, None),
            }
        }
        false
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
        assert!(process_action_common(action, &physics, &mut config));
    }

    #[test]
    fn zoom_out_marks_view_dirty() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::ZoomOut {
            center: Vec2D::zero(),
        };
        assert!(process_action_common(action, &physics, &mut config));
    }

    #[test]
    fn pan_update_marks_view_dirty() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::PanUpdate {
            delta: Vec2D::new(5.0, 3.0),
        };
        assert!(process_action_common(action, &physics, &mut config));
    }

    #[test]
    fn reset_view_marks_view_dirty() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let action = InputAction::ResetView;
        assert!(process_action_common(action, &physics, &mut config));
    }

    #[test]
    fn toggle_rendering_flips_flag() {
        let mut config = RendererConfig::default();
        let physics = make_disconnected_coordinator();
        let affects = process_action_common(InputAction::ToggleRendering, &physics, &mut config);
        assert!(!affects);
        assert!(!config.render_drawing);
        // Toggle back.
        process_action_common(InputAction::ToggleRendering, &physics, &mut config);
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
        ];
        for action in actions {
            let label = format!("{:?}", action);
            assert!(
                !process_action_common(action, &physics, &mut config),
                "action {} should not affect view",
                label
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

    fn build_test_state() -> UnifiedRenderState {
        let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
        UnifiedRenderState {
            input: InputHandler::new(),
            physics: make_disconnected_coordinator(),
            viewport: dummy_viewport(),
            energy_manager: EnergyWindowManager::new(energy_data),
            config: RendererConfig::default(),
            view_dirty: false,
            last_snapshot: None,
            cursor_world_pos: Vec2D::zero(),
            current_ball_size_idx: 2,
            metrics: MetricsCollector::new(),
            active_tool: OperatorTool::None,
            tool_handler: None,
            default_operator_snapshot: OperatorSnapshot::default(),
            resize_pending: false,
        }
    }

    #[test]
    fn selecting_ball_tool_enables_cursor_preview_and_marks_view_dirty() {
        let mut state = build_test_state();

        let affects_view = state.process_action(
            InputAction::SelectOperatorTool(OperatorTool::Ball),
            None,
            None,
        );

        assert!(!affects_view);
        assert_eq!(state.active_tool, OperatorTool::Ball);
        assert!(state.config.draw_cursor);
        assert!(state.view_dirty);
    }

    #[test]
    fn switching_away_from_ball_tool_disables_cursor_preview() {
        let mut state = build_test_state();
        state.active_tool = OperatorTool::Ball;
        state.config.draw_cursor = true;

        let affects_view = state.process_action(
            InputAction::SelectOperatorTool(OperatorTool::LineSegment),
            None,
            None,
        );

        assert!(!affects_view);
        assert_eq!(state.active_tool, OperatorTool::LineSegment);
        assert!(!state.config.draw_cursor);
        assert!(state.view_dirty);
    }

    #[test]
    fn next_ball_size_marks_view_dirty_when_ball_tool_is_active() {
        let mut state = build_test_state();
        state.active_tool = OperatorTool::Ball;

        let affects_view = state.process_action(InputAction::NextBallSize, None, None);

        assert!(!affects_view);
        assert_eq!(state.current_ball_size_idx, 3);
        assert!(state.view_dirty);
    }
}
