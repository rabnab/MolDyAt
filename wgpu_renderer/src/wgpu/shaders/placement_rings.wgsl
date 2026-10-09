// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Ball placement preview: one instance = three outlines around the cursor
// showing the new ball's radius (white), its equilibrium radius (magenta) and
// its cutoff radius (gray). Matches the rings in ball.wgsl.
//
// u.params: unused

const STROKE_WIDTH: f32 = 0.15;

struct Inst {
    @location(0) center: vec2<f32>,
    // (ball radius, equilibrium radius, cutoff radius)
    @location(1) radii: vec3<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) q: vec2<f32>,
    @location(1) @interpolate(flat) radii: vec3<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    // Half a stroke of extra room, as in ball.wgsl.
    let reach = inst.radii.z + STROKE_WIDTH * 0.5;
    let q = (quad_corner(vi) * 2.0 - 1.0) * reach;

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + q, 0.0, 1.0);
    out.q = q;
    out.radii = inst.radii;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let dist = length(in.q);

    let cutoff = ring_band(dist, in.radii.z, STROKE_WIDTH);
    if cutoff > 0.0 {
        return vec4<f32>(0.43, 0.43, 0.43, cutoff);
    }
    let eq = ring_band(dist, in.radii.y, STROKE_WIDTH);
    if eq > 0.0 {
        return vec4<f32>(1.0, 0.0, 1.0, eq);
    }
    return vec4<f32>(1.0, 1.0, 1.0, ring_band(dist, in.radii.x, STROKE_WIDTH));
}
