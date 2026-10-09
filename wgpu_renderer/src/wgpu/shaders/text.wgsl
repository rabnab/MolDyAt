// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Bitmap text shader: TriangleList of glyph quads in NDC (u.view_proj is the
// identity), sampling a one-channel glyph atlas at group 1.
//
// u.params = text colour (RGBA)

@group(1) @binding(0) var glyph_tex: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(v: Vertex) -> VOut {
    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(v.position, 0.0, 1.0);
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(glyph_tex, glyph_sampler, in.uv).r;
    if coverage < 0.01 {
        discard;
    }
    return vec4<f32>(u.params.rgb, u.params.a * coverage);
}
