// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Shared header, prepended to every shader by `gpu/shaders.rs`
// (WGSL has no #include). Must match `SceneUniforms` in gpu/batch.rs.
//
// Conventions for all shaders:
// - Entry points are `vs_main` / `fs_main`.
// - `u.view_proj` maps the shader's input positions to clip space; it is the
//   identity for pipelines whose positions are already in NDC.
// - `u.params` is pipeline-specific; each shader documents its use.
// - An optional texture + sampler lives at @group(1) @binding(0) / (1).
// - Instanced quads are TriangleStrips of 4 vertices; use `quad_corner()`.

struct Uniforms {
    view_proj: mat4x4<f32>,
    params: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

// Corner of an instanced quad (TriangleStrip): (0,0) (1,0) (0,1) (1,1).
fn quad_corner(vi: u32) -> vec2<f32> {
    return vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
}

// Anti-aliased coverage from a signed distance `d` in world units
// (negative inside the shape). fwidth(d) is roughly one pixel in world units,
// so edges stay ~1 px soft at any zoom. Fragment stage only.
fn aa(d: f32) -> f32 {
    let w = max(fwidth(d), 1e-6);
    return 1.0 - smoothstep(-w, w, d);
}

// Coverage of an outline band of `width` just inside the edge of a shape with
// signed distance `d` (world units, negative inside), with the same ~1 px
// anti-aliased edges as aa(). Uses derivatives: call it before any branching.
fn inset_band(d: f32, width: f32) -> f32 {
    return max(aa(d) - aa(d + width), 0.0);
}

// Rainbow colour map of the field overlays: blue (t=0) -> cyan -> green ->
// yellow -> red (t=1). The heatmap and its colour bar both use it.
fn rainbow(t: f32) -> vec3<f32> {
    let s = clamp(t, 0.0, 1.0) * 4.0;
    // Cap the segment index at 3: at t = 1.0 exactly, floor(s) would be 4, which
    // wrapped back to yellow (and values a hair below 1.0 stayed red).
    let i = min(floor(s), 3.0);
    let f = s - i;
    if i < 1.0 {
        return vec3<f32>(0.0, f, 1.0);         // blue -> cyan
    } else if i < 2.0 {
        return vec3<f32>(0.0, 1.0, 1.0 - f);   // cyan -> green
    } else if i < 3.0 {
        return vec3<f32>(f, 1.0, 0.0);         // green -> yellow
    }
    return vec3<f32>(1.0, 1.0 - f, 0.0);       // yellow -> red
}

// Coverage of a circular outline of `width` centred on `radius`, at distance
// `dist` from the centre (all world units). Zero outside the band, with a fixed
// 0.05 soft edge (no derivatives, so it is safe inside branches).
fn ring_band(dist: f32, radius: f32, width: f32) -> f32 {
    let inner = radius - width * 0.5;
    let outer = radius + width * 0.5;
    return smoothstep(inner, inner + 0.05, dist) * (1.0 - smoothstep(outer - 0.05, outer, dist));
}
