// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::InputAction;
use crate::application::persistence::LoadSource;
use crate::application::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};

impl TryFrom<InputAction> for DomainCommand {
    type Error = InputAction;

    fn try_from(action: InputAction) -> Result<Self, Self::Error> {
        let command = match action {
            InputAction::TogglePause => SimControl::TogglePause.into(),
            InputAction::ToggleStepwise => SimControl::ToggleStepwise.into(),
            InputAction::ToggleGravity => SimControl::ToggleGravity.into(),
            InputAction::DoubleTimeStep => SimControl::DoubleTimeStep.into(),
            InputAction::HalveTimeStep => SimControl::HalveTimeStep.into(),
            InputAction::IncreaseWaitTime => SimControl::IncreaseWaitTime.into(),
            InputAction::DecreaseWaitTime => SimControl::DecreaseWaitTime.into(),

            InputAction::AddBall { position } => BallCommand::Add(position).into(),
            InputAction::AddRandomBalls { count } => BallCommand::AddRandom(count).into(),
            InputAction::ClearBalls => BallCommand::Clear.into(),
            InputAction::StopAllBalls => BallCommand::StopAll.into(),
            InputAction::RandomizeSpeeds => BallCommand::RandomizeSpeeds.into(),
            InputAction::EnableRandomBallSpeed => BallCommand::EnableRandomSpeed.into(),
            InputAction::NextBallSize => BallCommand::NextSize.into(),

            InputAction::SaveWorld => WorldCommand::Save.into(),
            InputAction::LoadWorld => WorldCommand::Load(LoadSource::Latest).into(),

            InputAction::ToggleRealData => ViewCommand::ToggleRealData.into(),
            InputAction::ToggleTimingDisplay => ViewCommand::ToggleTimingDisplay.into(),
            InputAction::ToggleBallStats => ViewCommand::ToggleBallStats.into(),
            InputAction::StartVerbosityInput => ViewCommand::StartVerbosityInput.into(),

            InputAction::OperatorAction(command) => DomainCommand::Operator(command),
            other => return Err(other),
        };
        Ok(command)
    }
}
