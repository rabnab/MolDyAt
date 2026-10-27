// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Command scripts for headless runs (#340): the same commands as the keys,
//! by their registry id, at chosen steps, so a scenario is a text file.
//!
//! ```text
//! # comments run to the end of the line
//! operator.place_thermostat -8 0
//! operator.toggle_thermostat 0
//! step 1000
//! balls.add_random 500; sim.cycle_gravity
//! step 2000
//! world.save
//! ```
//!
//! - One command per line, or several separated by `;`.
//! - `step N` runs N simulation steps; the run is as long as all `step`s
//!   together (`--script` replaces `--steps`).
//! - A domain command by its full registry id
//!   ([`crate::application::DOMAIN_COMMANDS`], those marked `in_scripts`),
//!   with its parameter if it takes one (`balls.add_random 500`; without it
//!   the key's default).
//! - Operator commands (`operator.*`, see [`OperatorAction`]) do what the
//!   sidebar tools do. They name a thermostat or source sphere by its place
//!   in creation order (`0` is the first placed).
//!
//! Commands before the first `step` set the world up: they run before the
//! first snapshot, so `run_start` and the frame at step 0 show them. A
//! command after `step N` runs after that step (and after its frame).

use crate::application::{DomainCommandSpec, domain_command_spec};
use std::fmt;
use std::path::Path;

/// A thermostat or source sphere command, as the sidebar tools give them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OperatorAction {
    /// `operator.place_thermostat X Y`
    PlaceThermostat { x: f32, y: f32 },
    /// `operator.toggle_thermostat N`
    ToggleThermostat { index: usize },
    /// `operator.adjust_thermostat_temperature N STEPS` (wheel notches)
    AdjustThermostatTemperature { index: usize, steps: f32 },
    /// `operator.adjust_thermostat_radius N STEPS` (wheel notches)
    AdjustThermostatRadius { index: usize, steps: f32 },
    /// `operator.place_source_sphere X Y`
    PlaceSourceSphere { x: f32, y: f32 },
    /// `operator.toggle_source_sphere N`
    ToggleSourceSphere { index: usize },
    /// `operator.adjust_source_sphere_speed N DELTA`
    AdjustSourceSphereSpeed { index: usize, delta: f32 },
    /// `operator.adjust_source_sphere_period N STEPS`
    AdjustSourceSpherePeriod { index: usize, delta_steps: i32 },
    /// `operator.cycle_source_sphere_species N`
    CycleSourceSphereSpecies { index: usize },
}

/// The operator commands: id and what their arguments are, for parsing and
/// the docs.
pub const OPERATOR_COMMANDS: &[(&str, &str)] = &[
    ("operator.place_thermostat", "X Y"),
    ("operator.toggle_thermostat", "N"),
    ("operator.adjust_thermostat_temperature", "N STEPS"),
    ("operator.adjust_thermostat_radius", "N STEPS"),
    ("operator.place_source_sphere", "X Y"),
    ("operator.toggle_source_sphere", "N"),
    ("operator.adjust_source_sphere_speed", "N DELTA"),
    ("operator.adjust_source_sphere_period", "N STEPS"),
    ("operator.cycle_source_sphere_species", "N"),
];

/// One command of a script.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptAction {
    /// Run this many steps.
    Step(u32),
    /// A domain command of the registry, with its parameter if given.
    Domain { id: &'static str, arg: Option<u64> },
    Operator {
        id: &'static str,
        action: OperatorAction,
    },
}

/// A command and the line it is on (1-based), for errors and records.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptItem {
    pub line: usize,
    pub action: ScriptAction,
    /// The command as written (normalised spaces), for the records.
    pub text: String,
}

/// A parsed script.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Script {
    pub items: Vec<ScriptItem>,
}

/// A script that cannot be run, with the line it is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ScriptError {}

/// A command that is due at a step: run it after that many steps.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledItem {
    pub at_step: u32,
    pub item: ScriptItem,
}

/// A command that ran: where it is in the script and at which simulation
/// step, for the `command` record.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRecord {
    /// The simulation step when it ran.
    pub step: u64,
    /// Its line in the script (1-based).
    pub line: usize,
    /// Its id (`balls.add_random`, `operator.place_thermostat`).
    pub id: String,
    /// The arguments as written.
    pub args: Vec<String>,
}

impl CommandRecord {
    pub(crate) fn of(item: &ScriptItem, step: u64) -> Self {
        let mut words = item.text.split_whitespace().map(str::to_string);
        Self {
            step,
            line: item.line,
            id: words.next().unwrap_or_default(),
            args: words.collect(),
        }
    }
}

impl Script {
    /// Read and parse the script at `path`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// Parse a script; the first bad command fails it.
    pub fn parse(text: &str) -> Result<Self, ScriptError> {
        let mut items = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let code = raw.split('#').next().unwrap_or_default();
            for statement in code.split(';') {
                let words: Vec<&str> = statement.split_whitespace().collect();
                let Some((&name, args)) = words.split_first() else {
                    continue;
                };
                let action =
                    parse_command(name, args).map_err(|message| ScriptError { line, message })?;
                items.push(ScriptItem {
                    line,
                    action,
                    text: words.join(" "),
                });
            }
        }
        let script = Self { items };
        script.total_steps_checked().map_err(|line| ScriptError {
            line,
            message: "the steps add up to more than 4294967295".into(),
        })?;
        Ok(script)
    }

    /// All `step`s together: the length of the run.
    pub fn total_steps(&self) -> u32 {
        self.total_steps_checked().unwrap_or(u32::MAX)
    }

    fn total_steps_checked(&self) -> Result<u32, usize> {
        self.items
            .iter()
            .try_fold(0_u32, |total, item| match item.action {
                ScriptAction::Step(n) => total.checked_add(n).ok_or(item.line),
                _ => Ok(total),
            })
    }

    /// The commands (not the `step`s), each with the number of steps run
    /// before it.
    pub fn schedule(&self) -> Vec<ScheduledItem> {
        let mut at_step = 0_u32;
        let mut scheduled = Vec::new();
        for item in &self.items {
            match item.action {
                ScriptAction::Step(n) => at_step = at_step.saturating_add(n),
                _ => scheduled.push(ScheduledItem {
                    at_step,
                    item: item.clone(),
                }),
            }
        }
        scheduled
    }
}

fn parse_command(name: &str, args: &[&str]) -> Result<ScriptAction, String> {
    if name == "step" {
        let [count] = args else {
            return Err("`step` takes one number of steps, e.g. `step 1000`".into());
        };
        let steps = count
            .parse::<u32>()
            .map_err(|_| format!("`step {count}`: the number of steps must be a whole number"))?;
        return Ok(ScriptAction::Step(steps));
    }
    if let Some(&(id, usage)) = OPERATOR_COMMANDS.iter().find(|(id, _)| *id == name) {
        return parse_operator(id, args)
            .map(|action| ScriptAction::Operator { id, action })
            .map_err(|problem| format!("`{name}`: {problem} (usage: `{id} {usage}`)"));
    }
    let Some(spec) = domain_command_spec(name) else {
        return Err(format!(
            "unknown command `{name}`: use `step N`, a registry id such as \
             `balls.add_random` or an `operator.*` command"
        ));
    };
    if !spec.in_scripts {
        return Err(format!(
            "`{name}` cannot run in a script (only sim.*, balls.*, world.save \
             and world.export_* can)"
        ));
    }
    parse_domain_arg(spec, args).map(|arg| ScriptAction::Domain { id: spec.id, arg })
}

fn parse_domain_arg(spec: &DomainCommandSpec, args: &[&str]) -> Result<Option<u64>, String> {
    match (spec.param, args) {
        (_, []) => Ok(None),
        (Some(param), [value]) => {
            let value = value
                .parse::<u64>()
                .map_err(|_| format!("`{}`: the {} must be a whole number", spec.id, param.name))?;
            if value > param.max {
                return Err(format!(
                    "`{}`: the {} must be at most {}",
                    spec.id, param.name, param.max
                ));
            }
            Ok(Some(value))
        }
        (Some(param), _) => Err(format!(
            "`{}` takes one {} (default {})",
            spec.id, param.name, param.default
        )),
        (None, _) => Err(format!("`{}` takes no arguments", spec.id)),
    }
}

fn number<T: std::str::FromStr>(text: &str, what: &str) -> Result<T, String> {
    text.parse::<T>()
        .map_err(|_| format!("`{text}` is not a valid {what}"))
}

fn finite(text: &str, what: &str) -> Result<f32, String> {
    let value: f32 = number(text, what)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("`{text}` is not a finite {what}"))
    }
}

fn parse_operator(id: &str, args: &[&str]) -> Result<OperatorAction, String> {
    let index = |text: &str| number::<usize>(text, "index (0 is the first placed)");
    let wrong = || "wrong number of arguments".to_string();
    use OperatorAction as A;
    Ok(match (id, args) {
        ("operator.place_thermostat", [x, y]) => A::PlaceThermostat {
            x: finite(x, "x")?,
            y: finite(y, "y")?,
        },
        ("operator.toggle_thermostat", [n]) => A::ToggleThermostat { index: index(n)? },
        ("operator.adjust_thermostat_temperature", [n, steps]) => A::AdjustThermostatTemperature {
            index: index(n)?,
            steps: finite(steps, "number of steps")?,
        },
        ("operator.adjust_thermostat_radius", [n, steps]) => A::AdjustThermostatRadius {
            index: index(n)?,
            steps: finite(steps, "number of steps")?,
        },
        ("operator.place_source_sphere", [x, y]) => A::PlaceSourceSphere {
            x: finite(x, "x")?,
            y: finite(y, "y")?,
        },
        ("operator.toggle_source_sphere", [n]) => A::ToggleSourceSphere { index: index(n)? },
        ("operator.adjust_source_sphere_speed", [n, delta]) => A::AdjustSourceSphereSpeed {
            index: index(n)?,
            delta: finite(delta, "speed change")?,
        },
        ("operator.adjust_source_sphere_period", [n, steps]) => A::AdjustSourceSpherePeriod {
            index: index(n)?,
            delta_steps: number(steps, "whole number of steps")?,
        },
        ("operator.cycle_source_sphere_species", [n]) => {
            A::CycleSourceSphereSpecies { index: index(n)? }
        }
        _ => return Err(wrong()),
    })
}

#[cfg(test)]
mod tests;
