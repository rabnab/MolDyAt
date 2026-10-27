// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod command_registry;
mod domain_command;
mod energy_window_manager;
mod key_actions;
mod key_map;
mod key_state;
mod line_segment_tool;
mod mouse_input;
mod object_hover;
mod source_sphere_tool;
mod thermostat_tool;
mod tool_switch;
mod unified_render_state;
mod valve_tool;

use super::command_registry::KeyInput;
use super::input_handler::KeyCode;

/// The key event a US keyboard sends for `code` with no modifier held: its
/// physical key and the character it types (letters, digits, `-`, `=`, the
/// numpad). Bindings match characters, so tests press keys through this.
pub(super) fn us(code: KeyCode) -> KeyInput {
    KeyInput::new(code, us_char(code))
}

/// The character `code` types on a US keyboard without modifiers.
pub(super) fn us_char(code: KeyCode) -> Option<char> {
    let name = format!("{code:?}");
    if let Some(letter) = name.strip_prefix("Key") {
        return letter.chars().next().map(|c| c.to_ascii_lowercase());
    }
    if let Some(digit) = name
        .strip_prefix("Digit")
        .or_else(|| name.strip_prefix("Numpad"))
        .filter(|d| d.len() == 1)
    {
        return digit.chars().next();
    }
    match code {
        KeyCode::Minus | KeyCode::NumpadSubtract => Some('-'),
        KeyCode::Equal => Some('='),
        KeyCode::NumpadAdd => Some('+'),
        KeyCode::Comma => Some(','),
        KeyCode::Period => Some('.'),
        _ => None,
    }
}
