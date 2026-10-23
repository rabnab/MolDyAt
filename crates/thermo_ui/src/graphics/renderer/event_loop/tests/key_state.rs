// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::input_handler::{InputHandler, KeyCode};

#[test]
fn test_key_registration() {
    let mut handler = InputHandler::new();
    assert!(!handler.is_key_pressed(KeyCode::Space));

    handler.register_key_pressed(KeyCode::Space);
    assert!(handler.is_key_pressed(KeyCode::Space));

    handler.register_key_released(KeyCode::Space);
    assert!(!handler.is_key_pressed(KeyCode::Space));
}

#[test]
fn test_alt_modifier_detection() {
    let mut handler = InputHandler::new();
    assert!(!handler.is_alt_modifier_pressed());

    handler.register_key_pressed(KeyCode::AltLeft);
    assert!(handler.is_alt_modifier_pressed());

    handler.register_key_released(KeyCode::AltLeft);
    assert!(!handler.is_alt_modifier_pressed());

    handler.register_key_pressed(KeyCode::AltRight);
    assert!(handler.is_alt_modifier_pressed());

    handler.register_key_released(KeyCode::AltRight);
    assert!(!handler.is_alt_modifier_pressed());
}
