// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Colour bar of the field overlay (#398): one instance = one vertical bar in
// NDC, filled with the heatmap's rainbow colour map from blue at the bottom
// (low end of the field's range) to red at the top (high end).
//
// The heatmap is drawn at FIELD_OVERLAY_ALPHA over the world, whose
// background is black, so the field shows rainbow * FIELD_OVERLAY_ALPHA.
// The bar draws exactly that, opaque: blending it with alpha here would mix
// in whatever lies behind the bar (the margin and the light outline rect)
// and make it paler than the field.
//
// u.params: unused

struct Inst {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) t: f32,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let corner = quad_corner(vi);
    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + (corner * 2.0 - 1.0) * inst.half_size, 0.0, 1.0);
    // corner.y = 0 is the bottom edge in NDC.
    out.t = corner.y;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(rainbow(in.t) * FIELD_OVERLAY_ALPHA, 1.0);
}
