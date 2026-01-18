//! Input action execution for WGPU renderer
//!
//! Maps InputAction enum to physics commands and view updates.
//! Extracted from wgpu_window to separate input handling concerns.

use crate::graphics::renderer::InputAction;
use crate::graphics::renderer::event_loop::{
    process_action_common, EnergyWindowManager, PhysicsCoordinator,
};
use crate::graphics::renderer::viewport::ViewportState;
use crate::objects2d::ball_impl::{BallFactory, NUM_BALL_SIZES};

use super::ball_pipeline::BallPipeline;

/// Execute input actions from the centralized InputHandler.
///
/// Maps InputAction enum to physics commands and view updates.
/// This function is extracted from WgpuWindow to separate concerns.
#[allow(clippy::too_many_arguments)]
pub fn execute_input_action(
    action: &InputAction,
    physics: &PhysicsCoordinator,
    render_drawing: &mut bool,
    viewport: &mut ViewportState,
    ball_size_idx: &mut usize,
    energy_manager: &mut EnergyWindowManager,
    window_target: &winit::event_loop::EventLoopWindowTarget<()>,
    ball_pipeline: &mut BallPipeline,
    queue: &wgpu::Queue,
) {
    match action {
        // Simulation control
        InputAction::NextBallSize => {
            *ball_size_idx = (*ball_size_idx + 1) % NUM_BALL_SIZES;
            let radius = BallFactory::radius_for_size_idx(*ball_size_idx);
            println!("Ball size: {:.2} (index {})", radius, ball_size_idx);
        }

        // View control
        InputAction::ResetView => {
            viewport.reset();
            println!("View reset");
        }

        // Energy window control
        InputAction::ToggleEnergyWindow | InputAction::CyclePlotContent => {
            energy_manager.toggle(window_target);
        }

        // Toggle real data display in energy window
        // This switches between simulation units and SI units (Joules, Kelvin, m/s)
        InputAction::ToggleRealData => {
            // Delegated below
        }

        // Toggle cutoff rings around balls (WGPU-specific)
        InputAction::ToggleCutoffRings => {
            ball_pipeline.toggle_cutoff_rings(queue);
            let visible = ball_pipeline.are_cutoff_rings_visible();
            println!(
                "Cutoff rings: {}",
                if visible { "visible" } else { "hidden" }
            );
        }

        // Actions that are context-dependent or handled elsewhere
        InputAction::EnableCursor { .. }
        | InputAction::DisableCursor
        | InputAction::UpdateCursor { .. }
        | InputAction::EnableRandomBallSpeed
        | InputAction::ZoomIn { .. }
        | InputAction::ZoomOut { .. }
        | InputAction::PanStart { .. }
        | InputAction::PanUpdate { .. }
        | InputAction::PanEnd => {
            // These are handled separately in the event loop or not needed for WGPU
        }
        _ => {}
    }

    // Delegate common physics/renderer actions to shared helper
    let _ = process_action_common(action, physics, render_drawing);
}
