// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{UiSettings, load_key_map};
use crate::graphics::renderer::event_loop::command_registry::KeyInput;
use crate::graphics::renderer::event_loop::key_map::{HeldModifiers, KeyPhase};
use crate::graphics::renderer::event_loop::{COMMANDS, KeyCode, KeyMap};

fn parse(text: &str) -> (UiSettings, Vec<String>) {
    UiSettings::parse(text).expect("valid TOML")
}

#[test]
fn test_key_bindings_take_a_key_or_a_list_of_keys() {
    let (settings, warnings) = parse(
        r#"
        [key_bindings]
        "sim.toggle_pause" = ["SPACE", "P"]
        "world.save" = "Ctrl + W"
        "help.show" = []
        "#,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        settings.key_bindings,
        [
            ("help.show".to_string(), vec![]),
            (
                "sim.toggle_pause".to_string(),
                vec!["SPACE".to_string(), "P".to_string()]
            ),
            ("world.save".to_string(), vec!["Ctrl + W".to_string()]),
        ]
    );
}

#[test]
fn test_an_empty_file_gives_the_default_key_map() {
    let (settings, warnings) = parse("");
    assert!(warnings.is_empty());
    let (key_map, warnings) = settings.key_map();
    assert!(warnings.is_empty());
    assert_eq!(key_map.help_text(), KeyMap::default().help_text());
}

#[test]
fn test_unknown_settings_and_bad_values_are_warned_about_and_skipped() {
    let (settings, warnings) = parse(
        r#"
        theme = "dark"
        [key_bindings]
        "sim.toggle_pause" = 3
        "sim.cycle_gravity" = ["G", 4]
        "help.show" = "F1"
        "#,
    );
    assert_eq!(
        settings.key_bindings,
        [("help.show".to_string(), vec!["F1".to_string()])]
    );
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    assert!(warnings.iter().any(|w| w.contains("`theme`")));
    assert!(warnings.iter().any(|w| w.contains("sim.toggle_pause")));
    assert!(warnings.iter().any(|w| w.contains("sim.cycle_gravity")));
}

#[test]
fn test_key_bindings_that_are_not_a_table_are_skipped() {
    let (settings, warnings) = parse(r#"key_bindings = "P""#);
    assert!(settings.key_bindings.is_empty());
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn test_text_that_is_not_toml_is_an_error() {
    assert!(UiSettings::parse("[key_bindings\n").is_err());
}

#[test]
fn test_a_missing_explicit_file_is_an_error() {
    let missing = std::env::temp_dir().join("moldyat-no-such-ui-settings.toml");
    let error = load_key_map(Some(&missing)).expect_err("missing file");
    assert!(
        error.contains("moldyat-no-such-ui-settings.toml"),
        "{error}"
    );
}

#[test]
fn test_an_explicit_file_sets_the_key_map() {
    let path =
        std::env::temp_dir().join(format!("moldyat-ui-settings-{}.toml", std::process::id()));
    std::fs::write(&path, "[key_bindings]\n\"help.show\" = \"F9\"\n").expect("writable");
    let key_map = load_key_map(Some(&path)).expect("valid file");
    std::fs::remove_file(&path).ok();
    let help = COMMANDS
        .iter()
        .find(|c| c.id == "help.show")
        .expect("help.show");
    assert_eq!(key_map.binding_display(help), ["F9"]);
}

/// The example in the README ("Changing the keys") loads without warnings.
#[test]
fn test_readme_example_is_a_valid_settings_file() {
    let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md is readable");
    let readme = readme.replace("\r\n", "\n");
    let section = readme
        .split_once("### Changing the keys")
        .expect("README has the section")
        .1;
    let example = section
        .split_once("```toml\n")
        .and_then(|(_, rest)| rest.split_once("```"))
        .expect("the section has a toml block")
        .0;
    let (settings, warnings) = parse(example);
    assert!(warnings.is_empty(), "{warnings:?}");
    let (key_map, warnings) = settings.key_map();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(settings.key_bindings.len(), 4);
    // After copying the example, the US main-row `=` key still doubles dt.
    let (command, _) = key_map
        .command_for_key(
            KeyInput::new(KeyCode::Equal, Some('=')),
            KeyPhase::Up,
            HeldModifiers::default(),
        )
        .expect("the = key runs a command");
    assert_eq!(command.id, "sim.double_time_step");
}

/// Unquoted ids are dotted TOML keys; both spellings give the same entry.
#[test]
fn test_unquoted_ids_work_like_quoted_ones() {
    let (settings, warnings) = parse(
        r#"
        [key_bindings]
        sim.toggle_pause = "K"
        "help.show" = "F9"
        "#,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        settings.key_bindings,
        [
            ("help.show".to_string(), vec!["F9".to_string()]),
            ("sim.toggle_pause".to_string(), vec!["K".to_string()]),
        ]
    );
}

/// The same command quoted and unquoted: the first by id order is used.
#[test]
fn test_a_command_given_twice_is_warned_about() {
    let (settings, warnings) = parse(
        r#"
        [key_bindings]
        "sim.toggle_pause" = "K"
        sim.toggle_pause = "L"
        "#,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(settings.key_bindings.len(), 2);
    let (_, warnings) = settings.key_map();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("given twice"), "{warnings:?}");
}
