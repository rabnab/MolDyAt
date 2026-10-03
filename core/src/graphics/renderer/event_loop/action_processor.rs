use super::{InputAction, PhysicsCoordinator};
use crate::application::{DomainCommand, LogStuff, MainContent};
use crate::graphics::renderer::state_layers::RendererConfig;
use std::convert::TryFrom;

/// Shared mapping from InputAction to physics commands and renderer state flags.
/// Returns true if the action affects the viewport (caller should refresh view).
pub fn process_action_common(
    action: InputAction,
    physics: &PhysicsCoordinator,
    config: &mut RendererConfig,
) -> bool {
    let mut affects_view = false;

    match DomainCommand::try_from(action) {
        Ok(command) => {
            physics.send_command(command);
        }
        Err(action) => match action {
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
            InputAction::PrintHelp => {
                MainContent::print_help_message();
            }
            InputAction::CyclePlotContent => {
                config.cycle_plot_content();
            }
            InputAction::CycleThermoDisplay => {
                config.cycle_thermo_display();
                affects_view = true;
            }
            // Actions handled elsewhere (not in action_processor)
            InputAction::PanStart { .. }
            | InputAction::PanEnd
            | InputAction::UpdateCursor { .. }
            | InputAction::SelectOperatorTool(_)
            | InputAction::DeleteHovered
            | InputAction::ToggleEnergyWindow
            | InputAction::ToggleCutoffRings => {}
            InputAction::AddBall { .. }
            | InputAction::NextBallSize
            | InputAction::AddRandomBalls { .. }
            | InputAction::StopAllBalls
            | InputAction::ClearBalls
            | InputAction::TogglePause
            | InputAction::ToggleGravity
            | InputAction::ToggleStepwise
            | InputAction::RandomizeSpeeds
            | InputAction::EnableRandomBallSpeed
            | InputAction::DoubleTimeStep
            | InputAction::HalveTimeStep
            | InputAction::IncreaseWaitTime
            | InputAction::DecreaseWaitTime
            | InputAction::ToggleRealData
            | InputAction::ToggleTimingDisplay
            | InputAction::ToggleBallStats
            | InputAction::StartVerbosityInput
            | InputAction::SaveWorld
            | InputAction::LoadWorld
            | InputAction::OperatorAction(_) => {
                unreachable!("domain actions must map to DomainCommand")
            }
        },
    }

    affects_view
}
