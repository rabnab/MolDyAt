// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Source sphere icon shader: one instance = one source sphere icon.
// The atlas at group 1 holds two slots stacked vertically:
//   slot 0 (V [0.0, 0.5)) -> sourceOpen   (emitting / On)
//   slot 1 (V [0.5, 1.0)) -> sourceClosed (dormant / Off)
//
// u.params: unused

@group(1) @binding(0) var icon_tex: texture_2d<f32>;
@group(1) @binding(1) var icon_sampler: sampler;

struct Inst {
    @location(0) center: vec2<f32>,
    @location(1) half_size: f32,
    @location(2) slot: f32,
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
    // Atlas V grows with world y, as the original CPU-built quads did.
    out.uv = vec2<f32>(c.x, (inst.slot + c.y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let color = textureSample(icon_tex, icon_sampler, in.uv);
    if color.a < 0.01 {
        discard;
    }
    return color;
}
