// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Label capsule shader: one instance = one semi-transparent white capsule
// drawn behind a thermostat text label.
//
// Positions are already in NDC (the label code applies view_proj on the CPU),
// so u.view_proj is the identity.
// Requires common.wgsl to be prepended (Uniforms `u`, aa()).

const CAPSULE_ALPHA: f32 = 0.75;   // opacity of the white capsule

struct Inst {
    @location(0) center: vec2<f32>,     // NDC
    @location(1) half_ext: vec2<f32>,   // half width / half height in NDC, padding included
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) q: vec2<f32>,                            // offset from the centre (NDC)
    @location(1) @interpolate(flat) half_ext: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let m = 0.3 * inst.half_ext.y;   // margin so the anti-aliased edge isn't clipped

    let q = (quad_corner(vi) * 2.0 - 1.0) * (inst.half_ext + m);

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + q, 0.0, 1.0);
    out.q = q;
    out.half_ext = inst.half_ext;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // NDC units per pixel along screen x and y (constant over an axis-aligned
    // quad). Working in real pixels makes the end caps true semicircles at any
    // window aspect ratio, and makes fwidth() ~ 1 so edges are 1 px soft.
    let ndc_per_px = max(
        vec2<f32>(abs(dpdx(in.q.x)), abs(dpdy(in.q.y))),
        vec2<f32>(1e-9),
    );
    let p = in.q / ndc_per_px;           // pixel offset from the capsule centre
    let h = in.half_ext / ndc_per_px;    // half extents in pixels
    let r = h.y;                         // capsule: end radius = half height

    // Signed distance (pixels) to a rounded box with radius r.
    let e = abs(p) - vec2<f32>(max(h.x - r, 0.0), 0.0);
    let d = length(max(e, vec2<f32>(0.0))) + min(max(e.x, e.y), 0.0) - r;

    return vec4<f32>(1.0, 1.0, 1.0, CAPSULE_ALPHA * aa(d));
}
