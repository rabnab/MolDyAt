//! Input action execution for WGPU renderer.
//!
//! Handles WGPU-specific actions (ring display mode) that require wgpu resources.
//! All other actions are delegated to UnifiedRenderState.

use rust_thermo_core::graphics::renderer::InputAction;
use rust_thermo_core::graphics::renderer::event_loop::UnifiedRenderState;

use super::wgpu_pipelines::WgpuPipelines;

/// Execute input actions from the centralized InputHandler.
///
/// Only handles truly WGPU-specific actions that require wgpu resources.
/// All other actions (including ToggleEnergyWindow) are delegated to UnifiedRenderState.
///
/// Returns `true` if the action affects the viewport.
pub fn execute_input_action(
    action: InputAction,
    state: &mut UnifiedRenderState,
    pipelines: &mut WgpuPipelines,
    queue: &wgpu::Queue,
    window_target: &winit::event_loop::ActiveEventLoop,
    main_window: &winit::window::Window,
) -> bool {
    // Handle ONLY actions that need wgpu resources
    if let InputAction::ToggleCutoffRings = &action {
        pipelines.cycle_ring_display_mode(queue);
        return false; // doesn't affect view
    }

    // Delegate all other actions to UnifiedRenderState (single source of truth)
    // Pass window_target so ToggleEnergyWindow can be handled
    state.process_action(action, Some(window_target), Some(main_window))
}
