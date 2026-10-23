// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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

use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::graphics::renderer::state_layers::RendererConfig;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use thermo_physics::application::MainContent;
use thermo_physics::objects2d::{Rect, Vec2D};
use thermo_runtime::application::renderer_snapshot::{OperatorSnapshot, RendererSnapshot};
use thermo_runtime::application::scene_layers::SceneLayers;

use super::energy_window_manager::EnergyWindowManager;
use super::frame_orchestrator::FrameOrchestrator;
use super::input_handler::{InputAction, InputHandler, OperatorTool};
use super::input_orchestrator::InputOrchestrator;
use super::metrics::MetricsCollector;
use super::operator_tool_handler::OperatorToolHandler;
use super::physics_coordinator::PhysicsCoordinator;
use super::viewport_orchestrator::ViewportOrchestrator;

use thermo_runtime::application::{BallCommand, ViewCommand};

/// Owned renderer state that backs the event-loop orchestrators.
pub(super) struct RenderStateData {
    pub(super) viewport: ViewportTransform,
    pub(super) energy_manager: EnergyWindowManager,
    pub(super) config: RendererConfig,
    pub(super) view_dirty: bool,
    pub(super) last_snapshot: Option<Arc<RendererSnapshot>>,
    pub(super) cursor_world_pos: Vec2D,
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
                metrics: MetricsCollector::new(),
                active_tool: OperatorTool::None,
                tool_handler: None,
                default_operator_snapshot: OperatorSnapshot::default(),
                resize_pending: false,
                requested_window_size: None,
                pending_window_request: None,
            },
        };
        // The physics thread builds only the layers the config draws.
        state.physics.send_command(ViewCommand::SetSnapshotLayers(
            state.render.config.scene_layers(),
        ));
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

    pub fn active_tool(&self) -> OperatorTool {
        self.render.active_tool
    }

    pub fn metrics_mut(&mut self) -> &mut MetricsCollector {
        &mut self.render.metrics
    }

    pub fn merge_view_dirty(&mut self, dirty: bool) {
        self.render.view_dirty |= dirty;
    }

    /// Whether the view changed since the last drawn frame.
    pub fn is_view_dirty(&self) -> bool {
        self.render.view_dirty
    }

    pub(super) fn mark_view_dirty(&mut self) {
        self.render.view_dirty = true;
    }

    pub(super) fn is_active_tool(&self, tool: OperatorTool) -> bool {
        self.render.active_tool == tool
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

    /// Scene layers to draw this frame: the ones the config asks for that
    /// the current snapshot holds.
    pub fn frame_layers(&self) -> SceneLayers {
        FrameOrchestrator::frame_layers(self)
    }

    /// Call after a frame has been drawn.
    ///
    /// Finalises per-frame metrics, clears the `view_dirty` flag, and
    /// marks the current snapshot as displayed so `should_render` won't
    /// re-trigger for the same data.
    pub fn end_frame(&mut self) {
        FrameOrchestrator::end_frame(self);
    }

    /// Smoothed render FPS (rolling average over the last 60 frames).
    pub fn render_fps(&self) -> f32 {
        self.render.metrics.average_fps()
    }

    // ── Input-action dispatch ────────────────────────────────────────

    /// Process a user action that is common to all renderers.
    ///
    /// Registry commands (keys) are routed by their `CommandKind`: domain
    /// commands go to the physics thread, UI commands are handled here (see
    /// `command_registry`). Mouse and tool actions are handled directly.
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
        world_rect: thermo_physics::objects2d::Rect,
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

    /// Cycle to the next ball species.
    ///
    /// Sends the command to the physics thread, which owns the species of new
    /// balls; the window reads it back from the snapshot
    /// (`cursor_species`, `species`).
    pub fn next_species(&mut self) {
        self.physics.send_command(BallCommand::NextSpecies);
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
