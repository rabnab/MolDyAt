// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Plot segment shader (energy plot window): one instance = one straight line
// with round caps, in window pixels (origin top-left, y down). The quad covers
// the segment plus one pixel of anti-aliasing on every side; the fragment
// stage keeps the pixels within `half_width` of the segment (a capsule).
// A segment of length zero is a dot.
//
// u.params.xy = size of the render target in pixels; u.view_proj unused.

struct Inst {
    @location(0) p0: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) half_width: f32,
    @location(3) color: vec4<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // Position along (x) and across (y) the segment, in pixels from p0.
    @location(0) local: vec2<f32>,
    // Segment length and half width, in pixels.
    @location(1) @interpolate(flat) len_hw: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let d = inst.p1 - inst.p0;
    let len = length(d);
    var dir = vec2<f32>(1.0, 0.0);
    if len > 1e-4 {
        dir = d / len;
    }
    let normal = vec2<f32>(-dir.y, dir.x);
    let reach = inst.half_width + 1.0;
    let c = quad_corner(vi);
    let along = mix(-reach, len + reach, c.x);
    let across = mix(-reach, reach, c.y);
    let px = inst.p0 + dir * along + normal * across;
    let size = max(u.params.xy, vec2<f32>(1.0, 1.0));

    var out: VOut;
    out.clip = vec4<f32>(px.x / size.x * 2.0 - 1.0, 1.0 - px.y / size.y * 2.0, 0.0, 1.0);
    out.local = vec2<f32>(along, across);
    out.len_hw = vec2<f32>(len, inst.half_width);
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let t = clamp(in.local.x, 0.0, in.len_hw.x);
    let dist = length(vec2<f32>(in.local.x - t, in.local.y));
    let coverage = clamp(in.len_hw.y + 0.5 - dist, 0.0, 1.0);
    if coverage <= 0.0 {
        discard;
    }
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
