// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::command_registry::{
    CommandDescriptor, HeldModifiers, KeyBinding, KeyPhase, Trigger, UiAction, chord_for_key,
    command_for_key,
};
use std::collections::{HashMap, HashSet};
use thermo_physics::objects2d::Vec2D;
use thermo_runtime::application::operator_command::OperatorCommand;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

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
    pressed_keys: HashSet<KeyCode>,
    cursor_pos: Vec2D,
    cursor_pos_px: [f32; 2],

    // Pan state
    is_panning: bool,
    pan_start_pos_px: [f32; 2],
    pan_start_offset: Vec2D,

    // Keys pressed as a chord (Ctrl+S, Alt+=), with the command their
    // release runs (`None`: the chord fired on press). The release belongs
    // to the chord even if the modifier was let go first, so it never falls
    // through to the plain key's command (S = stepwise, = = double dt).
    chord_keys: HashMap<KeyCode, (&'static CommandDescriptor, KeyBinding)>,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            pressed_keys: HashSet::new(),
            cursor_pos: Vec2D::zero(),
            cursor_pos_px: [0.0, 0.0],
            is_panning: false,
            pan_start_pos_px: [0.0, 0.0],
            pan_start_offset: Vec2D::zero(),
            chord_keys: HashMap::new(),
        }
    }

    // Key state tracking
    pub fn register_key_pressed(&mut self, key: KeyCode) {
        self.pressed_keys.insert(key);
    }

    pub fn register_key_released(&mut self, key: KeyCode) {
        self.pressed_keys.remove(&key);
    }

    /// Forget every held key. Called when the window loses focus: keys
    /// released while another window has focus never send a release here,
    /// and a Ctrl that stayed "held" would turn plain S / O into save / load.
    pub fn release_all_keys(&mut self) {
        self.pressed_keys.clear();
        self.chord_keys.clear();
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    pub fn is_alt_modifier_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::AltLeft)
            || self.pressed_keys.contains(&KeyCode::AltRight)
    }

    pub fn is_ctrl_modifier_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::ControlLeft)
            || self.pressed_keys.contains(&KeyCode::ControlRight)
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
    pub fn handle_key_down(&mut self, key: KeyCode, is_repeat: bool) -> Option<InputAction> {
        self.register_key_pressed(key);

        // A chord is decided on the first press, while its modifier is
        // certainly held, and owns this key until the release, whatever its
        // trigger. An auto-repeat continues the press it repeats: a modifier
        // pressed while a plain key is held does not turn it into a chord.
        let chord = if is_repeat {
            self.chord_keys.get(&key).copied()
        } else {
            chord_for_key(key, self.held_modifiers())
        };
        if let Some((command, binding)) = chord {
            self.chord_keys.insert(key, (command, binding));
            let fires_now = match binding.trigger {
                Trigger::Press => true,
                Trigger::PressOnce => !is_repeat,
                Trigger::Release => false,
            };
            return fires_now.then_some(InputAction::Command(command));
        }

        let (command, binding) = command_for_key(key, KeyPhase::Down, HeldModifiers::default())?;
        if is_repeat && binding.trigger == Trigger::PressOnce {
            return None;
        }
        Some(InputAction::Command(command))
    }

    /// Process a key release and return the command it runs.
    pub fn handle_key_up(&mut self, key: KeyCode) -> Option<InputAction> {
        self.register_key_released(key);

        // The release of a key that started a chord is part of that chord.
        if let Some((command, binding)) = self.chord_keys.remove(&key) {
            return (binding.trigger == Trigger::Release).then_some(InputAction::Command(command));
        }

        // Otherwise the press was plain: modifiers pressed since do not count.
        command_for_key(key, KeyPhase::Up, HeldModifiers::default())
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
