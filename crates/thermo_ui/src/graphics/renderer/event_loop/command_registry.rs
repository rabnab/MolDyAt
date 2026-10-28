// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The command registry: every keyboard command in one table.
//!
//! Each [`CommandDescriptor`] names a command (a stable id such as
//! `sim.toggle_pause`), describes it for the help text, binds it to keys and
//! says what it does: a [`UiAction`] that stays in the renderer, or a
//! [`DomainCommand`] for the physics thread. The key map (`key_map`, with
//! the key dispatch and the help text) starts from these default bindings,
//! and the UI-vs-domain routing reads this table, so adding a command means
//! one entry here plus
//! its handler (a `UiAction` arm in `action_processor`, or a `DomainCommand`
//! variant handled by `DomainCommandHandler`).
//!
//! Mouse input (placing balls, zoom, pan, operator tools) is not in the
//! registry; it carries positions and goes through the operator tools.

use super::key_map::display_names;
use super::key_names::bind_key_name;
use crate::graphics::energy_plot::MAX_PLOTS;
use Modifier::{Alt, Ctrl};
use Trigger::{Press, PressOnce, Release};
use thermo_physics::application::{HIGH_GRAVITY_MPS2, NORMAL_GRAVITY_MPS2};
use thermo_runtime::application::{DomainCommand, domain_command_spec};
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
    /// Sent to the physics thread: the id of its entry in the domain
    /// command table (`thermo_runtime::application::DOMAIN_COMMANDS`), which
    /// a headless script runs by the same id (#340). A key uses the
    /// default of the command's parameter.
    Domain(&'static str),
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
    /// The energy plot's time view (zoom and pan, #367) back to its default.
    ResetPlotView,
    /// Change the plots of the plot window (#339).
    Plot(PlotChange),
    /// Switch the plot window between the time series and the
    /// distributions (#343).
    TogglePlotDistributions,
    DeleteHovered,
}

/// A change of what the plot window shows (#339); the keys work in both
/// windows and act on the focused plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotChange {
    /// The next energy preset for the focused plot (`view.cycle_plot` in
    /// the plot window).
    CyclePreset,
    NextSeries,
    PreviousSeries,
    AddSeries,
    RemoveSeries,
    AddPlot,
    RemovePlot,
    FocusNext,
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
    pub key: BindKey,
    pub modifier: Modifier,
    pub trigger: Trigger,
    /// Not shown in the help text: an alias of a shown binding (`=` for
    /// `+`, the same key on US keyboards).
    pub hidden: bool,
}

/// What a binding matches.
///
/// Every key that types a character (letters, digits, symbols such as `+`
/// and `-`, the numpad) matches that character in the current layout, so a
/// shortcut sits where its character is printed on every layout: German `+`
/// is US `]`, AZERTY `A` is US `Q` (#327). Only keys that type no
/// character (SPACE, ESC, TAB, DEL, BACKSPACE, Shift, F1–F12) are physical
/// keys ([`KeyCode`], named after their place on a US keyboard).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindKey {
    Code(KeyCode),
    Symbol(char),
}

/// A key event as the registry sees it: the physical key, the character it
/// types in the current layout without modifiers (`symbol`), and the one it
/// types with the modifiers held (`typed`; Swiss `+` is Shift + 1). Both
/// are `None` for keys that type nothing, such as Shift or F1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyInput {
    pub code: KeyCode,
    pub symbol: Option<char>,
    pub typed: Option<char>,
}

impl KeyInput {
    /// A key typing `symbol`, with no modifier changing it.
    pub fn new(code: KeyCode, symbol: Option<char>) -> Self {
        Self {
            code,
            symbol,
            typed: symbol,
        }
    }

    /// The same key, typing `typed` with the modifiers held.
    pub fn with_typed(self, typed: Option<char>) -> Self {
        Self { typed, ..self }
    }
}

/// A key without a known character, e.g. in tests.
impl From<KeyCode> for KeyInput {
    fn from(code: KeyCode) -> Self {
        Self::new(code, None)
    }
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
    const fn new(key: BindKey, modifier: Modifier, trigger: Trigger) -> Self {
        Self {
            key,
            modifier,
            trigger,
            hidden: false,
        }
    }

    /// The same binding, left out of the help text.
    const fn hidden(self) -> Self {
        Self {
            hidden: true,
            ..self
        }
    }

    /// The key as the help text shows it, e.g. `Ctrl + S`.
    pub fn display(&self) -> String {
        let key = bind_key_name(self.key);
        match self.modifier {
            Modifier::None => key,
            Modifier::Ctrl => format!("Ctrl + {key}"),
            Modifier::Alt => format!("Alt + {key}"),
        }
    }
}

impl CommandDescriptor {
    /// The default bindings as the help text shows them, e.g.
    /// `DEL / BACKSPACE`. Keys with the same name (left and right Shift) are
    /// listed once; hidden aliases are left out.
    pub fn binding_display(&self) -> Vec<String> {
        display_names(self.default_binding.iter().copied())
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
            CommandKind::Domain(id) => domain_command_spec(id).map(|spec| spec.build(None)),
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
    ("{max_plots}", || MAX_PLOTS.to_string()),
];

const fn key(key: KeyCode, trigger: Trigger) -> KeyBinding {
    KeyBinding::new(BindKey::Code(key), Modifier::None, trigger)
}

/// A key by the character it types (see [`BindKey`]).
const fn symbol(symbol: char, trigger: Trigger) -> KeyBinding {
    KeyBinding::new(BindKey::Symbol(symbol), Modifier::None, trigger)
}

/// A key, given by the character it types (see [`BindKey`]), with a
/// modifier. The chord is decided when the key goes down with
/// the modifier held, and it owns that key until its release: a `Release`
/// chord fires on that release even if the modifier was let go first, and
/// the plain binding of the same key never fires for that press.
const fn symbol_chord(modifier: Modifier, symbol: char, trigger: Trigger) -> KeyBinding {
    KeyBinding::new(BindKey::Symbol(symbol), modifier, trigger)
}

/// Every keyboard command, in help-text order within each category.
pub static COMMANDS: &[CommandDescriptor] = &[
    // ── Simulation ──────────────────────────────────────────────────
    CommandDescriptor {
        id: "sim.toggle_pause",
        label: "Pause or resume the simulation.",
        category: CommandCategory::Simulation,
        default_binding: &[key(KeyCode::Space, Press)],
        kind: CommandKind::Domain("sim.toggle_pause"),
    },
    CommandDescriptor {
        id: "sim.toggle_stepwise",
        label: "Toggle step-by-step mode.",
        category: CommandCategory::Simulation,
        default_binding: &[symbol('s', Release)],
        kind: CommandKind::Domain("sim.toggle_stepwise"),
    },
    CommandDescriptor {
        id: "sim.cycle_gravity",
        label: "Cycle gravity: off -> {normal_gravity} -> {high_gravity}.",
        category: CommandCategory::Simulation,
        default_binding: &[symbol('g', Release)],
        kind: CommandKind::Domain("sim.cycle_gravity"),
    },
    CommandDescriptor {
        id: "sim.double_time_step",
        label: "Double the simulation time step.",
        category: CommandCategory::Simulation,
        default_binding: &[symbol('+', Release), symbol('=', Release).hidden()],
        kind: CommandKind::Domain("sim.double_time_step"),
    },
    CommandDescriptor {
        id: "sim.halve_time_step",
        label: "Halve the simulation time step.",
        category: CommandCategory::Simulation,
        default_binding: &[symbol('-', Release)],
        kind: CommandKind::Domain("sim.halve_time_step"),
    },
    CommandDescriptor {
        id: "sim.decrease_wait_time",
        label: "Decrease the wait time between steps.",
        category: CommandCategory::Simulation,
        default_binding: &[
            symbol_chord(Alt, '+', Release),
            symbol_chord(Alt, '=', Release).hidden(),
        ],
        kind: CommandKind::Domain("sim.decrease_wait_time"),
    },
    CommandDescriptor {
        id: "sim.increase_wait_time",
        label: "Increase the wait time between steps.",
        category: CommandCategory::Simulation,
        default_binding: &[symbol_chord(Alt, '-', Release)],
        kind: CommandKind::Domain("sim.increase_wait_time"),
    },
    // ── Balls ───────────────────────────────────────────────────────
    CommandDescriptor {
        id: "balls.clear",
        label: "Clear all balls.",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::Escape, Press)],
        kind: CommandKind::Domain("balls.clear"),
    },
    CommandDescriptor {
        id: "balls.stop_all",
        label: "Stop all balls (zero velocities).",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::F1, Release)],
        kind: CommandKind::Domain("balls.stop_all"),
    },
    CommandDescriptor {
        id: "balls.add_random",
        label: "Add 100 random balls.",
        category: CommandCategory::Balls,
        default_binding: &[key(KeyCode::F5, Release)],
        kind: CommandKind::Domain("balls.add_random"),
    },
    CommandDescriptor {
        id: "balls.next_species",
        label: "New balls and new source spheres get the next species (right click too).",
        category: CommandCategory::Balls,
        // A letter, not `]` / `[`: bindings are physical keys, and N is N on
        // QWERTY, QWERTZ and AZERTY alike.
        default_binding: &[symbol('n', Release)],
        kind: CommandKind::Domain("balls.next_species"),
    },
    CommandDescriptor {
        id: "balls.previous_species",
        label: "New balls and new source spheres get the previous species.",
        category: CommandCategory::Balls,
        default_binding: &[symbol_chord(Alt, 'n', Release)],
        kind: CommandKind::Domain("balls.previous_species"),
    },
    CommandDescriptor {
        id: "balls.randomize_speeds",
        label: "Randomize all ball speeds.",
        category: CommandCategory::Balls,
        default_binding: &[symbol('r', Release)],
        kind: CommandKind::Domain("balls.randomize_speeds"),
    },
    CommandDescriptor {
        id: "balls.enable_random_speed",
        label: "Enable random ball speed.",
        category: CommandCategory::Balls,
        default_binding: &[
            key(KeyCode::ShiftLeft, Press),
            key(KeyCode::ShiftRight, Press),
        ],
        kind: CommandKind::Domain("balls.enable_random_speed"),
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
        default_binding: &[symbol_chord(Ctrl, 's', PressOnce)],
        kind: CommandKind::Domain("world.save"),
    },
    CommandDescriptor {
        id: "world.load_latest",
        label: "Load the newest save from the save directory (paused, default view).",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Ctrl, 'o', PressOnce)],
        kind: CommandKind::Domain("world.load_latest"),
    },
    CommandDescriptor {
        id: "world.export_series_csv",
        label: "Export the metrics series (energies, wall momentum, species counts) as CSV to the save directory.",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Ctrl, 'e', PressOnce)],
        kind: CommandKind::Domain("world.export_series_csv"),
    },
    CommandDescriptor {
        id: "world.export_series_jsonl",
        label: "Export the metrics series as JSONL to the save directory.",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Ctrl, 'j', PressOnce)],
        kind: CommandKind::Domain("world.export_series_jsonl"),
    },
    CommandDescriptor {
        id: "world.export_snapshot_csv",
        label: "Export the state of every ball as CSV to the save directory.",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Alt, 'e', PressOnce)],
        kind: CommandKind::Domain("world.export_snapshot_csv"),
    },
    CommandDescriptor {
        id: "world.export_snapshot_jsonl",
        label: "Export the state of every ball as JSONL to the save directory.",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Alt, 'j', PressOnce)],
        kind: CommandKind::Domain("world.export_snapshot_jsonl"),
    },
    CommandDescriptor {
        id: "world.export_distributions_csv",
        label: "Export the speed histogram (with its Maxwell-Boltzmann counts) and g(r) per species pair as CSV to the save directory.",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Ctrl, 'g', PressOnce)],
        kind: CommandKind::Domain("world.export_distributions_csv"),
    },
    CommandDescriptor {
        id: "world.export_distributions_jsonl",
        label: "Export the speed histogram and g(r) as JSONL to the save directory.",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Alt, 'g', PressOnce)],
        kind: CommandKind::Domain("world.export_distributions_jsonl"),
    },
    CommandDescriptor {
        id: "world.toggle_sample_stream_csv",
        label: "Start or stop streaming every metrics sample as CSV to the save directory (keeps the whole run).",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Ctrl, 'r', PressOnce)],
        kind: CommandKind::Domain("world.toggle_sample_stream_csv"),
    },
    CommandDescriptor {
        id: "world.toggle_sample_stream_jsonl",
        label: "Start or stop streaming every metrics sample as JSONL to the save directory.",
        category: CommandCategory::World,
        default_binding: &[symbol_chord(Alt, 'r', PressOnce)],
        kind: CommandKind::Domain("world.toggle_sample_stream_jsonl"),
    },
    // ── View ────────────────────────────────────────────────────────
    CommandDescriptor {
        id: "view.reset",
        label: "Reset the viewport (1.0x zoom, centred).",
        category: CommandCategory::View,
        default_binding: &[symbol('0', Release)],
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
        label: "Cycle the field overlay: Off -> Grid -> Temperature -> Pressure -> Density -> Flow speed -> Flow arrows -> Off.",
        category: CommandCategory::View,
        default_binding: &[symbol('t', Release)],
        kind: CommandKind::Ui(UiAction::CycleThermoDisplay),
    },
    CommandDescriptor {
        id: "view.cycle_plot",
        label: "Cycle the plot content (in the plot window: the focused plot's energy preset).",
        category: CommandCategory::View,
        default_binding: &[symbol('p', Release)],
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
        id: "view.plot_distributions",
        label: "Switch the plot window between the time series and the distributions (speed histogram with Maxwell-Boltzmann, g(r)); opens it if closed.",
        category: CommandCategory::View,
        default_binding: &[symbol('m', Press)],
        kind: CommandKind::Ui(UiAction::TogglePlotDistributions),
    },
    CommandDescriptor {
        id: "view.reset_plot_view",
        label: "Reset the energy plot's time range (follow the newest sample; also a middle double click in the plot).",
        category: CommandCategory::View,
        default_binding: &[key(KeyCode::Home, PressOnce)],
        kind: CommandKind::Ui(UiAction::ResetPlotView),
    },
    CommandDescriptor {
        id: "view.plot_next_series",
        label: "Plot window: show the next series in place of the focused plot's last one.",
        category: CommandCategory::View,
        default_binding: &[symbol('.', Release)],
        kind: CommandKind::Ui(UiAction::Plot(PlotChange::NextSeries)),
    },
    CommandDescriptor {
        id: "view.plot_previous_series",
        label: "Plot window: show the previous series in place of the focused plot's last one.",
        category: CommandCategory::View,
        default_binding: &[symbol(',', Release)],
        kind: CommandKind::Ui(UiAction::Plot(PlotChange::PreviousSeries)),
    },
    CommandDescriptor {
        id: "view.plot_add_series",
        label: "Plot window: add the next series to the focused plot.",
        category: CommandCategory::View,
        default_binding: &[symbol_chord(Ctrl, 'p', PressOnce)],
        kind: CommandKind::Ui(UiAction::Plot(PlotChange::AddSeries)),
    },
    CommandDescriptor {
        id: "view.plot_remove_series",
        label: "Plot window: remove the last series of the focused plot.",
        category: CommandCategory::View,
        default_binding: &[symbol_chord(Alt, 'p', PressOnce)],
        kind: CommandKind::Ui(UiAction::Plot(PlotChange::RemoveSeries)),
    },
    CommandDescriptor {
        id: "view.plot_add_plot",
        label: "Plot window: add a plot below the focused one (up to {max_plots}).",
        category: CommandCategory::View,
        default_binding: &[symbol_chord(Ctrl, 'l', PressOnce)],
        kind: CommandKind::Ui(UiAction::Plot(PlotChange::AddPlot)),
    },
    CommandDescriptor {
        id: "view.plot_remove_plot",
        label: "Plot window: remove the focused plot.",
        category: CommandCategory::View,
        default_binding: &[symbol_chord(Alt, 'l', PressOnce)],
        kind: CommandKind::Ui(UiAction::Plot(PlotChange::RemovePlot)),
    },
    CommandDescriptor {
        id: "view.plot_focus_next",
        label: "Plot window: focus the next plot (or click a plot; click a legend entry to hide or show its series).",
        category: CommandCategory::View,
        default_binding: &[symbol('f', Release)],
        kind: CommandKind::Ui(UiAction::Plot(PlotChange::FocusNext)),
    },
    CommandDescriptor {
        id: "view.toggle_real_data",
        label: "Toggle the display of real data (SI units, see docs/units.md).",
        category: CommandCategory::View,
        default_binding: &[symbol('d', Release)],
        kind: CommandKind::Domain("view.toggle_real_data"),
    },
    CommandDescriptor {
        id: "view.toggle_ball_stats",
        label: "Toggle the ball statistics overlay.",
        category: CommandCategory::View,
        default_binding: &[symbol('b', Release)],
        kind: CommandKind::Domain("view.toggle_ball_stats"),
    },
    // ── Diagnostics ─────────────────────────────────────────────────
    CommandDescriptor {
        id: "debug.toggle_timings",
        label: "Toggle step timings in the log.",
        category: CommandCategory::Diagnostics,
        default_binding: &[key(KeyCode::F4, Release)],
        kind: CommandKind::Domain("debug.toggle_timings"),
    },
    CommandDescriptor {
        id: "debug.ball_verbosity",
        label: "Set a ball verbose: type its ID (or 'a' for all, '0' for none) and a mode \
                (c console, m main neighbours, o other neighbours, s speed) in the terminal.",
        category: CommandCategory::Diagnostics,
        default_binding: &[symbol('v', Release)],
        kind: CommandKind::Domain("debug.ball_verbosity"),
    },
    // ── Help ────────────────────────────────────────────────────────
    CommandDescriptor {
        id: "help.show",
        label: "Print this list of keys to the log.",
        category: CommandCategory::Help,
        default_binding: &[symbol('h', Release)],
        kind: CommandKind::Ui(UiAction::PrintHelp),
    },
];

/// The command with this id.
pub fn command_by_id(id: &str) -> Option<&'static CommandDescriptor> {
    COMMANDS.iter().find(|command| command.id == id)
}
