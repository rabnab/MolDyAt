// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Key-binding overrides (#335) and key names.

use super::super::command_registry::{
    BindKey, COMMANDS, CommandDescriptor, KeyInput, Modifier, Trigger, command_by_id,
};
use super::super::input_handler::{InputAction, InputHandler, KeyCode};
use super::super::key_map::{HeldModifiers, KeyMap, KeyPhase};
use super::super::key_names::parse_key;

fn command(id: &str) -> &'static CommandDescriptor {
    command_by_id(id).unwrap_or_else(|| panic!("no command {id}"))
}

/// The key map with these `(id, keys)` overrides, and its warnings.
fn with(overrides: &[(&str, &[&str])]) -> (KeyMap, Vec<String>) {
    let owned: Vec<(&str, Vec<String>)> = overrides
        .iter()
        .map(|(id, keys)| (*id, keys.iter().map(|k| k.to_string()).collect()))
        .collect();
    KeyMap::with_overrides(owned.iter().map(|(id, keys)| (*id, keys.as_slice())))
}

fn id(action: Option<InputAction>) -> Option<&'static str> {
    action.map(|action| action.command().expect("a registry command").id)
}

#[test]
fn test_parse_key_reads_what_the_help_text_shows() {
    // Every default binding's name parses back to that binding's key.
    for command in COMMANDS {
        for binding in command.default_binding {
            let name = binding.display();
            let (modifier, keys) =
                parse_key(&name).unwrap_or_else(|e| panic!("{}: {e}", command.id));
            assert_eq!(modifier, binding.modifier, "{name}");
            assert!(keys.contains(&binding.key), "{name} -> {keys:?}");
        }
    }
}

#[test]
fn test_parse_key_spellings() {
    let code = |c| vec![BindKey::Code(c)];
    let symbol = |c| vec![BindKey::Symbol(c)];
    // Letters and digits are the characters they type; letters ignore case.
    assert_eq!(parse_key("s"), Ok((Modifier::None, symbol('s'))));
    assert_eq!(parse_key("S"), Ok((Modifier::None, symbol('s'))));
    assert_eq!(parse_key("ctrl+s"), Ok((Modifier::Ctrl, symbol('s'))));
    assert_eq!(parse_key(" Alt + 7 "), Ok((Modifier::Alt, symbol('7'))));
    assert_eq!(
        parse_key("space"),
        Ok((Modifier::None, code(KeyCode::Space)))
    );
    assert_eq!(
        parse_key("num +"),
        Ok((Modifier::None, code(KeyCode::NumpadAdd)))
    );
    assert_eq!(parse_key("F12"), Ok((Modifier::None, code(KeyCode::F12))));
    assert_eq!(
        parse_key("Alt + +"),
        Ok((Modifier::Alt, vec![BindKey::Symbol('+')]))
    );
    assert_eq!(
        parse_key("#"),
        Ok((Modifier::None, vec![BindKey::Symbol('#')]))
    );
    assert_eq!(
        parse_key("Shift"),
        Ok((
            Modifier::None,
            vec![
                BindKey::Code(KeyCode::ShiftLeft),
                BindKey::Code(KeyCode::ShiftRight)
            ]
        ))
    );
    for bad in ["", "Ctrl +", "Hyper + S", "SPACEBAR", "ab"] {
        assert!(parse_key(bad).is_err(), "{bad:?} should not parse");
    }
}

#[test]
fn test_default_bindings_of_a_command_share_one_trigger() {
    // An override takes the trigger of the command's defaults.
    for command in COMMANDS {
        // A command without default keys is valid: a user can bind it.
        let Some(first) = command.default_binding.first() else {
            continue;
        };
        let first = first.trigger;
        for binding in command.default_binding {
            assert_eq!(binding.trigger, first, "{}", command.id);
        }
    }
}

#[test]
fn test_override_replaces_the_keys_of_a_command() {
    let (key_map, warnings) = with(&[("sim.toggle_pause", &["K", "Ctrl + SPACE"])]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let pause = command("sim.toggle_pause");
    assert_eq!(key_map.binding_display(pause), ["K", "Ctrl + SPACE"]);
    for binding in key_map.bindings_of(pause) {
        assert_eq!(binding.trigger, Trigger::Press);
    }
    // SPACE no longer pauses; K does.
    let down = KeyPhase::Down;
    let none = HeldModifiers::default();
    assert!(
        key_map
            .command_for_key(super::us(KeyCode::Space), down, none)
            .is_none()
    );
    let (found, _) = key_map
        .command_for_key(super::us(KeyCode::KeyK), down, none)
        .expect("K pauses");
    assert_eq!(found.id, "sim.toggle_pause");
}

#[test]
fn test_an_empty_list_unbinds_a_command() {
    let (key_map, warnings) = with(&[("help.show", &[])]);
    assert!(warnings.is_empty());
    assert!(key_map.binding_display(command("help.show")).is_empty());
    assert!(!key_map.help_text().contains("Print this list of keys"));
}

#[test]
fn test_an_overridden_key_moves_away_from_its_default_command() {
    // G cycles gravity by default; the file gives it to the pause.
    let (key_map, warnings) = with(&[("sim.toggle_pause", &["G"])]);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("sim.cycle_gravity"), "{warnings:?}");
    assert!(
        key_map
            .binding_display(command("sim.cycle_gravity"))
            .is_empty()
    );
    let mut handler = InputHandler::with_key_map(key_map);
    assert_eq!(
        id(handler.handle_key_down(super::us(KeyCode::KeyG), false)),
        Some("sim.toggle_pause")
    );
}

#[test]
fn test_two_entries_binding_one_key_keep_the_first() {
    let (key_map, warnings) = with(&[("sim.toggle_pause", &["X"]), ("help.show", &["X"])]);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("help.show"), "{warnings:?}");
    assert_eq!(key_map.binding_display(command("sim.toggle_pause")), ["X"]);
    assert!(key_map.binding_display(command("help.show")).is_empty());
}

#[test]
fn test_mistakes_are_warned_about_and_the_rest_is_used() {
    let (key_map, warnings) = with(&[
        ("sim.no_such_command", &["X"]),
        ("help.show", &["F9", "NOPE"]),
    ]);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("sim.no_such_command")));
    assert!(warnings.iter().any(|w| w.contains("NOPE")));
    assert_eq!(key_map.binding_display(command("help.show")), ["F9"]);
    // Commands without an entry keep their defaults.
    assert_eq!(
        key_map.binding_display(command("sim.cycle_gravity")),
        command("sim.cycle_gravity").binding_display()
    );
}

#[test]
fn test_no_key_is_bound_twice_after_overrides() {
    let (key_map, _) = with(&[
        ("sim.toggle_pause", &["G", "S"]),
        ("sim.cycle_gravity", &["SPACE"]),
        ("world.save", &["Ctrl + O"]),
    ]);
    let mut seen = std::collections::HashSet::new();
    for command in COMMANDS {
        for binding in key_map.bindings_of(command) {
            assert!(
                seen.insert((binding.key, binding.modifier)),
                "{} bound twice",
                binding.display()
            );
        }
    }
}

#[test]
fn test_overridden_chord_and_symbol_keys_dispatch() {
    let (key_map, warnings) = with(&[
        ("world.save", &["Alt + W"]),
        ("sim.double_time_step", &["*"]),
    ]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let mut handler = InputHandler::with_key_map(key_map);
    handler.handle_key_down(super::us(KeyCode::AltLeft), false);
    assert_eq!(
        id(handler.handle_key_down(super::us(KeyCode::KeyW), false)),
        Some("world.save")
    );
    handler.handle_key_up(super::us(KeyCode::KeyW));
    handler.handle_key_up(super::us(KeyCode::AltLeft));
    // `*` doubles the time step wherever it is typed: on the US keyboard it
    // is Shift + 8, which types `8` without Shift.
    let star = KeyInput::new(KeyCode::Digit8, Some('8')).with_typed(Some('*'));
    handler.handle_key_down(super::us(KeyCode::ShiftLeft), false);
    assert!(handler.handle_key_down(star, false).is_none());
    assert_eq!(
        id(handler.handle_key_up(star)),
        Some("sim.double_time_step")
    );
    // `+` no longer does.
    let plus = KeyInput::new(KeyCode::BracketRight, Some('+'));
    handler.handle_key_down(plus, false);
    assert!(handler.handle_key_up(plus).is_none());
}

#[test]
fn test_help_text_shows_the_overridden_keys() {
    let (key_map, _) = with(&[("sim.toggle_pause", &["K"])]);
    let line = key_map
        .help_text()
        .lines()
        .find(|line| line.contains("Pause or resume"))
        .expect("pause line")
        .to_string();
    assert!(line.trim_start().starts_with("K "), "{line}");
}

/// An override of the physical numpad key takes it, even though the key
/// types `+`, which double time step binds as a symbol (#355 review).
#[test]
fn test_override_of_a_numpad_key_wins_over_the_symbol_it_types() {
    let (key_map, warnings) = with(&[("sim.toggle_pause", &["Num +"])]);
    // Double dt binds `+` only; the numpad key was free.
    assert!(warnings.is_empty(), "{warnings:?}");
    let up_or_down = |key: KeyInput, phase| {
        key_map
            .command_for_key(key, phase, HeldModifiers::default())
            .map(|(command, _)| command.id)
    };
    let num_plus = KeyInput::new(KeyCode::NumpadAdd, Some('+'));
    assert_eq!(
        up_or_down(num_plus, KeyPhase::Down),
        Some("sim.toggle_pause")
    );
    assert_eq!(up_or_down(num_plus, KeyPhase::Up), None);
    // The main-row `+` still doubles dt, as the help says.
    let de_plus = KeyInput::new(KeyCode::BracketRight, Some('+'));
    assert_eq!(
        up_or_down(de_plus, KeyPhase::Up),
        Some("sim.double_time_step")
    );
    assert_eq!(
        key_map.binding_display(command("sim.double_time_step")),
        ["+"]
    );
}

/// A character override takes that character on every key that types it,
/// the numpad included, away from its default command, as the help shows.
#[test]
fn test_character_override_takes_the_character_on_every_key() {
    let (key_map, warnings) = with(&[("balls.clear", &["-"])]);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("sim.halve_time_step"), "{warnings:?}");
    let down = |key: KeyInput| {
        key_map
            .command_for_key(key, KeyPhase::Down, HeldModifiers::default())
            .map(|(command, _)| command.id)
    };
    for code in [KeyCode::Minus, KeyCode::NumpadSubtract] {
        assert_eq!(
            down(KeyInput::new(code, Some('-'))),
            Some("balls.clear"),
            "{code:?}"
        );
    }
    assert!(
        key_map
            .binding_display(command("sim.halve_time_step"))
            .is_empty()
    );
    assert_eq!(key_map.binding_display(command("balls.clear")), ["-"]);
}

/// Case does not matter for non-ASCII letters either: `Ü` binds the key
/// that types `ü` (#355 review).
#[test]
fn test_non_ascii_letters_ignore_case() {
    assert_eq!(parse_key("Ü"), parse_key("ü"));
    assert_eq!(
        parse_key("Ü"),
        Ok((Modifier::None, vec![BindKey::Symbol('ü')]))
    );
    let (key_map, warnings) = with(&[("help.show", &["Ü"])]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let key = KeyInput::new(KeyCode::BracketLeft, Some('ü'));
    let (found, _) = key_map
        .command_for_key(key, KeyPhase::Up, HeldModifiers::default())
        .expect("ü runs a command");
    assert_eq!(found.id, "help.show");
}

/// A key listed twice in one entry counts once, without a warning.
#[test]
fn test_a_key_listed_twice_in_one_entry_is_not_a_clash() {
    let (key_map, warnings) = with(&[("sim.toggle_pause", &["K", "k"])]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(key_map.binding_display(command("sim.toggle_pause")), ["K"]);
}
