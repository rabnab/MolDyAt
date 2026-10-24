// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Names of keys: how the help text shows a binding, and how a key-binding
//! file (`ui.toml`, #335) names one. A name the help text shows parses back
//! to the same binding, so users can copy keys from the help (`H`).

use super::command_registry::{BindKey, Modifier};
use winit::keyboard::KeyCode;

/// Keys bound by name, as physical keys: those that type no character, and
/// the numpad keys by their `Num` names for a file that wants the numpad
/// apart from the main row. Several codes may share a name (left and right
/// Shift); a name then binds all of them.
const NAMED_KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::Space, "SPACE"),
    (KeyCode::Escape, "ESC"),
    (KeyCode::Tab, "TAB"),
    (KeyCode::Enter, "ENTER"),
    (KeyCode::Delete, "DEL"),
    (KeyCode::Backspace, "BACKSPACE"),
    (KeyCode::Insert, "INS"),
    (KeyCode::Home, "HOME"),
    (KeyCode::End, "END"),
    (KeyCode::PageUp, "PAGEUP"),
    (KeyCode::PageDown, "PAGEDOWN"),
    (KeyCode::ArrowUp, "UP"),
    (KeyCode::ArrowDown, "DOWN"),
    (KeyCode::ArrowLeft, "LEFT"),
    (KeyCode::ArrowRight, "RIGHT"),
    (KeyCode::ShiftLeft, "Shift"),
    (KeyCode::ShiftRight, "Shift"),
    (KeyCode::NumpadAdd, "Num +"),
    (KeyCode::NumpadSubtract, "Num -"),
    (KeyCode::NumpadMultiply, "Num *"),
    (KeyCode::NumpadDivide, "Num /"),
    (KeyCode::NumpadEnter, "Num ENTER"),
    (KeyCode::Numpad0, "Num 0"),
    (KeyCode::Numpad1, "Num 1"),
    (KeyCode::Numpad2, "Num 2"),
    (KeyCode::Numpad3, "Num 3"),
    (KeyCode::Numpad4, "Num 4"),
    (KeyCode::Numpad5, "Num 5"),
    (KeyCode::Numpad6, "Num 6"),
    (KeyCode::Numpad7, "Num 7"),
    (KeyCode::Numpad8, "Num 8"),
    (KeyCode::Numpad9, "Num 9"),
    (KeyCode::F1, "F1"),
    (KeyCode::F2, "F2"),
    (KeyCode::F3, "F3"),
    (KeyCode::F4, "F4"),
    (KeyCode::F5, "F5"),
    (KeyCode::F6, "F6"),
    (KeyCode::F7, "F7"),
    (KeyCode::F8, "F8"),
    (KeyCode::F9, "F9"),
    (KeyCode::F10, "F10"),
    (KeyCode::F11, "F11"),
    (KeyCode::F12, "F12"),
];

/// Help-text name of a physical key, or `?` for a key without a name.
pub(super) fn key_name(key: KeyCode) -> String {
    NAMED_KEYS
        .iter()
        .find(|(code, _)| *code == key)
        .map_or_else(|| "?".to_string(), |(_, name)| name.to_string())
}

/// Help-text name of a bound key.
pub(super) fn bind_key_name(key: BindKey) -> String {
    match key {
        BindKey::Code(code) => key_name(code),
        // Letters as printed on the keys.
        BindKey::Symbol(symbol) => symbol.to_uppercase().to_string(),
    }
}

/// Parse a key as the help text writes it: an optional `Ctrl +` or `Alt +`,
/// then the name of a key that types no character (`SPACE`, `F4`, `ESC`;
/// case does not matter) or a single character (`S`, `0`, `+`, `ü`), which
/// binds that character wherever the layout puts it, typed with or without
/// Shift, the numpad included (see [`BindKey`]). Letters ignore case. The
/// numpad names (`Num +`, `Num 0`, …) bind the numpad key alone, as a
/// physical key.
///
/// Returns every key the name stands for (`Shift` is both Shift keys).
pub fn parse_key(spec: &str) -> Result<(Modifier, Vec<BindKey>), String> {
    let spec = spec.trim();
    let (modifier, name) = split_modifier(spec);
    if name.is_empty() {
        return Err(format!("`{spec}` names no key"));
    }
    let keys = parse_key_name(name).ok_or_else(|| format!("unknown key `{name}` in `{spec}`"))?;
    Ok((modifier, keys))
}

/// Split `Ctrl + S` / `ctrl+s` into the modifier and the key name.
fn split_modifier(spec: &str) -> (Modifier, &str) {
    for (prefix, modifier) in [("ctrl", Modifier::Ctrl), ("alt", Modifier::Alt)] {
        let Some(head) = spec.get(..prefix.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(prefix) {
            continue;
        }
        if let Some(rest) = spec[prefix.len()..].trim_start().strip_prefix('+') {
            return (modifier, rest.trim());
        }
    }
    (Modifier::None, spec)
}

/// A letter in lower case, as a key types it without Shift (`Ü` → `ü`), so
/// case does not matter for non-ASCII letters either; anything else as it
/// is. With Shift held the key types `Ü`, but the lookup tries the
/// unmodified `ü` first, so the binding fires either way.
fn lower_case(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

fn parse_key_name(name: &str) -> Option<Vec<BindKey>> {
    let named: Vec<BindKey> = NAMED_KEYS
        .iter()
        .filter(|(_, key_name)| key_name.eq_ignore_ascii_case(name))
        .map(|(code, _)| BindKey::Code(*code))
        .collect();
    if !named.is_empty() {
        return Some(named);
    }
    let mut chars = name.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c.is_whitespace() || c.is_control() {
        return None;
    }
    Some(vec![BindKey::Symbol(lower_case(c))])
}
