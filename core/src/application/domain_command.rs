use crate::application::operator_command::OperatorCommand;
use crate::application::physics_thread::PhysicsCommand;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};

/// Unified application-level command contract produced by UI/input code.
///
/// During the migration, renderer-side code emits `DomainCommand` and the
/// existing physics-thread command path adapts from it into `PhysicsCommand`.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum DomainCommand {
    Pause,
    Resume,
    TogglePause,
    AddBall(Vec2D),
    AddBallWithVelocity(Ball2D),
    AddRandomBalls(usize),
    ClearBalls,
    StopAllBalls,
    RandomizeSpeeds,
    EnableRandomBallSpeed,
    NextBallSize,
    DoubleTimeStep,
    HalveTimeStep,
    IncreaseWaitTime,
    DecreaseWaitTime,
    ToggleRealData,
    ToggleStepwise,
    ToggleTimingDisplay,
    ToggleBallStats,
    StartVerbosityInput,
    Resize { world_rect: Rect },
    ResumeAfterResize,
    Operator(Box<dyn OperatorCommand>),
}

impl From<DomainCommand> for PhysicsCommand {
    fn from(command: DomainCommand) -> Self {
        match command {
            DomainCommand::Pause => PhysicsCommand::Pause,
            DomainCommand::Resume => PhysicsCommand::Resume,
            DomainCommand::TogglePause => PhysicsCommand::TogglePause,
            DomainCommand::AddBall(pos) => PhysicsCommand::AddBall(pos),
            DomainCommand::AddBallWithVelocity(ball) => PhysicsCommand::AddBallWithVelocity(ball),
            DomainCommand::AddRandomBalls(count) => PhysicsCommand::AddRandomBalls(count),
            DomainCommand::ClearBalls => PhysicsCommand::ClearBalls,
            DomainCommand::StopAllBalls => PhysicsCommand::StopAllBalls,
            DomainCommand::RandomizeSpeeds => PhysicsCommand::RandomizeSpeeds,
            DomainCommand::EnableRandomBallSpeed => PhysicsCommand::EnableRandomBallSpeed,
            DomainCommand::NextBallSize => PhysicsCommand::NextBallSize,
            DomainCommand::DoubleTimeStep => PhysicsCommand::DoubleTimeStep,
            DomainCommand::HalveTimeStep => PhysicsCommand::HalveTimeStep,
            DomainCommand::IncreaseWaitTime => PhysicsCommand::IncreaseWaitTime,
            DomainCommand::DecreaseWaitTime => PhysicsCommand::DecreaseWaitTime,
            DomainCommand::ToggleRealData => PhysicsCommand::ToggleRealData,
            DomainCommand::ToggleStepwise => PhysicsCommand::ToggleStepwise,
            DomainCommand::ToggleTimingDisplay => PhysicsCommand::ToggleTimingDisplay,
            DomainCommand::ToggleBallStats => PhysicsCommand::ToggleBallStats,
            DomainCommand::StartVerbosityInput => PhysicsCommand::StartVerbosityInput,
            DomainCommand::Resize { world_rect } => PhysicsCommand::Resize { world_rect },
            DomainCommand::ResumeAfterResize => PhysicsCommand::ResumeAfterResize,
            DomainCommand::Operator(command) => PhysicsCommand::Operator(command),
        }
    }
}
