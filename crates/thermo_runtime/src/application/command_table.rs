// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The domain half of the command registry (#340): every command id that
//! changes the simulation (or saves, loads, exports) and the
//! [`DomainCommand`] it stands for. The UI's key registry
//! (`thermo_ui::…::command_registry`) refers to these entries by id, and a
//! headless command script (`crate::script`) runs them by id too, so a key
//! and a script line do the same thing.
//!
//! A command may take one whole-number parameter (`balls.add_random` takes
//! the ball count): a key uses its default, a script may give it.

use super::domain_command::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
use crate::export::{ExportFormat, ExportKind};
use thermo_physics::application::persistence::LoadSource;

/// The whole-number parameter of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandParam {
    /// What it counts, for messages (`count`).
    pub name: &'static str,
    /// The value a key uses.
    pub default: u64,
    /// The largest value a script may give.
    pub max: u64,
}

/// One domain command.
#[derive(Debug, Clone, Copy)]
pub struct DomainCommandSpec {
    /// Stable id (`<category>.<name>`), shared with the key registry.
    pub id: &'static str,
    pub param: Option<CommandParam>,
    /// Whether a headless script may run it: commands that change the
    /// simulation (gravity, time step, `balls.*`) and the ones that write
    /// files (`world.save`, `world.export_*`). Pause, stepwise mode, the wait
    /// time, loading, the sample stream and display commands are for the
    /// window.
    pub in_scripts: bool,
    make: fn(u64) -> DomainCommand,
}

impl DomainCommandSpec {
    /// The command, with `value` for its parameter (the default without
    /// one; ignored by a command without a parameter).
    pub fn build(&self, value: Option<u64>) -> DomainCommand {
        let value = value.or(self.param.map(|p| p.default)).unwrap_or(0);
        (self.make)(value)
    }
}

const fn spec(
    id: &'static str,
    in_scripts: bool,
    make: fn(u64) -> DomainCommand,
) -> DomainCommandSpec {
    DomainCommandSpec {
        id,
        param: None,
        in_scripts,
        make,
    }
}

fn export(kind: ExportKind, format: ExportFormat) -> DomainCommand {
    WorldCommand::Export { kind, format }.into()
}

fn toggle_stream(format: ExportFormat) -> DomainCommand {
    WorldCommand::ToggleSampleStream { format }.into()
}

/// Every domain command, in the order of the key registry.
///
/// Pause, stepwise mode and the wait time are not scriptable: a headless run
/// steps the world itself, so a paused world would turn the script's
/// remaining steps into no-ops, and there is no wait between steps.
pub const DOMAIN_COMMANDS: &[DomainCommandSpec] = &[
    spec("sim.toggle_pause", false, |_| {
        SimControl::TogglePause.into()
    }),
    spec("sim.toggle_stepwise", false, |_| {
        SimControl::ToggleStepwise.into()
    }),
    spec("sim.cycle_gravity", true, |_| {
        SimControl::ToggleGravity.into()
    }),
    spec("sim.double_time_step", true, |_| {
        SimControl::DoubleTimeStep.into()
    }),
    spec("sim.halve_time_step", true, |_| {
        SimControl::HalveTimeStep.into()
    }),
    spec("sim.decrease_wait_time", false, |_| {
        SimControl::DecreaseWaitTime.into()
    }),
    spec("sim.increase_wait_time", false, |_| {
        SimControl::IncreaseWaitTime.into()
    }),
    spec("balls.clear", true, |_| BallCommand::Clear.into()),
    spec("balls.stop_all", true, |_| BallCommand::StopAll.into()),
    DomainCommandSpec {
        id: "balls.add_random",
        param: Some(CommandParam {
            name: "count",
            default: 100,
            max: 1_000_000,
        }),
        in_scripts: true,
        make: |count| BallCommand::AddRandom(count as usize).into(),
    },
    spec("balls.next_species", true, |_| {
        BallCommand::NextSpecies.into()
    }),
    spec("balls.previous_species", true, |_| {
        BallCommand::PreviousSpecies.into()
    }),
    spec("balls.randomize_speeds", true, |_| {
        BallCommand::RandomizeSpeeds.into()
    }),
    spec("balls.enable_random_speed", true, |_| {
        BallCommand::EnableRandomSpeed.into()
    }),
    spec("world.save", true, |_| WorldCommand::Save.into()),
    spec("world.load_latest", false, |_| {
        WorldCommand::Load(LoadSource::Latest).into()
    }),
    spec("world.export_series_csv", true, |_| {
        export(ExportKind::Series, ExportFormat::Csv)
    }),
    spec("world.export_series_jsonl", true, |_| {
        export(ExportKind::Series, ExportFormat::Jsonl)
    }),
    spec("world.export_snapshot_csv", true, |_| {
        export(ExportKind::Snapshot, ExportFormat::Csv)
    }),
    spec("world.export_snapshot_jsonl", true, |_| {
        export(ExportKind::Snapshot, ExportFormat::Jsonl)
    }),
    spec("world.export_distributions_csv", true, |_| {
        export(ExportKind::Distributions, ExportFormat::Csv)
    }),
    spec("world.export_distributions_jsonl", true, |_| {
        export(ExportKind::Distributions, ExportFormat::Jsonl)
    }),
    spec("world.toggle_sample_stream_csv", false, |_| {
        toggle_stream(ExportFormat::Csv)
    }),
    spec("world.toggle_sample_stream_jsonl", false, |_| {
        toggle_stream(ExportFormat::Jsonl)
    }),
    spec("view.toggle_real_data", false, |_| {
        ViewCommand::ToggleRealData.into()
    }),
    spec("view.toggle_ball_stats", false, |_| {
        ViewCommand::ToggleBallStats.into()
    }),
    spec("debug.toggle_timings", false, |_| {
        ViewCommand::ToggleTimingDisplay.into()
    }),
    spec("debug.ball_verbosity", false, |_| {
        ViewCommand::StartVerbosityInput.into()
    }),
];

/// The domain command with `id`, if there is one.
pub fn domain_command_spec(id: &str) -> Option<&'static DomainCommandSpec> {
    DOMAIN_COMMANDS.iter().find(|spec| spec.id == id)
}
