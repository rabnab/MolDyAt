use crate::objects2d::Vec2D;
use std::collections::HashSet;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, VirtualKeyCode};

// Type alias for compatibility - in winit 0.28.7 it's VirtualKeyCode
// In future winit versions it will be winit::keyboard::KeyCode
pub type KeyCode = VirtualKeyCode;

/// Actions that can result from user input
#[derive(Debug, Clone)]
pub enum InputAction {
    // Ball manipulation
    AddBall { size_idx: usize, position: Vec2D },
    RemoveBallAt { position: Vec2D },
    CycleBallSize { increase: bool },

    // Simulation control
    TogglePause,
    SingleStep,
    Reset,

    // View control
    ZoomIn { center: Vec2D },
    ZoomOut { center: Vec2D },
    PanStart { pos_px: [f32; 2] },
    PanUpdate { delta: Vec2D },
    PanEnd,
    ResetView,

    // Rendering
    ToggleRendering,
    CyclePlotContent,
    ToggleEnergyWindow,

    // Cursor
    UpdateCursor { position: Vec2D },
    EnableCursor { repeated: bool },
    DisableCursor,

    // Application
    Quit,
}

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
        }
    }

    // Key state tracking
    pub fn register_key_pressed(&mut self, key: KeyCode) {
        self.pressed_keys.insert(key);
    }

    pub fn register_key_released(&mut self, key: KeyCode) {
        self.pressed_keys.remove(&key);
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

        match key {
            KeyCode::Space => Some(InputAction::TogglePause),
            KeyCode::S if !is_repeat => Some(InputAction::SingleStep),
            KeyCode::R => Some(InputAction::Reset),
            KeyCode::F1 => Some(InputAction::CyclePlotContent),
            KeyCode::F2 => Some(InputAction::ToggleRendering),
            KeyCode::F3 => Some(InputAction::ToggleEnergyWindow),
            KeyCode::Home => Some(InputAction::ResetView),
            KeyCode::Escape => Some(InputAction::Quit),
            KeyCode::Key1 | KeyCode::Key2 | KeyCode::Key3 | KeyCode::Key4 | KeyCode::Key5 => {
                // Number keys for ball size selection handled by caller
                None
            }
            _ => None,
        }
    }

    /// Process keyboard release and return action if any
    pub fn handle_key_up(&mut self, key: KeyCode) -> Option<InputAction> {
        self.register_key_released(key);

        match key {
            KeyCode::A => Some(InputAction::DisableCursor),
            _ => None,
        }
    }

    /// Process mouse button and return action if any
    pub fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        pos_px: [f32; 2],
        world_pos: Vec2D,
        current_pan_offset: Vec2D,
    ) -> Option<InputAction> {
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                if self.is_key_pressed(KeyCode::LShift) || self.is_key_pressed(KeyCode::RShift) {
                    Some(InputAction::RemoveBallAt {
                        position: world_pos,
                    })
                } else {
                    // Ball size determined by number key or default
                    let size_idx = self.get_selected_ball_size();
                    Some(InputAction::AddBall {
                        size_idx,
                        position: world_pos,
                    })
                }
            }
            (MouseButton::Right, ElementState::Pressed) => {
                self.start_pan(pos_px, current_pan_offset);
                Some(InputAction::PanStart { pos_px })
            }
            (MouseButton::Right, ElementState::Released) => {
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

    /// Get currently selected ball size from number keys (1-5)
    pub(crate) fn get_selected_ball_size(&self) -> usize {
        if self.is_key_pressed(KeyCode::Key1) {
            0
        } else if self.is_key_pressed(KeyCode::Key2) {
            1
        } else if self.is_key_pressed(KeyCode::Key3) {
            2
        } else if self.is_key_pressed(KeyCode::Key4) {
            3
        } else if self.is_key_pressed(KeyCode::Key5) {
            4
        } else {
            2 // Default to medium size
        }
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}
