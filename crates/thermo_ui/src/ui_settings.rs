// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The UI settings file (`ui.toml`): settings of this computer's window, not
//! of a simulation. Unlike the `SimConfig` (`--config`), it never travels
//! with saves. For now it holds the key bindings (#335):
//!
//! ```toml
//! [key_bindings]
//! # command id = keys, written as the help text (`H`) shows them
//! "sim.toggle_pause" = ["SPACE", "P"]
//! "world.save" = "Ctrl + W"
//! "view.cycle_cutoff_rings" = []     # no key
//! ```
//!
//! An entry replaces all keys of that command; the ids are those of the
//! README key table. Quoting an id is optional: unquoted, TOML reads
//! `sim.toggle_pause` as nested keys, which are joined back. See `KeyMap::with_overrides` for how mistakes and
//! clashes are handled.

use crate::graphics::renderer::event_loop::KeyMap;
use std::path::{Path, PathBuf};

/// The settings file read when `--ui-config` is not given, relative to the
/// working directory (like the default save directory). A missing file
/// means the defaults.
pub const DEFAULT_UI_SETTINGS_FILE: &str = "ui.toml";

/// The contents of a UI settings file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiSettings {
    /// `[key_bindings]`: command id and its keys, sorted by id.
    pub key_bindings: Vec<(String, Vec<String>)>,
}

impl UiSettings {
    /// Read a settings file. Fails only if the text is not TOML; entries it
    /// cannot use are skipped and reported in the returned warnings.
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), String> {
        let table: toml::Table = text.parse().map_err(|error| format!("{error}"))?;
        let mut settings = Self::default();
        let mut warnings = Vec::new();
        for (name, value) in table {
            match (name.as_str(), value) {
                ("key_bindings", toml::Value::Table(bindings)) => {
                    collect_bindings("", bindings, &mut settings.key_bindings, &mut warnings);
                    // Sorted by id, whatever spelling the file used.
                    settings.key_bindings.sort_by(|a, b| a.0.cmp(&b.0));
                }
                ("key_bindings", _) => {
                    warnings.push("`key_bindings` must be a table: ignored".to_string());
                }
                (other, _) => warnings.push(format!("unknown setting `{other}`: ignored")),
            }
        }
        Ok((settings, warnings))
    }

    /// Read the settings file at `path`.
    pub fn load(path: &Path) -> Result<(Self, Vec<String>), String> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{error}"))?;
        Self::parse(&text)
    }

    /// The key bindings: the defaults with this file's entries applied.
    pub fn key_map(&self) -> (KeyMap, Vec<String>) {
        KeyMap::with_overrides(
            self.key_bindings
                .iter()
                .map(|(id, keys)| (id.as_str(), keys.as_slice())),
        )
    }
}

/// The entries of `table` with their full ids. Command ids contain dots, so
/// an unquoted id (`sim.toggle_pause = "K"`) is a dotted TOML key, which
/// TOML reads as nested tables (`sim` → `toggle_pause`); joining the keys
/// back with dots makes both spellings work.
fn collect_bindings(
    prefix: &str,
    table: toml::Table,
    bindings: &mut Vec<(String, Vec<String>)>,
    warnings: &mut Vec<String>,
) {
    for (key, value) in table {
        let id = if prefix.is_empty() {
            key
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(nested) => collect_bindings(&id, nested, bindings, warnings),
            value => match key_list(value) {
                Some(keys) => bindings.push((id, keys)),
                None => warnings.push(format!(
                    "key_bindings.`{id}`: expected a key or a list of keys: ignored"
                )),
            },
        }
    }
}

/// `"S"` or `["S", "Ctrl + S"]`; `None` for anything else.
fn key_list(value: toml::Value) -> Option<Vec<String>> {
    match value {
        toml::Value::String(key) => Some(vec![key]),
        toml::Value::Array(keys) => keys
            .into_iter()
            .map(|key| match key {
                toml::Value::String(key) => Some(key),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// The key map for the window: from `explicit` (`--ui-config`), or from
/// [`DEFAULT_UI_SETTINGS_FILE`] if it exists, otherwise the defaults.
/// Warnings about single entries are logged; a file that cannot be read or
/// is not TOML is an error, so a broken file is never silently ignored.
pub fn load_key_map(explicit: Option<&Path>) -> Result<KeyMap, String> {
    let path = match explicit {
        Some(path) => path.to_path_buf(),
        None => {
            let default = PathBuf::from(DEFAULT_UI_SETTINGS_FILE);
            if !default.exists() {
                return Ok(KeyMap::default());
            }
            default
        }
    };
    let (settings, mut warnings) =
        UiSettings::load(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let (key_map, key_warnings) = settings.key_map();
    warnings.extend(key_warnings);
    for warning in &warnings {
        log::warn!("{}: {warning}", path.display());
    }
    log::info!(
        "UI settings from {}: {} key binding entries",
        path.display(),
        settings.key_bindings.len()
    );
    Ok(key_map)
}

#[cfg(test)]
mod tests;
