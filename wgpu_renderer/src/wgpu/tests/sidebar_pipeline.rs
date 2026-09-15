use crate::wgpu::pipelines::sidebar_pipeline::*;
use crate::wgpu::{MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_TOP};
use rust_thermo_core::graphics::sidebar::{ICON_COUNT, ICON_DISPLAY_SIZE, ICON_SPACING};

/// Background quad uses 6 vertices.
const BG_VERTS: usize = 6;

#[test]
fn sidebar_vertex_size_matches_shader_layout() {
    // position: vec2<f32> = 8, uv: vec2<f32> = 8, mode: f32 = 4 → 20 bytes
    assert_eq!(std::mem::size_of::<SidebarVertex>(), 20);
}

#[test]
fn rounded_rect_params_size() {
    // size_px: vec2<f32> = 8, corner_radius: f32 = 4, border_width: f32 = 4 → 16 bytes
    assert_eq!(std::mem::size_of::<RoundedRectParams>(), 16);
}

#[test]
fn max_vertices_includes_background_and_highlight() {
    assert_eq!(MAX_VERTICES, (ICON_COUNT + 2) * 6);
}

#[test]
fn build_geometry_produces_correct_count_for_all_icons() {
    let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    // 1 bg quad + ICON_COUNT icon quads.
    assert_eq!(verts.len(), BG_VERTS + ICON_COUNT * 6);
}

#[test]
fn build_geometry_single_icon() {
    let min_for_one = MARGIN_TOP + ICON_SPACING + ICON_DISPLAY_SIZE + MARGIN_BOTTOM;
    let (verts, _) = build_sidebar_geometry(4, 800.0, min_for_one + 1.0, None);
    assert_eq!(verts.len(), BG_VERTS + 6, "only one icon should fit");
}

#[test]
fn build_geometry_zero_icons() {
    let too_short = MARGIN_TOP + ICON_SPACING + ICON_DISPLAY_SIZE + MARGIN_BOTTOM - 1.0;
    let (verts, _) = build_sidebar_geometry(4, 800.0, too_short, None);
    assert!(verts.is_empty());
}

#[test]
fn build_geometry_clips_when_window_too_small() {
    let min_for_one = MARGIN_TOP + ICON_SPACING + ICON_DISPLAY_SIZE + MARGIN_BOTTOM;
    let (verts, _) = build_sidebar_geometry(4, 400.0, min_for_one + 1.0, None);
    assert_eq!(verts.len(), BG_VERTS + 6, "only one icon should fit");
}

#[test]
fn build_geometry_empty_when_window_too_short() {
    let too_short = MARGIN_TOP + ICON_SPACING + ICON_DISPLAY_SIZE + MARGIN_BOTTOM - 1.0;
    let (verts, _) = build_sidebar_geometry(4, 400.0, too_short, None);
    assert!(verts.is_empty());
}

#[test]
fn ndc_positions_are_within_clip_space() {
    let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    for v in &verts {
        assert!(
            v.position[0] >= -1.0 && v.position[0] <= 1.0,
            "x NDC out of range: {}",
            v.position[0]
        );
        assert!(
            v.position[1] >= -1.0 && v.position[1] <= 1.0,
            "y NDC out of range: {}",
            v.position[1]
        );
    }
}

#[test]
fn all_geometry_is_in_left_margin() {
    let win_w = 1920.0_f32;
    let (verts, _) = build_sidebar_geometry(4, win_w, 1080.0, None);
    let ndc_margin_right = (MARGIN_LEFT / win_w) * 2.0 - 1.0;
    for v in &verts {
        assert!(
            v.position[0] <= ndc_margin_right + 1e-5,
            "extends beyond left margin: NDC x={}, margin NDC x={}",
            v.position[0],
            ndc_margin_right
        );
    }
}

#[test]
fn uv_coordinates_span_atlas_correctly() {
    let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    let icons = &verts[BG_VERTS..];
    // Icon 0 is procedural ball (mode 3), so atlas UV rows start at icon 1.
    assert!((icons[6].uv[1] - 0.0).abs() < 1e-5, "wall icon uv_top");
    assert!((icons[7].uv[1] - 0.25).abs() < 1e-5, "wall icon uv_bottom");
    assert!((icons[12].uv[1] - 0.25).abs() < 1e-5, "valve icon uv_top");
    assert!((icons[13].uv[1] - 0.5).abs() < 1e-5, "valve icon uv_bottom");
    assert!((icons[24].uv[1] - 0.75).abs() < 1e-5, "source icon uv_top");
    assert!(
        (icons[25].uv[1] - 1.0).abs() < 1e-5,
        "source icon uv_bottom"
    );
}

#[test]
fn uv_x_spans_full_texture_width() {
    let (verts, _) = build_sidebar_geometry(1, 800.0, 600.0, None);
    let icons = &verts[BG_VERTS..];
    assert!((icons[0].uv[0] - 0.0).abs() < 1e-5, "TL uv.x");
    assert!((icons[1].uv[0] - 0.0).abs() < 1e-5, "BL uv.x");
    assert!((icons[2].uv[0] - 1.0).abs() < 1e-5, "TR uv.x");
    assert!((icons[5].uv[0] - 1.0).abs() < 1e-5, "BR uv.x");
}

#[test]
fn icons_are_vertically_ordered_top_to_bottom() {
    let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    let icons = &verts[BG_VERTS..];
    for i in 0..3 {
        let quad_base = i * 6;
        let next_quad_base = (i + 1) * 6;
        assert!(
            icons[next_quad_base].position[1] < icons[quad_base].position[1],
            "icon {} is not above icon {}",
            i,
            i + 1
        );
    }
}

#[test]
fn decode_icon_produces_correct_buffer_size() {
    let decoded = decode_icon(ICON_WALL, 64);
    assert_eq!(decoded.len(), 64 * 64 * 4);
}

#[test]
fn decode_icon_all_embedded_pngs_are_valid() {
    for (name, data) in [
        ("wall", ICON_WALL),
        ("valve", ICON_VALVE),
        ("thermo", ICON_THERMO),
        ("sourceOpen", ICON_SOURCE_OPEN),
    ] {
        let decoded = decode_icon(data, 64);
        assert_eq!(
            decoded.len(),
            64 * 64 * 4,
            "{name} icon decoding produced wrong size"
        );
    }
}

#[test]
fn icons_centered_horizontally_in_margin() {
    let win_w = 1920.0_f32;
    let (verts, _) = build_sidebar_geometry(1, win_w, 1080.0, None);
    let icons = &verts[BG_VERTS..];
    let expected_left_px = (MARGIN_LEFT - ICON_DISPLAY_SIZE) * 0.5;
    let expected_right_px = expected_left_px + ICON_DISPLAY_SIZE;
    let expected_left_ndc = (expected_left_px / win_w) * 2.0 - 1.0;
    let expected_right_ndc = (expected_right_px / win_w) * 2.0 - 1.0;
    assert!(
        (icons[0].position[0] - expected_left_ndc).abs() < 1e-5,
        "left edge"
    );
    assert!(
        (icons[2].position[0] - expected_right_ndc).abs() < 1e-5,
        "right edge"
    );
}

// ── Background rectangle tests ──────────────────────────────

#[test]
fn background_quad_has_mode_zero() {
    let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    for v in &verts[..BG_VERTS] {
        assert!((v.mode).abs() < 1e-5, "bg vertex mode should be 0.0");
    }
}

#[test]
fn icon_quads_have_mode_one() {
    let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    let mut mode1_count = 0usize;
    let mut mode3_count = 0usize;
    for v in &verts[BG_VERTS..] {
        if (v.mode - 1.0).abs() < 1e-5 {
            mode1_count += 1;
        } else if (v.mode - 3.0).abs() < 1e-5 {
            mode3_count += 1;
        } else {
            panic!("unexpected icon mode: {}", v.mode);
        }
    }
    assert_eq!(mode3_count, 6, "exactly one procedural ball quad expected");
    assert_eq!(
        mode1_count,
        (ICON_COUNT - 1) * 6,
        "remaining icons are textured"
    );
}

#[test]
fn background_encloses_all_icons() {
    let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    let bg = &verts[..BG_VERTS];
    let icons = &verts[BG_VERTS..];

    let bg_min_x = bg
        .iter()
        .map(|v| v.position[0])
        .fold(f32::INFINITY, f32::min);
    let bg_max_x = bg
        .iter()
        .map(|v| v.position[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let bg_min_y = bg
        .iter()
        .map(|v| v.position[1])
        .fold(f32::INFINITY, f32::min);
    let bg_max_y = bg
        .iter()
        .map(|v| v.position[1])
        .fold(f32::NEG_INFINITY, f32::max);

    for v in icons {
        assert!(v.position[0] >= bg_min_x - 1e-5, "icon left of bg");
        assert!(v.position[0] <= bg_max_x + 1e-5, "icon right of bg");
        assert!(v.position[1] >= bg_min_y - 1e-5, "icon below bg");
        assert!(v.position[1] <= bg_max_y + 1e-5, "icon above bg");
    }
}

#[test]
fn rect_params_have_positive_dimensions() {
    let (_, params) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    assert!(params.size_px[0] > 0.0, "rect width");
    assert!(params.size_px[1] > 0.0, "rect height");
    assert!(params.corner_radius > 0.0, "corner radius");
    assert!(params.border_width > 0.0, "border width");
}

#[test]
fn rect_params_width_matches_icon_plus_padding() {
    let (_, params) = build_sidebar_geometry(4, 1920.0, 1080.0, None);
    let expected_w = ICON_DISPLAY_SIZE + 2.0 * RECT_PADDING;
    assert!(
        (params.size_px[0] - expected_w).abs() < 1e-5,
        "rect width: got {}, expected {}",
        params.size_px[0],
        expected_w
    );
}

#[test]
fn rect_params_zero_when_no_icons_visible() {
    let too_short = MARGIN_TOP + ICON_SPACING + ICON_DISPLAY_SIZE + MARGIN_BOTTOM - 1.0;
    let (_, params) = build_sidebar_geometry(4, 800.0, too_short, None);
    assert!((params.size_px[0]).abs() < 1e-5, "rect width should be 0");
    assert!((params.size_px[1]).abs() < 1e-5, "rect height should be 0");
}
