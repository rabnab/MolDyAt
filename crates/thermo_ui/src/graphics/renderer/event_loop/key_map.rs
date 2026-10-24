// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The key bindings in effect: the defaults of [`COMMANDS`], with the
//! overrides of a key-binding file applied (#335). Key dispatch and the help
//! text read this map, so they always show what the keys do.

use super::command_registry::{
    BindKey, COMMANDS, CommandCategory, CommandDescriptor, KeyBinding, KeyInput, Modifier, Trigger,
    command_by_id,
};
use super::key_names::parse_key;
use Modifier::{Alt, Ctrl};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

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

/// The bindings of every command, in [`COMMANDS`] order.
#[derive(Debug, Clone)]
pub struct KeyMap {
    bindings: Vec<(&'static CommandDescriptor, KeyBinding)>,
}

impl Default for KeyMap {
    /// The default bindings of [`COMMANDS`].
    fn default() -> Self {
        let bindings = COMMANDS
            .iter()
            .flat_map(|command| command.default_binding.iter().map(move |b| (command, *b)))
            .collect();
        Self { bindings }
    }
}

impl KeyMap {
    /// The defaults with `overrides` applied: `(command id, keys)`, in the
    /// order given (a `ui.toml` gives them sorted by id). The keys replace
    /// all bindings of that command (an empty list unbinds it); each key is
    /// written as [`parse_key`] reads it and keeps the trigger of the
    /// command's default bindings.
    ///
    /// Nothing here fails: an unknown id, an unparsable key or a key taken
    /// twice is skipped and reported in the returned warnings, so a typo
    /// never leaves the user without keys. When two entries bind one key,
    /// the first in that order keeps it. A key an override binds is taken
    /// away from the command that had it by default. A key listed twice in
    /// one entry counts once.
    pub fn with_overrides<'a>(
        overrides: impl IntoIterator<Item = (&'a str, &'a [String])>,
    ) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut replaced: Vec<(&'static CommandDescriptor, Vec<KeyBinding>)> = Vec::new();
        for (id, keys) in overrides {
            let Some(command) = command_by_id(id) else {
                warnings.push(format!("unknown command `{id}`: ignored"));
                continue;
            };
            // A file can name a command twice: quoted, and as a dotted table
            // key (see `ui_settings`).
            if replaced.iter().any(|(c, _)| c.id == command.id) {
                warnings.push(format!("`{id}` is given twice: the first entry is used"));
                continue;
            }
            let trigger = default_trigger(command);
            let mut bindings = Vec::new();
            for spec in keys {
                match parse_key(spec) {
                    Ok((modifier, keys)) => {
                        bindings.extend(keys.into_iter().map(|key| KeyBinding {
                            key,
                            modifier,
                            trigger,
                            hidden: false,
                        }))
                    }
                    Err(error) => warnings.push(format!("`{id}`: {error}: ignored")),
                }
            }
            replaced.push((command, bindings));
        }

        // Overrides claim their keys first, then the defaults of the other
        // commands fill in what is left.
        let mut taken: HashMap<(BindKey, Modifier), &'static str> = HashMap::new();
        let mut kept: Vec<(&'static CommandDescriptor, KeyBinding)> = Vec::new();
        for (command, bindings) in &replaced {
            for binding in bindings {
                match taken.get(&(binding.key, binding.modifier)) {
                    None => {
                        taken.insert((binding.key, binding.modifier), command.id);
                        kept.push((command, *binding));
                    }
                    // Listed twice in this entry (`["K", "k"]`).
                    Some(&owner) if owner == command.id => {}
                    Some(_) => {
                        warnings.push(format!(
                            "`{}`: {} is already bound by another entry: ignored",
                            command.id,
                            binding.display()
                        ));
                    }
                }
            }
        }
        for command in COMMANDS {
            if replaced.iter().any(|(c, _)| c.id == command.id) {
                continue;
            }
            for binding in command.default_binding {
                match taken.entry((binding.key, binding.modifier)) {
                    Entry::Vacant(slot) => {
                        slot.insert(command.id);
                        kept.push((command, *binding));
                    }
                    Entry::Occupied(_) => warnings.push(format!(
                        "`{}` loses its default key {}, which the file binds to another command",
                        command.id,
                        binding.display()
                    )),
                }
            }
        }

        // Back into COMMANDS order, so lookups and the help text keep it.
        let mut bindings = Vec::with_capacity(kept.len());
        for command in COMMANDS {
            bindings.extend(kept.iter().filter(|(c, _)| c.id == command.id).copied());
        }
        (Self { bindings }, warnings)
    }

    /// The bindings of `command`, in order.
    pub fn bindings_of(
        &self,
        command: &'static CommandDescriptor,
    ) -> impl Iterator<Item = KeyBinding> + '_ {
        self.bindings
            .iter()
            .filter(move |(c, _)| c.id == command.id)
            .map(|(_, binding)| *binding)
    }

    /// The command a key event runs, with the binding that matched.
    ///
    /// Of the bindings for `key` that fire in `phase`, a held modifier's
    /// binding wins (Ctrl before Alt); otherwise the plain binding fires.
    /// Auto-repeat is not filtered here: the caller drops a repeated
    /// [`Trigger::PressOnce`]. `InputHandler` decides chords with
    /// [`KeyMap::chord_for_key`] on the first press and looks up everything
    /// else here with no modifiers held.
    pub fn command_for_key(
        &self,
        key: KeyInput,
        phase: KeyPhase,
        held: HeldModifiers,
    ) -> Option<(&'static CommandDescriptor, KeyBinding)> {
        let fires_in_phase = |trigger: Trigger| match phase {
            KeyPhase::Down => trigger != Trigger::Release,
            KeyPhase::Up => trigger == Trigger::Release,
        };
        [Ctrl, Alt, Modifier::None]
            .into_iter()
            .filter(|&modifier| held.holds(modifier))
            .find_map(|modifier| self.find_binding(key, modifier, |b| fires_in_phase(b.trigger)))
    }

    /// The chord a key press starts: the binding of `key` whose modifier is
    /// held (Ctrl before Alt), whatever its trigger. A chord owns its key
    /// until the release (see `InputHandler`).
    pub fn chord_for_key(
        &self,
        key: KeyInput,
        held: HeldModifiers,
    ) -> Option<(&'static CommandDescriptor, KeyBinding)> {
        [Ctrl, Alt]
            .into_iter()
            .filter(|&modifier| held.holds(modifier))
            .find_map(|modifier| self.find_binding(key, modifier, |_| true))
    }

    /// The binding of `key` with `modifier` that passes `accept`. The
    /// physical key comes first, so letters, digits and named keys keep
    /// their command on every layout; a key without one runs the binding of
    /// the character it types, so a key that types `+` doubles the time
    /// step whatever its place (#327). The unmodified character comes before
    /// the one typed with the modifiers, which serves layouts where the
    /// symbol needs Shift.
    ///
    /// The first of these with any binding for `modifier` owns the press,
    /// even if `accept` turns that binding down: a key bound to a press
    /// command must not run the command of its character on release (Num +
    /// bound to pause would also double dt, #355 review).
    fn find_binding(
        &self,
        key: KeyInput,
        modifier: Modifier,
        accept: impl Fn(&KeyBinding) -> bool,
    ) -> Option<(&'static CommandDescriptor, KeyBinding)> {
        let wanted = [
            Some(BindKey::Code(key.code)),
            key.symbol.map(BindKey::Symbol),
            key.typed.map(BindKey::Symbol),
        ];
        let owner = wanted.into_iter().flatten().find(|&wanted| {
            self.bindings
                .iter()
                .any(|(_, b)| b.key == wanted && b.modifier == modifier)
        })?;
        self.bindings
            .iter()
            .find(|(_, b)| b.key == owner && b.modifier == modifier && accept(b))
            .copied()
    }

    /// The keys of `command` as the help text shows them, e.g.
    /// `DEL / BACKSPACE`. Keys with the same name (left and right Shift) are
    /// listed once.
    pub fn binding_display(&self, command: &'static CommandDescriptor) -> Vec<String> {
        display_names(self.bindings_of(command))
    }

    /// The help text (key `H`): every command with a key, by category.
    pub fn help_text(&self) -> String {
        let mut text = String::from("HELP: Available keyboard shortcuts:");
        for category in CommandCategory::ALL {
            let lines: Vec<String> = COMMANDS
                .iter()
                .filter(|command| command.category == category)
                .filter_map(|command| {
                    let keys = self.binding_display(command);
                    (!keys.is_empty())
                        .then(|| format!("\n  {:<22} {}", keys.join(" / "), command.label_text()))
                })
                .collect();
            if lines.is_empty() {
                continue;
            }
            text.push_str(&format!("\n{}:", category.title()));
            for line in lines {
                text.push_str(&line);
            }
        }
        text
    }
}

/// The trigger of a command's default bindings (all of a command's default
/// bindings share one). A command without default keys fires once per
/// press (`PressOnce`): holding a key the user bound to it must not repeat
/// it.
fn default_trigger(command: &CommandDescriptor) -> Trigger {
    command
        .default_binding
        .first()
        .map_or(Trigger::PressOnce, |binding| binding.trigger)
}

/// Binding names in order, each once; hidden aliases are left out.
pub(super) fn display_names(bindings: impl IntoIterator<Item = KeyBinding>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for binding in bindings.into_iter().filter(|b| !b.hidden) {
        let name = binding.display();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}
