// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::operator_command::OperatorCommand;
use crate::application::persistence::LoadSource;
use crate::application::scene_layers::SceneLayers;
use crate::export::{ExportFormat, ExportKind};
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};

/// Unified application-level command contract produced by UI/input code.
///
/// `DomainCommand` is the single command type used from input handling all the
/// way through to the physics thread. Renderer-side code emits `DomainCommand`
/// and the physics thread receives and handles it directly, one group at a
/// time (see `DomainCommandHandler`).
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum DomainCommand {
    /// Running the simulation: pause, step mode, time step, gravity.
    Sim(SimControl),
    /// Adding, removing and changing balls.
    Balls(BallCommand),
    /// The world as a whole: its size, saving and loading.
    World(WorldCommand),
    /// What the simulation reports and shows; no effect on physics.
    View(ViewCommand),
    Operator(Box<dyn OperatorCommand>),
}

/// Commands that control how the simulation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimControl {
    Pause,
    Resume,
    TogglePause,
    ToggleStepwise,
    ToggleGravity,
    DoubleTimeStep,
    HalveTimeStep,
    IncreaseWaitTime,
    DecreaseWaitTime,
}

/// Commands that add, remove or change balls.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum BallCommand {
    Add(Vec2D),
    AddWithVelocity(Ball2D),
    AddRandom(usize),
    Clear,
    StopAll,
    RandomizeSpeeds,
    EnableRandomSpeed,
    /// New balls get the next species of the world's table (wrapping).
    NextSpecies,
    /// New balls get the previous species of the world's table (wrapping).
    PreviousSpecies,
}

/// Commands on the world as a whole.
#[derive(Debug)]
pub enum WorldCommand {
    Resize {
        world_rect: Rect,
    },
    ResumeAfterResize,
    /// Write the world to a new save (reported as a `PhysicsEvent`).
    Save,
    /// Replace the world with a save (reported as a `PhysicsEvent`).
    Load(LoadSource),
    /// Write the metrics series or a per-ball snapshot dump to the save
    /// directory (reported as a `PhysicsEvent`).
    Export {
        kind: ExportKind,
        format: ExportFormat,
    },
}

/// Commands that change what is reported or shown, not the physics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewCommand {
    ToggleRealData,
    ToggleTimingDisplay,
    ToggleBallStats,
    StartVerbosityInput,
    /// Build snapshots with only these layers (F2 drops the heavy ones).
    SetSnapshotLayers(SceneLayers),
}

impl From<SimControl> for DomainCommand {
    fn from(command: SimControl) -> Self {
        Self::Sim(command)
    }
}

impl From<BallCommand> for DomainCommand {
    fn from(command: BallCommand) -> Self {
        Self::Balls(command)
    }
}

impl From<WorldCommand> for DomainCommand {
    fn from(command: WorldCommand) -> Self {
        Self::World(command)
    }
}

impl From<ViewCommand> for DomainCommand {
    fn from(command: ViewCommand) -> Self {
        Self::View(command)
    }
}
