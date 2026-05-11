use crate::application::operator_command::OperatorCommand;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};

/// Unified application-level command contract produced by UI/input code.
///
/// `DomainCommand` is the single command type used from input handling all the
/// way through to the physics thread. Renderer-side code emits `DomainCommand`
/// and the physics thread receives and handles it directly.
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
