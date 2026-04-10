use crate::graphics::renderer::event_loop::DEFAULT_SOURCE_RADIUS;

// Just verify the constant is accessible and has a reasonable value.
#[test]
fn default_source_radius_is_positive() {
    assert!(DEFAULT_SOURCE_RADIUS > 0.0);
}
