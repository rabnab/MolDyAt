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
        InputAction::CycleThermoDisplay => {
            config.cycle_thermo_display();
            affects_view = true;
        }
        // Actions handled elsewhere (not in action_processor)
        InputAction::PanStart { .. }
        | InputAction::PanEnd
        | InputAction::UpdateCursor { .. }
        | InputAction::SelectOperatorTool(_)
        | InputAction::ToggleEnergyWindow  // handled in UnifiedRenderState::process_action
        | InputAction::ToggleCutoffRings => {}  // needs wgpu resources
        InputAction::CommitLineSegment { start, end } => {
            physics.send_command(PhysicsCommand::AddLineSegment {
                start: *start,
                end: *end,
            });
        }
        InputAction::MoveLineSegmentEndpoint {
            segment_id,
            new_pos,
            move_start,
        } => {
            physics.send_command(PhysicsCommand::MoveLineSegmentEndpoint {
                segment_id: *segment_id,
                new_pos: *new_pos,
                move_start: *move_start,
            });
        }
        InputAction::CommitValveSegment {
            line_id,
            t_start,
            t_end,
        } => {
            physics.send_command(PhysicsCommand::AddValveSegment {
                line_id: *line_id,
                t_start: *t_start,
                t_end: *t_end,
                is_closed: true,
            });
        }
        InputAction::ToggleValve { valve_id } => {
            // We don't know current state here; physics thread will handle it
            // via a dedicated toggle command. For now use SetValveClosed with
            // a toggle-like approach: we'll add a Toggle variant.
            // Actually, let's just negate — but we don't have state here.
            // Send as toggle: the physics command needs the target state.
            // Since we can't query, we'll add a dedicated toggle command.
            physics.send_command(PhysicsCommand::ToggleValve {
                valve_id: *valve_id,
            });
        }
        InputAction::MoveValveSegment {
            valve_id,
            t_start,
            t_end,
        } => {
            physics.send_command(PhysicsCommand::MoveValveSegment {
                valve_id: *valve_id,
                t_start: *t_start,
                t_end: *t_end,
            });
        }
    }

    affects_view
}
