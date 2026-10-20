// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::input_handler::{InputAction, InputHandler};
use crate::objects2d::Vec2D;

#[test]
fn test_pan_state() {
    let mut handler = InputHandler::new();
    assert!(!handler.is_panning());

    handler.start_pan([100.0, 200.0], Vec2D::new(10.0, 20.0));
    assert!(handler.is_panning());
    assert_eq!(handler.pan_start_pos_px(), [100.0, 200.0]);
    assert_eq!(handler.pan_start_offset(), Vec2D::new(10.0, 20.0));

    handler.end_pan();
    assert!(!handler.is_panning());
}

#[test]
fn test_cursor_position_tracking() {
    let mut handler = InputHandler::new();
    let world_pos = Vec2D::new(5.0, 10.0);
    let screen_pos = [100.0, 200.0];

    handler.set_cursor_pos(world_pos, screen_pos);

    assert_eq!(handler.cursor_pos(), world_pos);
    assert_eq!(handler.cursor_pos_px(), screen_pos);
}

#[test]
fn test_right_button_cycles_ball_size() {
    let mut handler = InputHandler::new();
    let action = handler.handle_mouse_button(
        winit::event::MouseButton::Right,
        winit::event::ElementState::Pressed,
        [100.0, 200.0],
        Vec2D::new(5.0, 10.0),
        Vec2D::zero(),
    );
    assert!(matches!(action, Some(InputAction::NextSpecies)));
}

#[test]
fn test_middle_button_starts_pan() {
    let mut handler = InputHandler::new();
    let action = handler.handle_mouse_button(
        winit::event::MouseButton::Middle,
        winit::event::ElementState::Pressed,
        [100.0, 200.0],
        Vec2D::new(5.0, 10.0),
        Vec2D::zero(),
    );
    assert!(matches!(action, Some(InputAction::PanStart { pos_px }) if pos_px == [100.0, 200.0]));
    assert!(handler.is_panning());
}

#[test]
fn test_middle_button_ends_pan() {
    let mut handler = InputHandler::new();
    handler.start_pan([100.0, 200.0], Vec2D::zero());
    let action = handler.handle_mouse_button(
        winit::event::MouseButton::Middle,
        winit::event::ElementState::Released,
        [150.0, 250.0],
        Vec2D::new(7.0, 12.0),
        Vec2D::zero(),
    );
    assert!(matches!(action, Some(InputAction::PanEnd)));
    assert!(!handler.is_panning());
}
