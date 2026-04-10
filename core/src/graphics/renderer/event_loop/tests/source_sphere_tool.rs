// Just verify the constant is accessible and has a reasonable value.
#[test]
fn default_source_radius_is_positive() {
    use super::super::unified_render_state::UnifiedRenderState;
    assert!(UnifiedRenderState::DEFAULT_SOURCE_RADIUS > 0.0);
}
