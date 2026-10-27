// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Plot text shader (energy plot window): TriangleList of glyph quads in
// window pixels (origin top-left, y down) with a colour per vertex, sampling
// the one-channel 8x8 glyph atlas at group 1.
//
// u.params.xy = size of the render target in pixels; u.view_proj unused.

@group(1) @binding(0) var glyph_tex: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
}

@vertex
fn vs_main(v: Vertex) -> VOut {
    let size = max(u.params.xy, vec2<f32>(1.0, 1.0));
    var out: VOut;
    out.clip = vec4<f32>(
        v.position.x / size.x * 2.0 - 1.0,
        1.0 - v.position.y / size.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = v.uv;
    out.color = v.color;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(glyph_tex, glyph_sampler, in.uv).r;
    if coverage < 0.01 {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
