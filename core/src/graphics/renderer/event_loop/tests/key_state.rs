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
