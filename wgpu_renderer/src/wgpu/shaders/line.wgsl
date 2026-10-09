// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Hairline shader: LineList of world-space vertices with per-vertex colour
// (neighbour lines, source-sphere arcs, snap indicators).
//
// u.params: unused

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(v: Vertex) -> VOut {
    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(v.position, 0.0, 1.0);
    out.color = v.color;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return in.color;
}
