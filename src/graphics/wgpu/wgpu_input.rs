//! Input action execution for WGPU renderer
//!
//! Maps InputAction enum to physics commands and view updates.
//! Extracted from wgpu_window to separate input handling concerns.

use crate::graphics::renderer::InputAction;
use crate::graphics::renderer::event_loop::UnifiedRenderState;
use crate::objects2d::ball_impl::{BallFactory, NUM_BALL_SIZES};

use super::ball_pipeline::BallPipeline;

/// Execute input actions from the centralized InputHandler.
///
/// Handles WGPU-specific actions (cutoff rings, ball size cycling, energy window),
/// then delegates common actions to UnifiedRenderState.
///
/// Returns `true` if the action affects the viewport.
pub fn execute_input_action(
    action: &InputAction,
    state: &mut UnifiedRenderState,
    ball_pipeline: &mut BallPipeline,
    queue: &wgpu::Queue,
    window_target: &winit::event_loop::EventLoopWindowTarget<()>,
) -> bool {
    // Handle WGPU-specific actions first
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
        InputAction::NextBallSize => {
            state.current_ball_size_idx = (state.current_ball_size_idx + 1) % NUM_BALL_SIZES;
            let radius = BallFactory::radius_for_size_idx(state.current_ball_size_idx);
            println!(
                "Ball size: {:.2} (index {})",
                radius, state.current_ball_size_idx
            );
            return false; // doesn't affect view
        }
        InputAction::ResetView => {
            state.reset_view();
            return true; // affects view
        }
        InputAction::ToggleEnergyWindow | InputAction::CyclePlotContent => {
            state.energy_manager.toggle(window_target);
            return false; // doesn't affect view
        }
        _ => {}
    }

    // Delegate all other actions to UnifiedRenderState (single source of truth)
    state.process_action(action)
}
