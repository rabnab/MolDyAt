//! Input action execution for WGPU renderer
//!
//! Handles WGPU-specific actions (cutoff rings) that require wgpu resources.
//! All other actions are delegated to UnifiedRenderState.

use crate::graphics::renderer::InputAction;
use crate::graphics::renderer::event_loop::UnifiedRenderState;

use super::ball_pipeline::BallPipeline;

/// Execute input actions from the centralized InputHandler.
///
/// Only handles truly WGPU-specific actions that require wgpu resources
/// or need access to resources not available in UnifiedRenderState (like window_target).
/// All other actions are delegated to UnifiedRenderState.
///
/// Returns `true` if the action affects the viewport.
pub fn execute_input_action(
    action: &InputAction,
    state: &mut UnifiedRenderState,
    ball_pipeline: &mut BallPipeline,
    queue: &wgpu::Queue,
    window_target: &winit::event_loop::EventLoopWindowTarget<()>,
) -> bool {
    // Handle ONLY actions that need wgpu resources or window_target
    match action {
        InputAction::ToggleCutoffRings => {
            ball_pipeline.toggle_cutoff_rings(queue);
            let visible = ball_pipeline.are_cutoff_rings_visible();
            println!(
                "Cutoff rings: {}",
                if visible { "visible" } else { "hidden" }
            );
            return false; // doesn't affect view
        }
        InputAction::ToggleEnergyWindow | InputAction::CyclePlotContent => {
            state.toggle_energy_window(window_target);
            return false; // doesn't affect view
        }
        _ => {}
    }

    // Delegate all other actions to UnifiedRenderState (single source of truth)
    state.process_action(action)
}
