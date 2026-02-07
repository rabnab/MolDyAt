use super::{InputAction, PhysicsCoordinator};
use crate::application::physics_thread::PhysicsCommand;
use crate::application::{LogStuff, MainContent};
use crate::graphics::renderer::state_layers::RendererConfig;

/// Shared mapping from InputAction to physics commands and renderer state flags.
/// Returns true if the action affects the viewport (caller should refresh view).
pub fn process_action_common(
    action: &InputAction,
    physics: &PhysicsCoordinator,
    config: &mut RendererConfig,
) -> bool {
    let mut affects_view = false;

    match action {
        InputAction::TogglePause => {
            physics.send_command(PhysicsCommand::TogglePause);
        }
        InputAction::ZoomIn { .. } | InputAction::ZoomOut { .. } => {
            // Viewport changes are handled by the event loop
            affects_view = true;
        }
        InputAction::PanUpdate { .. } => {
            affects_view = true;
        }
        InputAction::ResetView => {
            affects_view = true;
        }
        InputAction::ToggleRendering => {
            config.toggle_rendering();
        }
        InputAction::AddBall { position } => {
            physics.send_command(PhysicsCommand::AddBall(*position));
            // When adding ball (Ctrl released), disable cursor to return to crosshair
            config.disable_cursor();
        }
        InputAction::NextBallSize => {
            physics.send_command(PhysicsCommand::NextBallSize);
        }
        InputAction::AddRandomBalls { count } => {
            physics.send_command(PhysicsCommand::AddRandomBalls(*count));
        }
        InputAction::StopAllBalls => {
            physics.send_command(PhysicsCommand::StopAllBalls);
        }
        InputAction::ClearBalls => {
            physics.send_command(PhysicsCommand::ClearBalls);
        }
        InputAction::ToggleStepwise => {
            physics.send_command(PhysicsCommand::ToggleStepwise);
        }
        InputAction::RandomizeSpeeds => {
            physics.send_command(PhysicsCommand::RandomizeSpeeds);
        }
        InputAction::DoubleTimeStep => {
            physics.send_command(PhysicsCommand::DoubleTimeStep);
        }
        InputAction::HalveTimeStep => {
            physics.send_command(PhysicsCommand::HalveTimeStep);
        }
        InputAction::IncreaseWaitTime => {
            physics.send_command(PhysicsCommand::IncreaseWaitTime);
        }
        InputAction::DecreaseWaitTime => {
            physics.send_command(PhysicsCommand::DecreaseWaitTime);
        }
        InputAction::EnableRandomBallSpeed => {
            physics.send_command(PhysicsCommand::EnableRandomBallSpeed);
        }
        InputAction::ToggleRealData => {
            physics.send_command(PhysicsCommand::ToggleRealData);
        }
        InputAction::ToggleTimingDisplay => {
            physics.send_command(PhysicsCommand::ToggleTimingDisplay);
        }
        InputAction::ToggleBallStats => {
            physics.send_command(PhysicsCommand::ToggleBallStats);
        }
        InputAction::StartVerbosityInput => {
            physics.send_command(PhysicsCommand::StartVerbosityInput);
        }
        InputAction::PrintHelp => {
            MainContent::print_help_message();
        }
        InputAction::EnableCursor { repeated } => {
            config.enable_cursor(*repeated);
        }
        InputAction::DisableCursor => {
            config.disable_cursor();
        }
        InputAction::CyclePlotContent => {
            config.cycle_plot_content();
        }
        // Actions handled in event loops (need framework-specific resources)
        InputAction::PanStart { .. }
        | InputAction::PanEnd
        | InputAction::UpdateCursor { .. }
        | InputAction::ToggleEnergyWindow  // needs window_target
        | InputAction::ToggleCutoffRings => {}  // needs wgpu resources
    }

    affects_view
}
