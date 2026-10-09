// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Solid rectangle shader: one instance = one axis-aligned, filled rectangle
// (the cursor crosshair bars).
//
// u.params: unused

struct Inst {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let q = (quad_corner(vi) * 2.0 - 1.0) * inst.half_size;
    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + q, 0.0, 1.0);
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return in.color;
}
