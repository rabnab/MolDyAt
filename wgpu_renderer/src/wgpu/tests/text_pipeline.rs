use super::super::text_pipeline::active_text_uv_max;

#[test]
fn active_text_uv_uses_full_texture_when_sizes_match() {
    assert_eq!(active_text_uv_max((64, 8), (64, 8)), [1.0, 1.0]);
}

#[test]
fn active_text_uv_shrinks_when_content_is_narrower_than_texture() {
    assert_eq!(active_text_uv_max((80, 8), (64, 8)), [0.8, 1.0]);
}
