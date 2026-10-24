// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::command_registry::{CommandDescriptor, KeyBinding, KeyInput, Trigger, UiAction};
use super::key_map::{HeldModifiers, KeyMap, KeyPhase};
use std::collections::HashMap;
use thermo_physics::objects2d::Vec2D;
use thermo_runtime::application::operator_command::OperatorCommand;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, PhysicalKey};

pub type KeyCode = winit::keyboard::KeyCode;

/// Operator tool selected from the sidebar panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorTool {
    /// No tool selected — default mode.
    None,
    /// Manual ball placement tool (standard ball icon, sidebar index 0).
    Ball,
    /// Line segment barrier tool (wall icon, sidebar index 1).
    LineSegment,
    /// Valve segment tool (gate icon, sidebar index 2).
    ValveSegment,
    /// Thermostat tool (temperature icon, sidebar index 3).
    Thermostat,
    /// Source sphere tool (emitter icon, sidebar index 4).
    SourceSphere,
}

/// Actions that can result from user input.
///
/// Keyboard commands arrive as [`InputAction::Command`], looked up in the
/// command registry; the other variants come from the mouse and the operator
/// tools.
#[derive(Debug)]
pub enum InputAction {
    /// A registry command (see `command_registry`).
    Command(&'static CommandDescriptor),

    // Ball manipulation
    AddBall {
        position: Vec2D,
    },
    /// Cycle the species of new balls (right click).
    NextSpecies,

    // View control
    ZoomIn {
        center: Vec2D,
    },
    ZoomOut {
        center: Vec2D,
    },
    PanStart {
        pos_px: [f32; 2],
    },
    PanUpdate {
        delta: Vec2D,
    },
    PanEnd,

    // Cursor
    UpdateCursor {
        position: Vec2D,
    },

    // Operator panel
    SelectOperatorTool(OperatorTool),
    /// A self-contained operator command routed to the physics thread.
    OperatorAction(Box<dyn OperatorCommand>),
}

impl InputAction {
    /// The registry command, if this is one.
    pub fn command(&self) -> Option<&'static CommandDescriptor> {
        match self {
            Self::Command(command) => Some(command),
            _ => None,
        }
    }

    /// The renderer action of a UI registry command.
    pub fn ui_action(&self) -> Option<UiAction> {
        self.command().and_then(CommandDescriptor::ui_action)
    }
}

/// Handles keyboard and mouse input, tracking state and producing actions
pub struct InputHandler {
    // Every key held down, with what its first press decided.
    held_keys: HashMap<KeyCode, HeldKey>,
    cursor_pos: Vec2D,
    cursor_pos_px: [f32; 2],

    // Pan state
    is_panning: bool,
    pan_start_pos_px: [f32; 2],
    pan_start_offset: Vec2D,

    // The bindings in effect: the defaults, or those of a key-binding file.
    key_map: KeyMap,
}

/// What the first press of a held key decided; its release and auto-repeats
/// use this, not what the layout reports later.
#[derive(Debug, Clone, Copy, Default)]
struct HeldKey {
    /// The key event of the first press (`None` if only its state is known,
    /// see [`InputHandler::register_key_pressed`]). A release runs the
    /// command of these characters, even if the layout changed meanwhile.
    input: Option<KeyInput>,
    /// The chord the press started (Ctrl+S, Alt+=). The release belongs to
    /// the chord even if the modifier was let go first, so it never falls
    /// through to the plain key's command (S = stepwise, = = double dt).
    chord: Option<(&'static CommandDescriptor, KeyBinding)>,
}

/// The key of a winit key event for the command registry, or `None` for a
/// key without a known physical code.
///
/// `symbol` is the character the key types in the current layout with no
/// modifiers held, so Alt / Option, Shift or a dead-key setting do not
/// change it (#327); `typed` is the one it types with the modifiers held,
/// for symbols that need Shift on some layouts (Swiss `+` is Shift + 1).
/// Platforms without the unmodified character (web, iOS, Android) use the
/// typed one for both, so there a modifier can change `symbol` too.
pub fn key_input(event: &KeyEvent) -> Option<KeyInput> {
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    let typed = key_symbol(&event.logical_key);
    Some(KeyInput::new(code, key_symbol(&unmodified_key(event))).with_typed(typed))
}

/// Defines `$item` for the platforms where winit reports the unmodified
/// key (`KeyEventExtModifierSupplement`), `$fallback` for the others. The
/// predicate mirrors winit's own `cfg` for that trait and is written once.
macro_rules! by_modifier_supplement {
    (@split ($($predicate:tt)*) $item:item $fallback:item) => {
        #[cfg($($predicate)*)]
        $item
        #[cfg(not($($predicate)*))]
        $fallback
    };
    ($item:item else $fallback:item) => {
        by_modifier_supplement!(@split (any(
            target_os = "windows",
            target_os = "macos",
            all(
                unix,
                not(target_vendor = "apple"),
                not(target_os = "android"),
                not(target_os = "emscripten"),
                not(target_os = "redox")
            )
        )) $item $fallback);
    };
}

by_modifier_supplement! {
    fn unmodified_key(event: &KeyEvent) -> Key {
        use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
        event.key_without_modifiers()
    }
    else
    fn unmodified_key(event: &KeyEvent) -> Key {
        event.logical_key.clone()
    }
}

/// The character a key types, if it types exactly one.
pub(crate) fn key_symbol(key: &Key) -> Option<char> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(symbol), None) => Some(symbol),
        _ => None,
    }
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            held_keys: HashMap::new(),
            cursor_pos: Vec2D::zero(),
            cursor_pos_px: [0.0, 0.0],
            is_panning: false,
            pan_start_pos_px: [0.0, 0.0],
            pan_start_offset: Vec2D::zero(),
            key_map: KeyMap::default(),
        }
    }

    /// A handler with these bindings instead of the defaults.
    pub fn with_key_map(key_map: KeyMap) -> Self {
        Self {
            key_map,
            ..Self::new()
        }
    }

    /// The bindings in effect.
    pub fn key_map(&self) -> &KeyMap {
        &self.key_map
    }

    // Key state tracking
    pub fn register_key_pressed(&mut self, key: KeyCode) {
        self.held_keys.entry(key).or_default();
    }

    pub fn register_key_released(&mut self, key: KeyCode) {
        self.held_keys.remove(&key);
    }

    /// Forget every held key. Called when the window loses focus: keys
    /// released while another window has focus never send a release here,
    /// and a Ctrl that stayed "held" would turn plain S / O into save / load.
    pub fn release_all_keys(&mut self) {
        self.held_keys.clear();
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.held_keys.contains_key(&key)
    }

    pub fn is_alt_modifier_pressed(&self) -> bool {
        self.is_key_pressed(KeyCode::AltLeft) || self.is_key_pressed(KeyCode::AltRight)
    }

    pub fn is_ctrl_modifier_pressed(&self) -> bool {
        self.is_key_pressed(KeyCode::ControlLeft) || self.is_key_pressed(KeyCode::ControlRight)
    }

    // Cursor state
    pub fn cursor_pos(&self) -> Vec2D {
        self.cursor_pos
    }

    pub fn cursor_pos_px(&self) -> [f32; 2] {
        self.cursor_pos_px
    }

    pub fn set_cursor_pos(&mut self, world_pos: Vec2D, screen_pos: [f32; 2]) {
        self.cursor_pos = world_pos;
        self.cursor_pos_px = screen_pos;
    }

    // Pan state
    pub fn is_panning(&self) -> bool {
        self.is_panning
    }

    pub fn start_pan(&mut self, pos_px: [f32; 2], current_offset: Vec2D) {
        self.is_panning = true;
        self.pan_start_pos_px = pos_px;
        self.pan_start_offset = current_offset;
    }

    pub fn end_pan(&mut self) {
        self.is_panning = false;
    }

    pub fn pan_start_pos_px(&self) -> [f32; 2] {
        self.pan_start_pos_px
    }

    pub fn pan_start_offset(&self) -> Vec2D {
        self.pan_start_offset
    }

    fn held_modifiers(&self) -> HeldModifiers {
        HeldModifiers {
            ctrl: self.is_ctrl_modifier_pressed(),
            alt: self.is_alt_modifier_pressed(),
        }
    }

    /// Process a key press (or auto-repeat) and return the command it runs.
    pub fn handle_key_down(
        &mut self,
        input: impl Into<KeyInput>,
        is_repeat: bool,
    ) -> Option<InputAction> {
        let input = input.into();
        let key = input.code;

        // A chord is decided on the first press, while its modifier is
        // certainly held, and owns this key until the release, whatever its
        // trigger. An auto-repeat continues the press it repeats: a modifier
        // pressed while a plain key is held does not turn it into a chord,
        // and the characters of the first press stay (a repeat after a
        // layout switch reports others).
        let held = self.held_keys.get(&key).copied().unwrap_or_default();
        let (input, chord) = if is_repeat {
            (held.input.unwrap_or(input), held.chord)
        } else {
            (
                input,
                self.key_map.chord_for_key(input, self.held_modifiers()),
            )
        };
        self.held_keys.insert(
            key,
            HeldKey {
                input: Some(input),
                chord,
            },
        );
        if let Some((command, binding)) = chord {
            let fires_now = match binding.trigger {
                Trigger::Press => true,
                Trigger::PressOnce => !is_repeat,
                Trigger::Release => false,
            };
            return fires_now.then_some(InputAction::Command(command));
        }

        let (command, binding) =
            self.key_map
                .command_for_key(input, KeyPhase::Down, HeldModifiers::default())?;
        if is_repeat && binding.trigger == Trigger::PressOnce {
            return None;
        }
        Some(InputAction::Command(command))
    }

    /// Process a key release and return the command it runs.
    pub fn handle_key_up(&mut self, input: impl Into<KeyInput>) -> Option<InputAction> {
        let input = input.into();
        let held = self.held_keys.remove(&input.code).unwrap_or_default();

        // The release of a key that started a chord is part of that chord.
        if let Some((command, binding)) = held.chord {
            return (binding.trigger == Trigger::Release).then_some(InputAction::Command(command));
        }

        // Otherwise the press was plain: modifiers pressed since do not
        // count, and the characters are those of the press.
        self.key_map
            .command_for_key(
                held.input.unwrap_or(input),
                KeyPhase::Up,
                HeldModifiers::default(),
            )
            .map(|(command, _)| InputAction::Command(command))
    }

    /// Process mouse button and return action if any
    pub fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        pos_px: [f32; 2],
        _world_pos: Vec2D,
        current_pan_offset: Vec2D,
    ) -> Option<InputAction> {
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                None // Left click doesn't do anything by default
            }
            (MouseButton::Right, ElementState::Pressed) => Some(InputAction::NextSpecies),
            (MouseButton::Middle, ElementState::Pressed) => {
                self.start_pan(pos_px, current_pan_offset);
                Some(InputAction::PanStart { pos_px })
            }
            (MouseButton::Middle, ElementState::Released) => {
                self.end_pan();
                Some(InputAction::PanEnd)
            }
            _ => None,
        }
    }

    /// Process mouse motion and return action if panning
    pub fn handle_mouse_motion(&mut self, x: f32, y: f32) -> Option<InputAction> {
        self.cursor_pos_px = [x, y];

        if self.is_panning {
            let dx = x - self.pan_start_pos_px[0];
            let dy = y - self.pan_start_pos_px[1];
            Some(InputAction::PanUpdate {
                delta: Vec2D::new(dx, dy),
            })
        } else {
            Some(InputAction::UpdateCursor {
                position: self.cursor_pos,
            })
        }
    }

    /// Process mouse wheel and return zoom action
    pub fn handle_mouse_wheel(
        &mut self,
        delta: MouseScrollDelta,
        center: Vec2D,
    ) -> Option<InputAction> {
        let scroll_amount = match delta {
            MouseScrollDelta::LineDelta(_, y) => y,
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 50.0,
        };

        if scroll_amount > 0.0 {
            Some(InputAction::ZoomIn { center })
        } else if scroll_amount < 0.0 {
            Some(InputAction::ZoomOut { center })
        } else {
            None
        }
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}
