// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Thermostat icon shader: one instance = one thermostat icon (thermo.png).
// Requires common.wgsl to be prepended (Uniforms `u`).

@group(1) @binding(0) var icon_tex: texture_2d<f32>;
@group(1) @binding(1) var icon_samp: sampler;

struct Inst {
    @location(0) center: vec2<f32>,
    @location(1) half_size: f32,   // half the icon's side length (world units)
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let c = quad_corner(vi);

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + (c * 2.0 - 1.0) * inst.half_size, 0.0, 1.0);
    // Image row 0 is the top; world y points up.
    out.uv = vec2<f32>(c.x, 1.0 - c.y);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(icon_tex, icon_samp, in.uv);
}
