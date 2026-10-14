// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::operator_command::line::AddLineSegment;
use crate::application::persistence::LoadSource;
use crate::application::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
use crate::graphics::renderer::InputAction;
use crate::objects2d::Vec2D;
use std::convert::TryFrom;

#[test]
fn test_input_action_toggle_pause_maps_to_domain_command() {
    let command = DomainCommand::try_from(InputAction::TogglePause);
    assert!(matches!(
        command,
        Ok(DomainCommand::Sim(SimControl::TogglePause))
    ));
}

#[test]
fn test_input_action_toggle_gravity_maps_to_domain_command() {
    let command = DomainCommand::try_from(InputAction::ToggleGravity);
    assert!(matches!(
        command,
        Ok(DomainCommand::Sim(SimControl::ToggleGravity))
    ));
}

#[test]
fn test_input_action_add_ball_preserves_position() {
    let position = Vec2D::new(10.0, 20.0);
    let command = DomainCommand::try_from(InputAction::AddBall { position });

    assert!(
        matches!(command, Ok(DomainCommand::Balls(BallCommand::Add(mapped))) if mapped == position)
    );
}

#[test]
fn test_input_action_double_time_step_maps_to_domain_command() {
    let command = DomainCommand::try_from(InputAction::DoubleTimeStep);
    assert!(matches!(
        command,
        Ok(DomainCommand::Sim(SimControl::DoubleTimeStep))
    ));
}

#[test]
fn test_input_action_next_ball_size_maps_to_ball_command() {
    let command = DomainCommand::try_from(InputAction::NextBallSize);
    assert!(matches!(
        command,
        Ok(DomainCommand::Balls(BallCommand::NextSize))
    ));
}

#[test]
fn test_input_action_load_world_maps_to_world_command_with_latest_save() {
    let command = DomainCommand::try_from(InputAction::LoadWorld);
    assert!(matches!(
        command,
        Ok(DomainCommand::World(WorldCommand::Load(LoadSource::Latest)))
    ));
}

#[test]
fn test_input_action_toggle_ball_stats_maps_to_view_command() {
    let command = DomainCommand::try_from(InputAction::ToggleBallStats);
    assert!(matches!(
        command,
        Ok(DomainCommand::View(ViewCommand::ToggleBallStats))
    ));
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
