// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Colour bar of the field overlay (#398): one instance = one vertical bar in
// NDC, filled with the heatmap's rainbow colour map from blue at the bottom
// (low end of the field's range) to red at the top (high end).
//
// u.params: unused

const OVERLAY_ALPHA: f32 = 0.55;

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
    return vec4<f32>(rainbow(in.t), OVERLAY_ALPHA);
}
