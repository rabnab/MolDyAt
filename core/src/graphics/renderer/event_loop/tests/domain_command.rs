use crate::application::DomainCommand;
use crate::application::operator_command::line::AddLineSegment;
use crate::graphics::renderer::InputAction;
use crate::objects2d::Vec2D;
use std::convert::TryFrom;

#[test]
fn test_input_action_toggle_pause_maps_to_domain_command() {
    let command = DomainCommand::try_from(InputAction::TogglePause);
    assert!(matches!(command, Ok(DomainCommand::TogglePause)));
}

#[test]
fn test_input_action_add_ball_preserves_position() {
    let position = Vec2D::new(10.0, 20.0);
    let command = DomainCommand::try_from(InputAction::AddBall { position });

    assert!(matches!(command, Ok(DomainCommand::AddBall(mapped)) if mapped == position));
}

#[test]
fn test_input_action_double_time_step_maps_to_domain_command() {
    let command = DomainCommand::try_from(InputAction::DoubleTimeStep);
    assert!(matches!(command, Ok(DomainCommand::DoubleTimeStep)));
}

#[test]
fn test_input_action_operator_action_maps_to_domain_command() {
    let command = DomainCommand::try_from(InputAction::OperatorAction(Box::new(AddLineSegment {
        start: Vec2D::new(0.0, 0.0),
        end: Vec2D::new(5.0, 5.0),
    })));

    assert!(matches!(command, Ok(DomainCommand::Operator(_))));
}

#[test]
fn test_reset_view_stays_outside_domain_command() {
    let command = DomainCommand::try_from(InputAction::ResetView);
    assert!(matches!(command, Err(InputAction::ResetView)));
}

#[test]
fn test_toggle_energy_window_stays_outside_domain_command() {
    let command = DomainCommand::try_from(InputAction::ToggleEnergyWindow);
    assert!(matches!(command, Err(InputAction::ToggleEnergyWindow)));
}
