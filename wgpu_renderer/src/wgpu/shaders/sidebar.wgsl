// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Sidebar shader: TriangleList in NDC (u.view_proj is the identity) for the
// rounded-rect background, the selection highlight and the tool icons.
// Icons sample the icon atlas at group 1.
//
// Vertex `mode`: 0 = background, 1 = atlas icon, 2 = selection highlight,
//                3 = procedural ball icon
// u.params = (background width px, background height px, corner radius px,
//             border width px)

@group(1) @binding(0) var icon_tex: texture_2d<f32>;
@group(1) @binding(1) var icon_sampler: sampler;

// Medium gray fill, purple outline.
const FILL_COLOR: vec4<f32> = vec4<f32>(0.5, 0.5, 0.5, 1.0);
const OUTLINE_COLOR: vec4<f32> = vec4<f32>(0.6, 0.2, 0.8, 1.0);
const HIGHLIGHT_BORDER_COLOR: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 1.0);
const BALL_BASE_COLOR: vec3<f32> = vec3<f32>(0.80, 0.82, 0.86);
const BALL_DARK_COLOR: vec3<f32> = vec3<f32>(0.58, 0.60, 0.66);

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mode: f32,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) mode: f32,
}

@vertex
fn vs_main(v: Vertex) -> VOut {
    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(v.position, 0.0, 1.0);
    out.uv = v.uv;
    out.mode = v.mode;
    return out;
}

// Signed distance to a rounded rectangle centered at origin.
fn sd_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn background(uv: vec2<f32>) -> vec4<f32> {
    let size_px = u.params.xy;
    let corner_radius = u.params.z;
    let border_width = u.params.w;

    // UV [0,1] maps to pixel coords centered on the rect.
    let d = sd_rounded_rect((uv - vec2<f32>(0.5)) * size_px, size_px * 0.5, corner_radius);
    if d > 1.0 {
        discard;
    }
    let coverage = 1.0 - smoothstep(-0.5, 0.5, d);
    let border_blend = smoothstep(-border_width - 0.5, -border_width + 0.5, d);
    let color = mix(FILL_COLOR, OUTLINE_COLOR, border_blend);
    return vec4<f32>(color.rgb, color.a * coverage);
}

fn ball_icon(uv: vec2<f32>) -> vec4<f32> {
    let p = uv - vec2<f32>(0.5);
    let r = length(p);
    if r > 0.46 {
        discard;
    }
    // Radial body shading plus a soft specular highlight near the top-left.
    let body = mix(BALL_BASE_COLOR, BALL_DARK_COLOR, smoothstep(0.02, 0.46, r));
    let h = (p - vec2<f32>(-0.16, -0.16)) * 12.0;
    let color = body + vec3<f32>(0.30) * exp(-dot(h, h));
    return vec4<f32>(color, 1.0 - smoothstep(0.44, 0.47, r));
}

fn highlight(uv: vec2<f32>) -> vec4<f32> {
    // Border width ~2px on a ~56px quad ≈ 0.04 in UV space.
    let bw = 0.04;
    if all(uv > vec2<f32>(bw)) && all(uv < vec2<f32>(1.0 - bw)) {
        discard;
    }
    return HIGHLIGHT_BORDER_COLOR;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Sample before branching (textureSample needs uniform control flow).
    let icon = textureSample(icon_tex, icon_sampler, in.uv);

    if in.mode < 0.5 {
        return background(in.uv);
    }
    if in.mode > 2.5 {
        return ball_icon(in.uv);
    }
    if in.mode > 1.5 {
        return highlight(in.uv);
    }
    if icon.a < 0.01 {
        discard;
    }
    return icon;
}
