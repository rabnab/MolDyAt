// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::operator_command::OperatorCommand;
use crate::objects2d::Vec2D;
use std::collections::HashSet;
use std::time::{Duration, Instant};
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

/// Actions that can result from user input
#[derive(Debug)]
pub enum InputAction {
    // Ball manipulation
    AddBall {
        position: Vec2D,
    },
    NextBallSize,
    AddRandomBalls {
        count: usize,
    },
    StopAllBalls,
    ClearBalls,

    // Save / load the whole world (Ctrl+S / Ctrl+O)
    SaveWorld,
    LoadWorld,

    // Simulation control
    TogglePause,
    ToggleGravity,
    ToggleStepwise,
    RandomizeSpeeds,
    EnableRandomBallSpeed,
    DoubleTimeStep,
    HalveTimeStep,
    IncreaseWaitTime,
    DecreaseWaitTime,

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
    ResetView,

    // Rendering
    ToggleRendering,
    CyclePlotContent,
    CycleThermoDisplay,
    ToggleEnergyWindow,
    ToggleCutoffRings,

    // Display/Debug
    PrintHelp,
    ToggleRealData,
    ToggleTimingDisplay,
    ToggleBallStats,
    StartVerbosityInput,

    // Cursor
    UpdateCursor {
        position: Vec2D,
    },

    // Operator panel
    SelectOperatorTool(OperatorTool),
    /// Delete the object the active tool highlights under the cursor.
    DeleteHovered,
    /// A self-contained operator command routed to the physics thread.
    OperatorAction(Box<dyn OperatorCommand>),
}

/// Cooldown period between +/- key commands (in milliseconds)
const PLUS_MINUS_COOLDOWN_MS: u64 = 150;

/// Handles keyboard and mouse input, tracking state and producing actions
pub struct InputHandler {
    pressed_keys: HashSet<KeyCode>,
    pressed_alt_keys: HashSet<KeyCode>,
    cursor_pos: Vec2D,
    cursor_pos_px: [f32; 2],

    // Pan state
    is_panning: bool,
    pan_start_pos_px: [f32; 2],
    pan_start_offset: Vec2D,

    // Debounce tracking for +/- keys (prevents rapid-fire commands)
    last_plus_minus_time: Option<Instant>,

    // Keys that started a Ctrl chord (Ctrl+S / Ctrl+O). Their release is
    // consumed even if Ctrl was let go first, so it never falls through to
    // the plain-key action (S = stepwise).
    chord_keys: HashSet<KeyCode>,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            pressed_keys: HashSet::new(),
            pressed_alt_keys: HashSet::new(),
            cursor_pos: Vec2D::zero(),
            cursor_pos_px: [0.0, 0.0],
            is_panning: false,
            pan_start_pos_px: [0.0, 0.0],
            pan_start_offset: Vec2D::zero(),
            last_plus_minus_time: None,
            chord_keys: HashSet::new(),
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
        self.pressed_alt_keys.clear();
        self.chord_keys.clear();
    }

    pub fn register_alt_key_pressed(&mut self, key: KeyCode) {
        self.pressed_alt_keys.insert(key);
    }

    pub fn register_alt_key_released(&mut self, key: KeyCode) {
        self.pressed_alt_keys.remove(&key);
    }

    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    pub fn is_alt_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_alt_keys.contains(&key)
    }

    pub fn is_alt_modifier_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::AltLeft)
            || self.pressed_keys.contains(&KeyCode::AltRight)
    }

    pub fn is_ctrl_modifier_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::ControlLeft)
            || self.pressed_keys.contains(&KeyCode::ControlRight)
    }

    // Debounce tracking for +/- keys with cooldown period
    pub fn can_process_plus_minus(&self) -> bool {
        match self.last_plus_minus_time {
            None => true, // Never processed before
            Some(last_time) => {
                let elapsed = Instant::now().duration_since(last_time);
                elapsed >= Duration::from_millis(PLUS_MINUS_COOLDOWN_MS)
            }
        }
    }

    pub fn mark_plus_minus_processed(&mut self) {
        self.last_plus_minus_time = Some(Instant::now());
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

    /// Process keyboard input and return action if any
    pub fn handle_key_down(&mut self, key: KeyCode, is_repeat: bool) -> Option<InputAction> {
        self.register_key_pressed(key);

        // Ctrl+S / Ctrl+O act on key press, so the chord is decided while Ctrl
        // is certainly held; auto-repeat must not save or load again.
        if self.is_ctrl_modifier_pressed() {
            let action = match key {
                KeyCode::KeyS => Some(InputAction::SaveWorld),
                KeyCode::KeyO => Some(InputAction::LoadWorld),
                _ => None,
            };
            if action.is_some() {
                self.chord_keys.insert(key);
                return if is_repeat { None } else { action };
            }
        }

        match key {
            KeyCode::Space => Some(InputAction::TogglePause),
            KeyCode::ControlLeft | KeyCode::ControlRight => None,
            KeyCode::Escape => Some(InputAction::ClearBalls),
            // Backspace too: many laptop keyboards have no Delete key.
            KeyCode::Delete | KeyCode::Backspace if !is_repeat => Some(InputAction::DeleteHovered),
            KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(InputAction::EnableRandomBallSpeed),
            KeyCode::Minus | KeyCode::Equal => {
                // These are handled separately for alt-key tracking
                None
            }
            _ => None,
        }
    }

    /// Process keyboard release and return action if any
    pub fn handle_key_up(&mut self, key: KeyCode) -> Option<InputAction> {
        self.register_key_released(key);

        // The release of a key that started a chord is part of that chord.
        if self.chord_keys.remove(&key) {
            return None;
        }

        match key {
            KeyCode::Tab => Some(InputAction::ToggleEnergyWindow),
            KeyCode::F2 => Some(InputAction::ToggleRendering),
            KeyCode::F3 => Some(InputAction::ToggleCutoffRings),
            KeyCode::KeyH => Some(InputAction::PrintHelp),
            KeyCode::KeyD => Some(InputAction::ToggleRealData),
            KeyCode::KeyG => Some(InputAction::ToggleGravity),
            KeyCode::KeyS => Some(InputAction::ToggleStepwise),
            KeyCode::KeyT => Some(InputAction::CycleThermoDisplay),
            KeyCode::F4 => Some(InputAction::ToggleTimingDisplay),
            KeyCode::KeyP => Some(InputAction::CyclePlotContent),
            KeyCode::KeyR => Some(InputAction::RandomizeSpeeds),
            KeyCode::KeyB => Some(InputAction::ToggleBallStats),
            KeyCode::KeyV => Some(InputAction::StartVerbosityInput),
            KeyCode::ControlLeft | KeyCode::ControlRight => None,
            KeyCode::F5 => Some(InputAction::AddRandomBalls { count: 100 }),
            KeyCode::F1 => Some(InputAction::StopAllBalls),
            KeyCode::Digit0 | KeyCode::Numpad0 => Some(InputAction::ResetView),
            // Time step control with +/- keys
            KeyCode::Equal | KeyCode::NumpadAdd => {
                if self.is_alt_modifier_pressed() {
                    Some(InputAction::DecreaseWaitTime)
                } else {
                    Some(InputAction::DoubleTimeStep)
                }
            }
            KeyCode::Minus | KeyCode::NumpadSubtract => {
                if self.is_alt_modifier_pressed() {
                    Some(InputAction::IncreaseWaitTime)
                } else {
                    Some(InputAction::HalveTimeStep)
                }
            }
            _ => None,
        }
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
            (MouseButton::Right, ElementState::Pressed) => Some(InputAction::NextBallSize),
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
