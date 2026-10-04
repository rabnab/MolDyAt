//! Shared renderer event-loop state for both ggez and wgpu backends.
//!
//! `UnifiedRenderState` is now a thin facade over owned renderer state and
//! focused orchestrators so both backends still follow the same frame
//! lifecycle:
//!
//! ```text
//! begin_frame() → handle events → should_render() → render → end_frame()
//! ```
//!
//! `RenderStateData` holds shared renderer data such as the viewport, energy
//! window state, cursor/tool bookkeeping, and frame metrics. Orchestrators own
//! the higher-level input, frame, and viewport coordination while
//! `UnifiedRenderState` exposes the narrowed public surface used by the active
//! backends.

use crate::application::operator_command::delete::DeleteObject;
use crate::application::operator_command::line::SetGhostWall;
use crate::application::renderer_snapshot::{OperatorSnapshot, RendererSnapshot};
use crate::application::{MainContent, PhysicsEvent};
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::graphics::renderer::state_layers::RendererConfig;
use crate::objects2d::wall_chain::ObjectRef;
use crate::objects2d::{Rect, Vec2D};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::energy_window_manager::EnergyWindowManager;
use super::frame_orchestrator::{FrameDetail, FrameOrchestrator};
use super::input_handler::{InputAction, InputHandler, OperatorTool};
use super::input_orchestrator::InputOrchestrator;
use super::line_segment_tool::LineSegmentToolHandler;
use super::metrics::MetricsCollector;
use super::operator_tool_handler::{OperatorToolHandler, ToolContext, ToolPreview};
use super::physics_coordinator::PhysicsCoordinator;
use super::source_sphere_tool::SourceSphereToolHandler;
use super::thermostat_tool::ThermostatToolHandler;
use super::valve_segment_tool::ValveSegmentToolHandler;
use super::viewport_orchestrator::ViewportOrchestrator;

use crate::application::DomainCommand;

/// Owned renderer state that backs the event-loop orchestrators.
pub(super) struct RenderStateData {
    pub(super) viewport: ViewportTransform,
    pub(super) energy_manager: EnergyWindowManager,
    pub(super) config: RendererConfig,
    pub(super) view_dirty: bool,
    pub(super) last_snapshot: Option<Arc<RendererSnapshot>>,
    pub(super) cursor_world_pos: Vec2D,
    pub(super) current_ball_size_idx: usize,
    pub(super) metrics: MetricsCollector,
    pub(super) active_tool: OperatorTool,
    pub(super) tool_handler: Option<Box<dyn OperatorToolHandler>>,
    pub(super) default_operator_snapshot: OperatorSnapshot,
    pub(super) resize_pending: bool,
    /// Inner window size (physical pixels) the backend should ask for, so a
    /// loaded world fits the window without being stretched.
    pub(super) requested_window_size: Option<(u32, u32)>,
    /// The window size asked for after a load, while its answer is pending:
    /// that answer must not rescale the loaded world (see
    /// `ViewportOrchestrator::handle_resize`).
    pub(super) pending_window_request: Option<PendingWindowRequest>,
}

/// A window size the backend asked the OS for, and when.
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingWindowRequest {
    pub(super) size: (u32, u32),
    pub(super) at: Instant,
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

    // ── Owned renderer state ──────────────────────────────────────────
    pub(super) render: RenderStateData,
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

        let mut state = Self {
            input: InputHandler::new(),
            physics,
            render: RenderStateData {
                viewport,
                energy_manager,
                config: RendererConfig::default(),
                view_dirty: true,
                last_snapshot: None,
                cursor_world_pos: Vec2D::zero(),
                current_ball_size_idx: 2,
                metrics: MetricsCollector::new(),
                active_tool: OperatorTool::None,
                tool_handler: None,
                default_operator_snapshot: OperatorSnapshot::default(),
                resize_pending: false,
                requested_window_size: None,
                pending_window_request: None,
            },
        };
        // A world loaded with `--load` may not have the window's shape.
        state.fit_window_to_world();
        state
    }

    pub fn viewport(&self) -> &ViewportTransform {
        &self.render.viewport
    }

    pub fn energy_manager(&self) -> &EnergyWindowManager {
        &self.render.energy_manager
    }

    pub fn energy_manager_mut(&mut self) -> &mut EnergyWindowManager {
        &mut self.render.energy_manager
    }

    pub fn config(&self) -> &RendererConfig {
        &self.render.config
    }

    pub fn last_snapshot(&self) -> Option<&Arc<RendererSnapshot>> {
        self.render.last_snapshot.as_ref()
    }

    pub fn cursor_world_pos(&self) -> Vec2D {
        self.render.cursor_world_pos
    }

    pub fn current_ball_size_idx(&self) -> usize {
        self.render.current_ball_size_idx
    }

    pub fn active_tool(&self) -> OperatorTool {
        self.render.active_tool
    }

    pub fn metrics_mut(&mut self) -> &mut MetricsCollector {
        &mut self.render.metrics
    }

    pub fn merge_view_dirty(&mut self, dirty: bool) {
        self.render.view_dirty |= dirty;
    }

    pub(super) fn mark_view_dirty(&mut self) {
        self.render.view_dirty = true;
    }

    pub(super) fn is_active_tool(&self, tool: OperatorTool) -> bool {
        self.render.active_tool == tool
    }

    /// Return the active tool to idle, dropping any ghost wall it had sent
    /// to the physics thread.
    fn reset_tool_handler(&mut self) {
        if let Some(handler) = self.render.tool_handler.as_mut() {
            handler.reset();
        }
        if self.render.active_tool == OperatorTool::LineSegment {
            self.physics
                .send_command(DomainCommand::Operator(Box::new(SetGhostWall {
                    ghost: None,
                })));
        }
    }

    pub(super) fn select_operator_tool(&mut self, tool: OperatorTool) {
        self.reset_tool_handler();
        self.render.active_tool = tool;
        self.render.tool_handler = Self::handler_for_tool(tool);
        self.render.config.draw_cursor = tool == OperatorTool::Ball;
        self.render.view_dirty = true;
    }

    fn current_operator_snapshot(&self) -> &OperatorSnapshot {
        self.render
            .last_snapshot
            .as_deref()
            .map(|snapshot| &snapshot.operators)
            .unwrap_or(&self.render.default_operator_snapshot)
    }

    pub(super) fn can_add_ball_at(&self, position: Vec2D) -> bool {
        !self
            .current_operator_snapshot()
            .is_near_any_operator(position)
    }

    pub(super) fn with_tool_handler<R>(
        &mut self,
        f: impl FnOnce(&mut dyn OperatorToolHandler, ToolContext<'_>) -> R,
    ) -> Option<R> {
        let render = &mut self.render;
        let RenderStateData {
            tool_handler,
            last_snapshot,
            default_operator_snapshot,
            viewport,
            cursor_world_pos,
            ..
        } = render;
        let handler = tool_handler.as_deref_mut()?;
        let ctx = Self::make_tool_context_from(
            last_snapshot.as_deref(),
            default_operator_snapshot,
            viewport,
            *cursor_world_pos,
        );
        Some(f(handler, ctx))
    }

    // ── Frame lifecycle ──────────────────────────────────────────────

    /// Call at the start of each frame.
    ///
    /// Pulls the latest physics snapshot.  Does **not** start the metrics
    /// timer — call [`begin_render`] for that.
    pub fn begin_frame(&mut self) {
        FrameOrchestrator::begin_frame(self);
    }

    /// Start the per-frame metrics timer.
    ///
    /// Call this once per frame, immediately before issuing draw calls.
    /// Paired with [`end_frame`] which finalises the metrics.
    pub fn begin_render(&mut self) {
        FrameOrchestrator::begin_render(self);
    }

    /// Whether the renderer should draw a frame right now.
    pub fn should_render(&self) -> bool {
        FrameOrchestrator::should_render(self)
    }

    /// What the renderer draws this frame: [`FrameDetail::Full`] scene,
    /// [`FrameDetail::Light`] (rendering off, F2: operators, preview and
    /// labels only) or [`FrameDetail::Skip`].
    pub fn frame_detail(&self) -> FrameDetail {
        FrameOrchestrator::frame_detail(self)
    }

    /// Call after a frame has been drawn.
    ///
    /// Finalises per-frame metrics, clears the `view_dirty` flag, and
    /// marks the current simulation step as displayed so `should_render`
    /// won't re-trigger for the same data.
    pub fn end_frame(&mut self) {
        FrameOrchestrator::end_frame(self);
    }

    /// Smoothed render FPS (rolling average over the last 60 frames).
    pub fn render_fps(&self) -> f32 {
        self.render.metrics.average_fps()
    }

    /// Collect all operator preview geometry for the current frame.
    ///
    /// Delegates to the active [`OperatorToolHandler`] for preview and
    /// snap indicators, then bundles them with the selected sidebar icon.
    pub fn operator_previews(&self) -> super::FramePreviews {
        let ctx = self.make_tool_context();

        let (line_segment, valve_segment, snap_indicators) =
            if let Some(handler) = &self.render.tool_handler {
                let preview = handler.preview(&ctx);
                let snaps = handler.snap_indicators(&ctx);
                match preview {
                    ToolPreview::LineSegment {
                        start,
                        end,
                        thickness,
                    } => (Some((start, end, thickness)), None, snaps),
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
            hovered: self.hovered_object(),
        }
    }

    /// The object the active tool reports under the cursor, if any.
    pub(super) fn hovered_object(&self) -> Option<ObjectRef> {
        let handler = self.render.tool_handler.as_deref()?;
        handler.hovered(&self.make_tool_context())
    }

    /// Delete the hovered object, if any. The tool is reset first so it
    /// can't keep dragging or toggling an object that no longer exists.
    pub(super) fn delete_hovered_object(&mut self) {
        let Some(target) = self.hovered_object() else {
            return;
        };
        self.reset_tool_handler();
        self.physics
            .send_command(DomainCommand::Operator(Box::new(DeleteObject { target })));
        self.render.view_dirty = true;
    }

    /// React to a result reported by the physics thread. A loaded world is
    /// shown in the default view of its own rectangle; the view only changes
    /// once the physics thread has actually replaced the world.
    pub(super) fn handle_physics_event(&mut self, event: PhysicsEvent) {
        match event {
            PhysicsEvent::WorldSaved(path) => log::info!("Saved world to {}", path.display()),
            PhysicsEvent::SaveFailed(error) => log::error!("Saving world failed: {error}"),
            PhysicsEvent::WorldLoaded(loaded) => {
                log::info!("Loaded world from {} (paused)", loaded.path.display());
                self.reset_tool_handler();
                self.render.viewport.set_world_bounds(loaded.world_rect);
                self.render.viewport.reset();
                self.render.view_dirty = true;
                self.render.pending_window_request = None;
                self.fit_window_to_world();
            }
            PhysicsEvent::LoadFailed(error) => log::error!("Loading world failed: {error}"),
        }
    }

    /// Ask for a window of the world's shape when it differs from the drawing
    /// area's (a loaded world). The world is letterboxed meanwhile, so it is
    /// never stretched; the requested size maps it 1:1 (`LEN_TO_PX`), which
    /// removes the bars. Whatever size the OS grants, the loaded world is
    /// kept as saved (`pending_window_request`). A world of the window's shape is
    /// left alone, which keeps a normal start untouched.
    fn fit_window_to_world(&mut self) {
        let viewport = &self.render.viewport;
        let world = viewport.world_bounds();
        let (vp_w, vp_h) = viewport.viewport_size_px();
        let world_aspect = world.w.abs() / world.h.abs().max(f32::EPSILON);
        let viewport_aspect = vp_w / vp_h;
        if (world_aspect / viewport_aspect - 1.0).abs() < ViewportTransform::LETTERBOX_TOLERANCE {
            return;
        }
        let margins = viewport.margins();
        let width = world.w.abs() / ViewportTransform::LEN_TO_PX + margins.horizontal();
        let height = world.h.abs() / ViewportTransform::LEN_TO_PX + margins.vertical();
        self.render.requested_window_size = Some((width.round() as u32, height.round() as u32));
    }

    /// Apply the OS's immediate answer to a window size request: `size` is
    /// what the window got. Unlike a `Resized` event, this answer is final
    /// even when the size did not change, so the loaded world is no longer
    /// waiting for one, and a later (user) resize refits the world as usual.
    pub fn apply_window_size_answer(&mut self, width: f32, height: f32, margin_px: Margins) {
        self.handle_resize(width, height, margin_px);
        self.render.pending_window_request = None;
    }

    /// The window size a backend should request, once (see
    /// [`Self::fit_window_to_world`]).
    /// Taking it means the backend now asks the OS, so the answer is awaited
    /// from here on.
    pub fn take_requested_window_size(&mut self) -> Option<(u32, u32)> {
        let size = self.render.requested_window_size.take()?;
        self.render.pending_window_request = Some(PendingWindowRequest {
            size,
            at: Instant::now(),
        });
        Some(size)
    }

    /// Build a [`ToolContext`] from the current snapshot.
    ///
    /// The returned context borrows `snapshot` (if `Some`) or the provided
    /// `fallback` default.  Callers that also need `&mut self.tool_handler`
    /// should invoke this with explicit field borrows to avoid conflicting
    /// `&self` / `&mut self` borrows.
    pub(super) fn make_tool_context_from<'a>(
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
            self.render.last_snapshot.as_deref(),
            &self.render.default_operator_snapshot,
            &self.render.viewport,
            self.render.cursor_world_pos,
        )
    }

    /// Create a handler for the given tool, or `None` if no tool selected.
    pub(super) fn handler_for_tool(tool: OperatorTool) -> Option<Box<dyn OperatorToolHandler>> {
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
        InputOrchestrator::process_action(self, action, window_target, main_window)
    }

    // ── Viewport helpers ─────────────────────────────────────────────

    /// Zoom the viewport so that `world_point` stays fixed on screen.
    pub fn zoom_at_point(&mut self, world_point: &Vec2D, factor: f32) {
        ViewportOrchestrator::zoom_at_point(self, world_point, factor);
    }

    /// Reset viewport to default zoom and pan.
    pub fn reset_view(&mut self) {
        ViewportOrchestrator::reset_view(self);
    }

    /// Handle a window resize.
    ///
    /// Updates the viewport **and** notifies the physics thread so the
    /// simulation world matches the new window dimensions.
    pub fn handle_resize(&mut self, new_width: f32, new_height: f32, margin_px: Margins) {
        ViewportOrchestrator::handle_resize(self, new_width, new_height, margin_px);
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
        ViewportOrchestrator::handle_resize_with_world_rect(
            self, new_width, new_height, world_rect,
        );
    }

    /// Resume physics after resize if a resize completion is pending.
    pub fn resume_after_resize_if_pending(&mut self) {
        ViewportOrchestrator::resume_after_resize_if_pending(self);
    }

    /// Apply a pan delta (in pixels) originating from a drag gesture.
    pub fn apply_pan_from_input(&mut self, delta: Vec2D) {
        ViewportOrchestrator::apply_pan_from_input(self, delta);
    }

    /// Convert a screen-pixel position to world coordinates and update the
    /// stored cursor position.
    pub fn update_cursor_world_pos(&mut self, px: &[f32; 2]) {
        ViewportOrchestrator::update_cursor_world_pos(self, px);
    }

    // ── Energy window helpers ────────────────────────────────────────

    /// Toggle or cycle the energy plot window.
    pub fn toggle_energy_window(
        &mut self,
        window_target: &winit::event_loop::ActiveEventLoop,
        main_window: Option<&winit::window::Window>,
    ) {
        self.render
            .energy_manager
            .toggle(window_target, main_window);
    }

    // ── Ball size helpers ────────────────────────────────────────────

    /// Cycle to the next ball size.
    ///
    /// Updates the local size index and sends command to physics thread.
    pub fn next_ball_size(&mut self) {
        use crate::objects2d::ball_impl::NUM_BALL_SIZES;
        self.render.current_ball_size_idx =
            (self.render.current_ball_size_idx + 1) % NUM_BALL_SIZES;
        self.physics.send_command(DomainCommand::NextBallSize);
    }

    // ── Centralized mouse event handlers ──────────────────────────────

    /// Centralized handler for all mouse events.
    ///
    /// Processes MouseInput, CursorMoved, and MouseWheel events.
    /// Returns true if the action affects the viewport.
    pub fn handle_mouse_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        InputOrchestrator::handle_mouse_event(self, event)
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
        InputOrchestrator::handle_mouse_input_event(self, button, state)
    }

    /// Handle cursor moved event.
    ///
    /// Updates cursor position and processes pan gestures.
    /// When an operator tool is active, marks view dirty so previews and
    /// snap indicators update continuously.
    /// Returns true if the action affects the viewport.
    pub fn handle_cursor_moved_event(&mut self, x: f32, y: f32) -> bool {
        InputOrchestrator::handle_cursor_moved_event(self, x, y)
    }

    /// Handle mouse wheel event.
    ///
    /// Delegates to the active tool handler first. If the tool consumes
    /// the scroll (e.g. source sphere speed/period adjustment), the
    /// event stops. Otherwise falls through to zoom.
    /// Returns true if the action affects the viewport.
    pub fn handle_mouse_wheel_event(&mut self, delta: winit::event::MouseScrollDelta) -> bool {
        InputOrchestrator::handle_mouse_wheel_event(self, delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::renderer::event_loop::action_processor::process_action_common;

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
        // Switching back on must redraw the full scene at once.
        assert!(affects);
        assert!(!config.render_drawing);
        // Toggle back.
        process_action_common(InputAction::ToggleRendering, &physics, &mut config);
        assert!(config.render_drawing);
    }

    #[test]
    fn toggle_rendering_tells_the_physics_thread_which_snapshots_to_build() {
        let mut config = RendererConfig::default();
        let (physics, log) = PhysicsCoordinator::new_recording();

        process_action_common(InputAction::ToggleRendering, &physics, &mut config);
        process_action_common(InputAction::ToggleRendering, &physics, &mut config);

        let sent = log.lock().unwrap().clone();
        assert_eq!(
            sent,
            vec!["SetFullSnapshots(false)", "SetFullSnapshots(true)"]
        );
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
            render: RenderStateData {
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
                requested_window_size: None,
                pending_window_request: None,
            },
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
        assert_eq!(state.render.active_tool, OperatorTool::Ball);
        assert!(state.render.config.draw_cursor);
        assert!(state.render.view_dirty);
    }

    #[test]
    fn switching_away_from_ball_tool_disables_cursor_preview() {
        let mut state = build_test_state();
        state.render.active_tool = OperatorTool::Ball;
        state.render.config.draw_cursor = true;

        let affects_view = state.process_action(
            InputAction::SelectOperatorTool(OperatorTool::LineSegment),
            None,
            None,
        );

        assert!(!affects_view);
        assert_eq!(state.render.active_tool, OperatorTool::LineSegment);
        assert!(!state.render.config.draw_cursor);
        assert!(state.render.view_dirty);
    }

    #[test]
    fn next_ball_size_marks_view_dirty_when_ball_tool_is_active() {
        let mut state = build_test_state();
        state.render.active_tool = OperatorTool::Ball;

        let affects_view = state.process_action(InputAction::NextBallSize, None, None);

        assert!(!affects_view);
        assert_eq!(state.render.current_ball_size_idx, 3);
        assert!(state.render.view_dirty);
    }
}
