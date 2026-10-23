// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Invariants of the command table: unique ids and bindings, labels, the
//! help text and the README key table.

use super::super::command_registry::{
    BindKey, COMMANDS, CommandCategory, HeldModifiers, KeyBinding, KeyInput, KeyPhase,
    LABEL_VALUES, Modifier, Trigger, command_by_id, command_for_key, help_text,
};
use super::super::input_handler::{InputHandler, KeyCode};
use std::collections::HashSet;

#[test]
fn test_command_ids_are_unique() {
    let mut seen = HashSet::new();
    for command in COMMANDS {
        assert!(seen.insert(command.id), "duplicate id {}", command.id);
    }
}

#[test]
fn test_command_ids_start_with_their_category_prefix() {
    for command in COMMANDS {
        let (prefix, name) = command
            .id
            .split_once('.')
            .unwrap_or_else(|| panic!("{} has no category prefix", command.id));
        assert_eq!(prefix, command.category.id_prefix(), "{}", command.id);
        assert!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{} must be snake_case",
            command.id
        );
    }
}

#[test]
fn test_no_duplicate_bindings() {
    // A key with the same modifier may run only one command, even with a
    // different trigger: press and release would both fire.
    let mut seen = HashSet::new();
    for command in COMMANDS {
        for binding in command.default_binding {
            assert!(
                seen.insert((binding.key, binding.modifier)),
                "{} reuses the binding {}",
                command.id,
                binding.display()
            );
        }
    }
}

#[test]
fn test_every_command_has_a_label_and_a_key_with_a_name() {
    for command in COMMANDS {
        assert!(
            !command.label.trim().is_empty(),
            "{} has no label",
            command.id
        );
        assert!(
            !command.default_binding.is_empty(),
            "{} has no key",
            command.id
        );
        for binding in command.default_binding {
            assert!(
                !binding.display().contains('?'),
                "{} binds a key without a help name: {:?}",
                command.id,
                binding.key
            );
        }
    }
}

/// A key event for `key`. A symbol is typed on a key that has no binding
/// of its own (`IntlBackslash`), as on a layout that moves the symbol.
fn input_for(key: BindKey) -> KeyInput {
    match key {
        BindKey::Code(code) => super::us(code),
        BindKey::Symbol(symbol) => KeyInput::new(KeyCode::IntlBackslash, Some(symbol)),
    }
}

#[test]
fn test_symbol_test_key_has_no_binding() {
    for command in COMMANDS {
        for binding in command.default_binding {
            assert_ne!(binding.key, BindKey::Code(KeyCode::IntlBackslash));
        }
    }
}

/// Shortcuts follow the character a key types, not its place: letters,
/// digits and symbols sit where they are printed on every layout (#354
/// decision). Keys that type no character stay physical.
#[test]
fn test_shortcuts_follow_the_character_typed() {
    let up = |key| {
        command_for_key(key, KeyPhase::Up, HeldModifiers::default()).map(|(command, _)| command.id)
    };
    // AZERTY: the US `Q` key types `a`, the US `A` key types `q`; `G`
    // cycles gravity from the key that types `g`, wherever it is.
    assert_eq!(up(KeyInput::new(KeyCode::KeyQ, Some('a'))), None);
    assert_eq!(
        up(KeyInput::new(KeyCode::KeyG, Some('g'))),
        Some("sim.cycle_gravity")
    );
    // A layout whose US `0` key types `-` halves dt with it.
    assert_eq!(
        up(KeyInput::new(KeyCode::Digit0, Some('-'))),
        Some("sim.halve_time_step")
    );
    // AZERTY types `0` with Shift on that key (`à` without): reset view.
    assert_eq!(
        up(KeyInput::new(KeyCode::Digit0, Some('à')).with_typed(Some('0'))),
        Some("view.reset")
    );
    // The numpad types the same characters as the main row.
    assert_eq!(
        up(KeyInput::new(KeyCode::Numpad0, Some('0'))),
        Some("view.reset")
    );
    // TAB types no character and stays physical.
    assert_eq!(up(KeyCode::Tab.into()), Some("view.toggle_energy_window"));
}

/// `=` is a hidden alias of `+`: it dispatches, but the help shows `+`.
#[test]
fn test_hidden_alias_dispatches_but_is_not_shown() {
    let double = command_by_id("sim.double_time_step").expect("command");
    assert_eq!(double.binding_display(), ["+"]);
    let (found, _) = command_for_key(
        KeyInput::new(KeyCode::Equal, Some('=')),
        KeyPhase::Up,
        HeldModifiers::default(),
    )
    .expect("= dispatches");
    assert_eq!(found.id, "sim.double_time_step");
}

/// The ids of the commands one chord press runs: modifier down, key down,
/// then both up, the modifier first if `modifier_up_first`.
fn chord_press(binding: &KeyBinding, modifier_up_first: bool) -> Vec<&'static str> {
    let modifier_key = match binding.modifier {
        Modifier::Ctrl => KeyCode::ControlLeft,
        Modifier::Alt => KeyCode::AltLeft,
        Modifier::None => unreachable!("not a chord"),
    };
    let mut handler = InputHandler::new();
    let mut actions = vec![handler.handle_key_down(modifier_key, false)];
    let key = input_for(binding.key);
    actions.push(handler.handle_key_down(key, false));
    if modifier_up_first {
        actions.push(handler.handle_key_up(modifier_key));
        actions.push(handler.handle_key_up(key));
    } else {
        actions.push(handler.handle_key_up(key));
        actions.push(handler.handle_key_up(modifier_key));
    }
    actions
        .into_iter()
        .flatten()
        .map(|action| action.command().expect("a registry command").id)
        .collect()
}

#[test]
fn test_a_chord_runs_once_and_never_its_plain_key() {
    // Whatever the trigger, and even when the modifier is let go before the
    // key (Alt+= must not fall through to = = double the time step).
    for command in COMMANDS {
        for binding in command.default_binding {
            if binding.modifier == Modifier::None {
                continue;
            }
            for modifier_up_first in [false, true] {
                assert_eq!(
                    chord_press(binding, modifier_up_first),
                    [command.id],
                    "{} (modifier released first: {modifier_up_first})",
                    binding.display()
                );
            }
        }
    }
}

#[test]
fn test_ctrl_chords_ignore_auto_repeat() {
    for command in COMMANDS {
        for binding in command.default_binding {
            if binding.modifier == Modifier::Ctrl {
                assert_eq!(binding.trigger, Trigger::PressOnce, "{}", command.id);
            }
        }
    }
}

#[test]
fn test_every_binding_dispatches_to_its_command() {
    for command in COMMANDS {
        for binding in command.default_binding {
            let phase = match binding.trigger {
                Trigger::Press | Trigger::PressOnce => KeyPhase::Down,
                Trigger::Release => KeyPhase::Up,
            };
            let held = HeldModifiers {
                ctrl: binding.modifier == Modifier::Ctrl,
                alt: binding.modifier == Modifier::Alt,
            };
            let (found, _) = command_for_key(input_for(binding.key), phase, held)
                .unwrap_or_else(|| panic!("{} does not dispatch", binding.display()));
            assert_eq!(found.id, command.id, "{}", binding.display());
        }
    }
}

#[test]
fn test_every_command_appears_in_the_help_text() {
    let help = help_text();
    for command in COMMANDS {
        let keys = command.binding_display().join(" / ");
        let label = command.label_text();
        let line = help
            .lines()
            .find(|line| line.trim_start().starts_with(&keys) && line.ends_with(&label))
            .map(str::to_owned);
        assert!(
            line.is_some(),
            "{} ({keys}: {label}) is missing from the help text:\n{help}",
            command.id,
        );
    }
    for category in CommandCategory::ALL {
        if COMMANDS.iter().any(|c| c.category == category) {
            assert!(
                help.contains(&format!("\n{}:", category.title())),
                "{category:?}"
            );
        }
    }
}

#[test]
fn test_labels_name_only_known_values() {
    for command in COMMANDS {
        let text = command.label_text();
        assert!(
            !text.contains('{') && !text.contains('}'),
            "{} names an unknown value: {text}",
            command.id
        );
    }
    // Every value is used, so none goes stale.
    for (name, _) in LABEL_VALUES {
        assert!(
            COMMANDS.iter().any(|c| c.label.contains(name)),
            "{name} is unused"
        );
    }
}

#[test]
fn test_command_by_id_finds_every_command_and_nothing_else() {
    for command in COMMANDS {
        assert!(std::ptr::eq(
            command_by_id(command.id).expect("registered"),
            command
        ));
    }
    assert!(command_by_id("sim.no_such_command").is_none());
}

/// The keyboard rows of the README controls table, generated from the
/// registry.
fn readme_key_rows() -> String {
    let mut rows = String::new();
    for category in CommandCategory::ALL {
        for command in COMMANDS.iter().filter(|c| c.category == category) {
            let keys: Vec<String> = command
                .binding_display()
                .iter()
                .map(|key| format!("`{key}`"))
                .collect();
            rows.push_str(&format!(
                "| {} | {} |\n",
                keys.join(" / "),
                command.label_text()
            ));
        }
    }
    rows
}

#[test]
fn test_readme_key_table_matches_the_registry() {
    const START: &str =
        "<!-- key-table:start (generated from COMMANDS, see command_registry.rs) -->\n";
    const END: &str = "<!-- key-table:end -->";
    // The workspace root, two levels above this crate (`crates/thermo_ui`).
    // The crates are not published, so it always exists. Fail if it is not
    // where expected: a silent pass is how moving the crate switched this
    // test off once (#310).
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let workspace = std::fs::read_to_string(root.join("Cargo.toml"))
        .expect("the workspace Cargo.toml is two levels above this crate");
    assert!(
        workspace.contains("[workspace]"),
        "{} is not the workspace root",
        root.display()
    );
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md is readable");
    // A Windows checkout may have CRLF line endings.
    let readme = readme.replace("\r\n", "\n");
    let start = readme
        .find(START)
        .expect("README has the key-table start marker")
        + START.len();
    let end = readme[start..]
        .find(END)
        .map(|i| start + i)
        .expect("README has the key-table end marker");
    let table = &readme[start..end];
    let expected = format!("| Key | Action |\n|---|---|\n{}", readme_key_rows());
    assert_eq!(
        table, expected,
        "README key table is out of date; replace it with:\n{expected}"
    );
}

/// `N` and `Alt + N` step the species of new balls through the registry
/// (#238).
#[test]
fn test_n_keys_step_the_species_of_new_balls() {
    use thermo_runtime::application::{BallCommand, DomainCommand};
    let alt = HeldModifiers {
        alt: true,
        ..HeldModifiers::default()
    };
    for (held, id, expected) in [
        (
            HeldModifiers::default(),
            "balls.next_species",
            "NextSpecies",
        ),
        (alt, "balls.previous_species", "PreviousSpecies"),
    ] {
        let (command, _) = command_for_key(super::us(KeyCode::KeyN), KeyPhase::Up, held)
            .unwrap_or_else(|| panic!("N with {held:?} runs a command"));
        assert_eq!(command.id, id);
        let domain = command.domain_command().expect("a domain command");
        let DomainCommand::Balls(ball_command) = domain else {
            panic!("{id} is a ball command, got {domain:?}");
        };
        assert_eq!(format!("{ball_command:?}"), expected);
        assert!(matches!(
            ball_command,
            BallCommand::NextSpecies | BallCommand::PreviousSpecies
        ));
    }
}
