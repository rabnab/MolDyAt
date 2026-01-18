//! Input action execution for WGPU renderer
//!
//! Maps InputAction enum to physics commands and view updates.
//! Extracted from wgpu_window to separate input handling concerns.

use crate::application::physics_thread::PhysicsCommand;
use crate::application::{LogStuff, MainContent};
use crate::graphics::renderer::InputAction;
use crate::graphics::renderer::event_loop::{EnergyWindowManager, PhysicsCoordinator};
use crate::graphics::renderer::viewport::ViewportState;

use super::ball_pipeline::BallPipeline;

/// Ball sizes for cursor placement (matching BallFactory)
pub const BALL_SIZES: [f32; 5] = [0.15, 0.30, 0.60, 1.20, 2.40];

/// Execute input actions from the centralized InputHandler.
///
/// Maps InputAction enum to physics commands and view updates.
/// This function is extracted from WgpuWindow to separate concerns.
#[allow(clippy::too_many_arguments)]
pub fn execute_input_action(
    action: &InputAction,
    physics: &PhysicsCoordinator,
    viewport: &mut ViewportState,
    ball_size_idx: &mut usize,
    energy_manager: &mut EnergyWindowManager,
    window_target: &winit::event_loop::EventLoopWindowTarget<()>,
    ball_pipeline: &mut BallPipeline,
    queue: &wgpu::Queue,
) {
    match action {
        // Simulation control
        InputAction::TogglePause => {
            physics.send_command(PhysicsCommand::TogglePause);
            println!("Toggle pause");
        }
        InputAction::ToggleStepwise => {
            physics.send_command(PhysicsCommand::ToggleStepwise);
            println!("Toggle stepwise mode");
        }
        InputAction::RandomizeSpeeds => {
            physics.send_command(PhysicsCommand::RandomizeSpeeds);
            println!("Randomizing ball speeds");
        }
        InputAction::DoubleTimeStep => {
            physics.send_command(PhysicsCommand::DoubleTimeStep);
            println!("Double time step");
        }
        InputAction::HalveTimeStep => {
            physics.send_command(PhysicsCommand::HalveTimeStep);
            println!("Halve time step");
        }
        InputAction::IncreaseWaitTime => {
            physics.send_command(PhysicsCommand::IncreaseWaitTime);
            println!("Increase wait time (slower)");
        }
        InputAction::DecreaseWaitTime => {
            physics.send_command(PhysicsCommand::DecreaseWaitTime);
            println!("Decrease wait time (faster)");
        }

        // Ball manipulation
        InputAction::AddBall { position } => {
            physics.send_command(PhysicsCommand::AddBall(*position));
            println!("Adding ball at ({:.2}, {:.2})", position.x, position.y);
        }
        InputAction::AddRandomBalls { count } => {
            physics.send_command(PhysicsCommand::AddRandomBalls(*count));
            println!("Adding {} random balls", count);
        }
        InputAction::ClearBalls => {
            physics.send_command(PhysicsCommand::ClearBalls);
            println!("Clearing all balls");
        }
        InputAction::StopAllBalls => {
            physics.send_command(PhysicsCommand::StopAllBalls);
            println!("Stopping all balls");
        }
        InputAction::NextBallSize => {
            *ball_size_idx = (*ball_size_idx + 1) % BALL_SIZES.len();
            physics.send_command(PhysicsCommand::NextBallSize);
            println!(
                "Ball size: {} (index {})",
                BALL_SIZES[*ball_size_idx], ball_size_idx
            );
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
            physics.send_command(PhysicsCommand::ToggleRealData);
            println!("Status text units toggled (simulation units ↔ SI units)");
        }

        // Toggle cutoff rings around balls
        InputAction::ToggleCutoffRings => {
            ball_pipeline.toggle_cutoff_rings(queue);
            let visible = ball_pipeline.are_cutoff_rings_visible();
            println!(
                "Cutoff rings: {}",
                if visible { "visible" } else { "hidden" }
            );
        }

        // Help message
        InputAction::PrintHelp => {
            MainContent::print_help_message();
        }

        // Physics thread commands for display toggles
        InputAction::ToggleTimingDisplay => {
            physics.send_command(PhysicsCommand::ToggleTimingDisplay);
            println!("Toggled timing display");
        }
        InputAction::ToggleBallStats => {
            physics.send_command(PhysicsCommand::ToggleBallStats);
            println!("Toggled ball statistics display");
        }

        // Verbosity input - reads from stdin in physics thread
        InputAction::StartVerbosityInput => {
            println!("Starting verbosity input mode...");
            physics.send_command(PhysicsCommand::StartVerbosityInput);
        }

        // Toggle rendering (F2) - currently not tracked in WGPU
        // TODO: Add render_drawing state tracking if needed
        InputAction::ToggleRendering => {
            println!("Toggle rendering not yet fully implemented in WGPU");
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
    }
}
