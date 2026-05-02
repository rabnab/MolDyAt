use super::InputAction;
use crate::application::DomainCommand;

impl TryFrom<InputAction> for DomainCommand {
    type Error = InputAction;

    fn try_from(action: InputAction) -> Result<Self, Self::Error> {
        match action {
            InputAction::AddBall { position } => Ok(DomainCommand::AddBall(position)),
            InputAction::NextBallSize => Ok(DomainCommand::NextBallSize),
            InputAction::AddRandomBalls { count } => Ok(DomainCommand::AddRandomBalls(count)),
            InputAction::StopAllBalls => Ok(DomainCommand::StopAllBalls),
            InputAction::ClearBalls => Ok(DomainCommand::ClearBalls),
            InputAction::TogglePause => Ok(DomainCommand::TogglePause),
            InputAction::ToggleStepwise => Ok(DomainCommand::ToggleStepwise),
            InputAction::RandomizeSpeeds => Ok(DomainCommand::RandomizeSpeeds),
            InputAction::EnableRandomBallSpeed => Ok(DomainCommand::EnableRandomBallSpeed),
            InputAction::DoubleTimeStep => Ok(DomainCommand::DoubleTimeStep),
            InputAction::HalveTimeStep => Ok(DomainCommand::HalveTimeStep),
            InputAction::IncreaseWaitTime => Ok(DomainCommand::IncreaseWaitTime),
            InputAction::DecreaseWaitTime => Ok(DomainCommand::DecreaseWaitTime),
            InputAction::ToggleRealData => Ok(DomainCommand::ToggleRealData),
            InputAction::ToggleTimingDisplay => Ok(DomainCommand::ToggleTimingDisplay),
            InputAction::ToggleBallStats => Ok(DomainCommand::ToggleBallStats),
            InputAction::StartVerbosityInput => Ok(DomainCommand::StartVerbosityInput),
            InputAction::OperatorAction(command) => Ok(DomainCommand::Operator(command)),
            other => Err(other),
        }
    }
}
