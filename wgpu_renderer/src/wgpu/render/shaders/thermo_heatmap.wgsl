// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Thermo heatmap overlay: one instance covers the simulation world area.
//
// The scalar field (temperature or pressure, NaN-filled, smoothed and
// normalised to [0,1]) is an R8Unorm texture at group 1, one texel per cell.
//
// The field is reconstructed with a cubic B-spline filter instead of plain
// bilinear filtering. It is C2-continuous, so a coarse grid looks like a smooth
// surface rather than a mosaic, and unlike Catmull-Rom it never overshoots the
// data range, so it can't create ringing artefacts in a noisy field. It costs
// four bilinear taps per pixel (Sigg & Hadwiger, GPU Gems 2, ch. 20).
//
// Convention: `origin` is the world-space bottom-left corner and UV (0,0) is
// bottom-left. Texture row 0 holds ThermoGrid row 0 (= world bottom), so the
// texture V matches UV.y directly.
//
// u.params: unused

const OVERLAY_ALPHA: f32 = 0.55;

@group(1) @binding(0) var field_texture: texture_2d<f32>;
@group(1) @binding(1) var field_sampler: sampler;

struct Inst {
    @location(0) origin: vec2<f32>,
    @location(1) size: vec2<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let uv = quad_corner(vi);
    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.origin + uv * inst.size, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Cubic B-spline weights for the four texels around a sample point,
// where `f` in [0,1) is the fractional position between the middle two.
fn bspline_weights(f: f32) -> vec4<f32> {
    let f2 = f * f;
    let f3 = f2 * f;
    return vec4<f32>(
        (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0,
        (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0,
        (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0,
        f3 / 6.0,
    );
}

// Bicubic B-spline reconstruction of the scalar field using 4 bilinear taps.
fn sample_field(uv: vec2<f32>) -> f32 {
    let dims = vec2<f32>(textureDimensions(field_texture));

    // Texel space with texel centres at integer coordinates.
    let p = uv * dims - 0.5;
    let base = floor(p);
    let f = p - base;

    let wx = bspline_weights(f.x);
    let wy = bspline_weights(f.y);

    // Merge each pair of neighbouring weights into one bilinear tap.
    let g0 = vec2<f32>(wx.x + wx.y, wy.x + wy.y);
    let g1 = vec2<f32>(wx.z + wx.w, wy.z + wy.w);
    let h0 = vec2<f32>(wx.y, wy.y) / g0;
    let h1 = vec2<f32>(wx.w, wy.w) / g1;

    // Tap positions back in UV space (+0.5 = texel centre).
    let t0 = (base - 1.0 + h0 + 0.5) / dims;
    let t1 = (base + 1.0 + h1 + 0.5) / dims;

    let t00 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(t0.x, t0.y), 0.0).r;
    let t10 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(t1.x, t0.y), 0.0).r;
    let t01 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(t0.x, t1.y), 0.0).r;
    let t11 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(t1.x, t1.y), 0.0).r;

    return (t00 * g0.x + t10 * g1.x) * g0.y + (t01 * g0.x + t11 * g1.x) * g1.y;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(rainbow(sample_field(in.uv)), OVERLAY_ALPHA);
}
