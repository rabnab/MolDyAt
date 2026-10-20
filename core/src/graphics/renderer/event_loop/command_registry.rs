// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The command registry: every keyboard command in one table.
//!
//! Each [`CommandDescriptor`] names a command (a stable id such as
//! `sim.toggle_pause`), describes it for the help text, binds it to keys and
//! says what it does: a [`UiAction`] that stays in the renderer, or a
//! [`DomainCommand`] for the physics thread. Key dispatch
//! ([`command_for_key`]), the help text ([`help_text`]) and the UI-vs-domain
//! routing all read this table, so adding a command means one entry here plus
//! its handler (a `UiAction` arm in `action_processor`, or a `DomainCommand`
//! variant handled by `DomainCommandHandler`).
//!
//! Mouse input (placing balls, zoom, pan, operator tools) is not in the
//! registry; it carries positions and goes through the operator tools.

use crate::application::persistence::LoadSource;
use crate::application::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
use crate::application::{HIGH_GRAVITY_MPS2, NORMAL_GRAVITY_MPS2};
use Modifier::{Alt, Ctrl};
use Trigger::{Press, PressOnce, Release};
use winit::keyboard::KeyCode;

/// One keyboard command.
#[derive(Debug)]
pub struct CommandDescriptor {
    /// Stable id (`<category>.<name>`) for key-binding files and headless
    /// scripts. Never rename one; add a new command instead.
    pub id: &'static str,
    /// One line for the help text and the README key table. It may name
    /// values such as `{normal_gravity}` (see [`LABEL_VALUES`]); read it
    /// with [`CommandDescriptor::label_text`].
    pub label: &'static str,
    pub category: CommandCategory,
    /// Keys that run the command; empty for a command without a key.
    pub default_binding: &'static [KeyBinding],
    pub kind: CommandKind,
}

/// What a command does.
#[derive(Debug, Clone, Copy)]
pub enum CommandKind {
    /// Handled in the renderer; never reaches the physics thread.
    Ui(UiAction),
    /// Builds the command sent to the physics thread.
    Domain(fn() -> DomainCommand),
}

/// Renderer-side commands, handled in `action_processor::run_ui_action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    PrintHelp,
    ResetView,
    ToggleRendering,
    /// Drawn by the backend's ball pipeline; the backend handles it.
    CycleCutoffRings,
    CycleThermoDisplay,
    CyclePlotContent,
    ToggleEnergyWindow,
    DeleteHovered,
}

/// Groups for the help text. Each id starts with its category's prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    Simulation,
    Balls,
    Tools,
    World,
    View,
    Diagnostics,
    Help,
}

impl CommandCategory {
    /// All categories in help-text order.
    pub const ALL: [CommandCategory; 7] = [
        Self::Simulation,
        Self::Balls,
        Self::Tools,
        Self::World,
        Self::View,
        Self::Diagnostics,
        Self::Help,
    ];

    /// Heading in the help text.
    pub fn title(self) -> &'static str {
        match self {
            Self::Simulation => "Simulation",
            Self::Balls => "Balls",
            Self::Tools => "Tools",
            Self::World => "Save / load",
            Self::View => "View",
            Self::Diagnostics => "Diagnostics",
            Self::Help => "Help",
        }
    }

    /// The prefix of the ids in this category (`sim` in `sim.toggle_pause`).
    pub fn id_prefix(self) -> &'static str {
        match self {
            Self::Simulation => "sim",
            Self::Balls => "balls",
            Self::Tools => "tools",
            Self::World => "world",
            Self::View => "view",
            Self::Diagnostics => "debug",
            Self::Help => "help",
        }
    }
}

/// A key, the modifier that must be held with it, and when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub modifier: Modifier,
    pub trigger: Trigger,
}

/// Modifier held with a key. A binding with a modifier wins over the plain
/// binding of the same key while the modifier is held; otherwise the plain
/// binding fires whatever modifiers are held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    None,
    Ctrl,
    Alt,
}

/// When a binding fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// On key press, again on every auto-repeat.
    Press,
    /// On key press, not on auto-repeat.
    PressOnce,
    /// On key release.
    Release,
}

impl KeyBinding {
    const fn new(key: KeyCode, modifier: Modifier, trigger: Trigger) -> Self {
        Self {
            key,
            modifier,
            trigger,
        }
    }

    /// The key as the help text shows it, e.g. `Ctrl + S`.
    pub fn display(&self) -> String {
        let key = key_name(self.key);
        match self.modifier {
            Modifier::None => key.to_string(),
            Modifier::Ctrl => format!("Ctrl + {key}"),
            Modifier::Alt => format!("Alt + {key}"),
        }
    }
}

impl CommandDescriptor {
    /// The bindings as the help text shows them, e.g. `DEL / BACKSPACE`.
    /// Keys with the same name (left and right Shift) are listed once.
    pub fn binding_display(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for binding in self.default_binding {
            let name = binding.display();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    /// The label with its values filled in (see [`LABEL_VALUES`]).
    pub fn label_text(&self) -> String {
        LABEL_VALUES
            .iter()
            .fold(self.label.to_string(), |text, (name, value)| {
                if text.contains(name) {
                    text.replace(name, &value())
                } else {
                    text
                }
            })
    }

    /// The command for the physics thread, if this is a domain command.
    pub fn domain_command(&self) -> Option<DomainCommand> {
        match self.kind {
            CommandKind::Domain(make) => Some(make()),
            CommandKind::Ui(_) => None,
        }
    }

    /// The renderer action, if this is a UI command.
    pub fn ui_action(&self) -> Option<UiAction> {
        match self.kind {
            CommandKind::Ui(action) => Some(action),
            CommandKind::Domain(_) => None,
        }
    }
}

/// A placeholder in a label and the function that gives its text.
pub type LabelValue = (&'static str, fn() -> String);

/// Values a label may name, so it never repeats a constant by hand.
pub const LABEL_VALUES: &[LabelValue] = &[
    ("{normal_gravity}", || {
        format!("{NORMAL_GRAVITY_MPS2:.2} m/s^2")
    }),
    ("{high_gravity}", || format!("{HIGH_GRAVITY_MPS2:.2} m/s^2")),
];

/// Help-text name of a key.
fn key_name(key: KeyCode) -> &'static str {
    match key {
        KeyCode::Space => "SPACE",
        KeyCode::Escape => "ESC",
        KeyCode::Tab => "TAB",
        KeyCode::Delete => "DEL",
        KeyCode::Backspace => "BACKSPACE",
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "Shift",
        KeyCode::Equal => "=",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Minus => "-",
        KeyCode::NumpadAdd => "Num +",
        KeyCode::NumpadSubtract => "Num -",
        KeyCode::Digit0 => "0",
        KeyCode::Numpad0 => "Num 0",
        KeyCode::F1 => "F1",
        KeyCode::F2 => "F2",
        KeyCode::F3 => "F3",
        KeyCode::F4 => "F4",
        KeyCode::F5 => "F5",
        KeyCode::KeyB => "B",
        KeyCode::KeyD => "D",
        KeyCode::KeyG => "G",
        KeyCode::KeyH => "H",
        KeyCode::KeyO => "O",
        KeyCode::KeyP => "P",
        KeyCode::KeyR => "R",
        KeyCode::KeyS => "S",
        KeyCode::KeyT => "T",
        KeyCode::KeyV => "V",
        // A new binding needs a name here; the registry tests catch a gap.
        _ => "?",
    }
}

const fn key(key: KeyCode, trigger: Trigger) -> KeyBinding {
    KeyBinding::new(key, Modifier::None, trigger)
}

/// A key with a modifier. The chord is decided when the key goes down with
/// the modifier held, and it owns that key until its release: a `Release`
/// chord fires on that release even if the modifier was let go first, and
/// the plain binding of the same key never fires for that press.
const fn chord(modifier: Modifier, key: KeyCode, trigger: Trigger) -> KeyBinding {
    KeyBinding::new(key, modifier, trigger)
}

/// Every keyboard command, in help-text order within each category.
pub static COMMANDS: &[CommandDescriptor] = &[
    // ── Simulation ──────────────────────────────────────────────────
    CommandDescriptor {
        id: "sim.toggle_pause",
        label: "Pause or resume the simulation.",
        category: CommandCategory::Simulation,
        default_binding: &[key(KeyCode::Space, Press)],
        kind: CommandKind::Domain(|| SimControl::TogglePause.into()),
    },
    CommandDescriptor {
        id: "sim.toggle_stepwise",
        label: "Toggle step-by-step mode.",
        category: CommandCategory::Simulation,
        default_binding: &[key(KeyCode::KeyS, Release)],
        kind: CommandKind::Domain(|| SimControl::ToggleStepwise.into()),
    },
    CommandDescriptor {
        id: "sim.cycle_gravity",
        label: "Cycle gravity: off -> {normal_gravity} -> {high_gravity}.",
        category: CommandCategory::Simulation,
        default_binding: &[key(KeyCode::KeyG, Release)],
        kind: CommandKind::Domain(|| SimControl::ToggleGravity.into()),
    },
    CommandDescriptor {
        id: "sim.double_time_step",
        label: "Double the simulation time step.",
        category: CommandCategory::Simulation,
        default_binding: &[
            key(KeyCode::Equal, Release),
            key(KeyCode::NumpadAdd, Release),
        ],
        kind: CommandKind::Domain(|| SimControl::DoubleTimeStep.into()),
    },
    CommandDescriptor {
        id: "sim.halve_time_step",
        label: "Halve the simulation time step.",
        category: CommandCategory::Simulation,
        default_binding: &[
            key(KeyCode::Minus, Release),
            key(KeyCode::NumpadSubtract, Release),
        ],
        kind: CommandKind::Domain(|| SimControl::HalveTimeStep.into()),
    },
    CommandDescriptor {
        id: "sim.decrease_wait_time",
        label: "Decrease the wait time between steps.",
        category: CommandCategory::Simulation,
        default_binding: &[
            chord(Alt, KeyCode::Equal, Release),
            chord(Alt, KeyCode::NumpadAdd, Release),
        ],
        kind: CommandKind::Domain(|| SimControl::DecreaseWaitTime.into()),
    },
    CommandDescriptor {
        id: "sim.increase_wait_time",
        label: "Increase the wait time between steps.",
        category: CommandCategory::Simulation,
        default_binding: &[
            chord(Alt, KeyCode::Minus, Release),
            chord(Alt, KeyCode::NumpadSubtract, Release),
        ],
        kind: CommandKind::Domain(|| SimControl::IncreaseWaitTime.into()),
    },
    // ── Balls ───────────────────────────────────────────────────────
    CommandDescriptor {
        id: "balls.clear",
        label: "Clear all balls.",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::Escape, Press)],
        kind: CommandKind::Domain(|| BallCommand::Clear.into()),
    },
    CommandDescriptor {
        id: "balls.stop_all",
        label: "Stop all balls (zero velocities).",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::F1, Release)],
        kind: CommandKind::Domain(|| BallCommand::StopAll.into()),
    },
    CommandDescriptor {
        id: "balls.add_random",
        label: "Add 100 random balls.",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::F5, Release)],
        kind: CommandKind::Domain(|| BallCommand::AddRandom(100).into()),
    },
    CommandDescriptor {
        id: "balls.next_species",
        label: "New balls and new source spheres get the next species (right click too).",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::BracketRight, Release)],
        kind: CommandKind::Domain(|| BallCommand::NextSpecies.into()),
    },
    CommandDescriptor {
        id: "balls.previous_species",
        label: "New balls and new source spheres get the previous species.",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::BracketLeft, Release)],
        kind: CommandKind::Domain(|| BallCommand::PreviousSpecies.into()),
    },
    CommandDescriptor {
        id: "balls.randomize_speeds",
        label: "Randomize all ball speeds.",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::KeyR, Release)],
        kind: CommandKind::Domain(|| BallCommand::RandomizeSpeeds.into()),
    },
    CommandDescriptor {
        id: "balls.enable_random_speed",
        label: "Enable random ball speed.",
        category: CommandCategory::Balls,
        default_binding: &[
            key(KeyCode::ShiftLeft, Press),
            key(KeyCode::ShiftRight, Press),
        ],
        kind: CommandKind::Domain(|| BallCommand::EnableRandomSpeed.into()),
    },
    // ── Tools ───────────────────────────────────────────────────────
    CommandDescriptor {
        id: "tools.delete_hovered",
        label: "Delete the highlighted object (hover it with its sidebar tool selected).",
        category: CommandCategory::Tools,
        // Backspace too: many laptop keyboards have no Delete key.
        default_binding: &[
            key(KeyCode::Delete, PressOnce),
            key(KeyCode::Backspace, PressOnce),
        ],
        kind: CommandKind::Ui(UiAction::DeleteHovered),
    },
    // ── Save / load ─────────────────────────────────────────────────
    CommandDescriptor {
        id: "world.save",
        label: "Save the world to a new file in the save directory (--save-dir, default ./saves).",
        category: CommandCategory::World,
        default_binding: &[chord(Ctrl, KeyCode::KeyS, PressOnce)],
        kind: CommandKind::Domain(|| WorldCommand::Save.into()),
    },
    CommandDescriptor {
        id: "world.load_latest",
        label: "Load the newest save from the save directory (paused, default view).",
        category: CommandCategory::World,
        default_binding: &[chord(Ctrl, KeyCode::KeyO, PressOnce)],
        kind: CommandKind::Domain(|| WorldCommand::Load(LoadSource::Latest).into()),
    },
    // ── View ────────────────────────────────────────────────────────
    CommandDescriptor {
        id: "view.reset",
        label: "Reset the viewport (1.0x zoom, centred).",
        category: CommandCategory::View,
        default_binding: &[
            key(KeyCode::Digit0, Release),
            key(KeyCode::Numpad0, Release),
        ],
        kind: CommandKind::Ui(UiAction::ResetView),
    },
    CommandDescriptor {
        id: "view.toggle_rendering",
        label: "Toggle rendering.",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::F2, Release)],
        kind: CommandKind::Ui(UiAction::ToggleRendering),
    },
    CommandDescriptor {
        id: "view.cycle_cutoff_rings",
        label: "Cycle the cutoff-radius rings.",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::F3, Release)],
        kind: CommandKind::Ui(UiAction::CycleCutoffRings),
    },
    CommandDescriptor {
        id: "view.cycle_thermo_overlay",
        label: "Cycle the thermo overlay: Off -> Grid -> Temperature -> Pressure -> Off.",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::KeyT, Release)],
        kind: CommandKind::Ui(UiAction::CycleThermoDisplay),
    },
    CommandDescriptor {
        id: "view.cycle_plot",
        label: "Cycle the plot content.",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::KeyP, Release)],
        kind: CommandKind::Ui(UiAction::CyclePlotContent),
    },
    CommandDescriptor {
        id: "view.toggle_energy_window",
        label: "Toggle the energy plot window.",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::Tab, Release)],
        kind: CommandKind::Ui(UiAction::ToggleEnergyWindow),
    },
    CommandDescriptor {
        id: "view.toggle_real_data",
        label: "Toggle the display of real data (SI units, see docs/units.md).",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::KeyD, Release)],
        kind: CommandKind::Domain(|| ViewCommand::ToggleRealData.into()),
    },
    CommandDescriptor {
        id: "view.toggle_ball_stats",
        label: "Toggle the ball statistics overlay.",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::KeyB, Release)],
        kind: CommandKind::Domain(|| ViewCommand::ToggleBallStats.into()),
    },
    // ── Diagnostics ─────────────────────────────────────────────────
    CommandDescriptor {
        id: "debug.toggle_timings",
        label: "Toggle step timings in the log.",
        category: CommandCategory::Diagnostics,
        default_binding: &[key(KeyCode::F4, Release)],
        kind: CommandKind::Domain(|| ViewCommand::ToggleTimingDisplay.into()),
    },
    CommandDescriptor {
        id: "debug.ball_verbosity",
        label: "Set a ball verbose: type its ID (or 'a' for all, '0' for none) and a mode \
                (c console, m main neighbours, o other neighbours, s speed) in the terminal.",
        category: CommandCategory::Diagnostics,
        default_binding: &[key(KeyCode::KeyV, Release)],
        kind: CommandKind::Domain(|| ViewCommand::StartVerbosityInput.into()),
    },
    // ── Help ────────────────────────────────────────────────────────
    CommandDescriptor {
        id: "help.show",
        label: "Print this list of keys to the log.",
        category: CommandCategory::Help,
        default_binding: &[key(KeyCode::KeyH, Release)],
        kind: CommandKind::Ui(UiAction::PrintHelp),
    },
];

/// The command with this id.
pub fn command_by_id(id: &str) -> Option<&'static CommandDescriptor> {
    COMMANDS.iter().find(|command| command.id == id)
}

/// Whether a key event is a press (or auto-repeat) or a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPhase {
    Down,
    Up,
}

/// Modifiers held during a key event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldModifiers {
    pub ctrl: bool,
    pub alt: bool,
}

impl HeldModifiers {
    fn holds(self, modifier: Modifier) -> bool {
        match modifier {
            Modifier::None => true,
            Modifier::Ctrl => self.ctrl,
            Modifier::Alt => self.alt,
        }
    }
}

/// The command a key event runs, with the binding that matched.
///
/// Of the bindings for `key` that fire in `phase`, a held modifier's binding
/// wins (Ctrl before Alt); otherwise the plain binding fires. Auto-repeat is
/// not filtered here: the caller drops a repeated [`Trigger::PressOnce`].
/// `InputHandler` decides chords with [`chord_for_key`] on the first press
/// and looks up everything else here with no modifiers held.
pub fn command_for_key(
    key: KeyCode,
    phase: KeyPhase,
    held: HeldModifiers,
) -> Option<(&'static CommandDescriptor, KeyBinding)> {
    let fires_in_phase = |trigger: Trigger| match phase {
        KeyPhase::Down => trigger != Release,
        KeyPhase::Up => trigger == Release,
    };
    let find = |modifier: Modifier| {
        COMMANDS.iter().find_map(|command| {
            command
                .default_binding
                .iter()
                .find(|b| b.key == key && b.modifier == modifier && fires_in_phase(b.trigger))
                .map(|b| (command, *b))
        })
    };
    [Ctrl, Alt, Modifier::None]
        .into_iter()
        .filter(|&modifier| held.holds(modifier))
        .find_map(find)
}

/// The chord a key press starts: the binding of `key` whose modifier is
/// held (Ctrl before Alt), whatever its trigger. See [`chord`].
pub fn chord_for_key(
    key: KeyCode,
    held: HeldModifiers,
) -> Option<(&'static CommandDescriptor, KeyBinding)> {
    [Ctrl, Alt]
        .into_iter()
        .filter(|&modifier| held.holds(modifier))
        .find_map(|modifier| {
            COMMANDS.iter().find_map(|command| {
                command
                    .default_binding
                    .iter()
                    .find(|b| b.key == key && b.modifier == modifier)
                    .map(|b| (command, *b))
            })
        })
}

/// The help text (key `H`), generated from [`COMMANDS`].
pub fn help_text() -> String {
    let mut text = String::from("HELP: Available keyboard shortcuts:");
    for category in CommandCategory::ALL {
        let commands: Vec<_> = COMMANDS
            .iter()
            .filter(|command| command.category == category && !command.default_binding.is_empty())
            .collect();
        if commands.is_empty() {
            continue;
        }
        text.push_str(&format!("\n{}:", category.title()));
        for command in commands {
            let keys = command.binding_display().join(" / ");
            text.push_str(&format!("\n  {keys:<22} {}", command.label_text()));
        }
    }
    text
}
