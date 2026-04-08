// Sidebar shader — renders icon texture quads and a rounded-rect background.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mode: f32,
}

struct RectParams {
    size_px: vec2<f32>,
    corner_radius: f32,
    border_width: f32,
}

@group(0) @binding(0)
var icon_tex: texture_2d<f32>;
@group(0) @binding(1)
var icon_sampler: sampler;
@group(0) @binding(2)
var<uniform> rect: RectParams;

@vertex
fn vs_main(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mode: f32,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    out.mode = mode;
    return out;
}

// Signed distance to a rounded rectangle centered at origin.
fn sd_rounded_rect(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Medium gray fill, purple outline.
const FILL_COLOR: vec4<f32> = vec4<f32>(0.5, 0.5, 0.5, 1.0);
const OUTLINE_COLOR: vec4<f32> = vec4<f32>(0.6, 0.2, 0.8, 1.0);
const HIGHLIGHT_BORDER_COLOR: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 1.0);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.mode < 0.5 {
        // Rounded rectangle background.
        // UV [0,1] maps to pixel coords centered on the rect.
        let p = (in.uv - vec2<f32>(0.5)) * rect.size_px;
        let half = rect.size_px * 0.5;
        let d = sd_rounded_rect(p, half, rect.corner_radius);

        if d > 1.0 {
            discard;
        }

        let aa = 1.0 - smoothstep(-0.5, 0.5, d);
        let border_blend = smoothstep(
            -rect.border_width - 0.5,
            -rect.border_width + 0.5,
            d
        );
        let color = mix(FILL_COLOR, OUTLINE_COLOR, border_blend);
        return vec4<f32>(color.rgb, color.a * aa);
    }

    // Icon texture sampling.
    if in.mode > 1.5 {
        // Solid border frame around the selected icon (mode 2.0).
        // Border width ~2px on a ~56px quad ≈ 0.036 in UV space.
        let bw = 0.04;
        let u = in.uv.x;
        let v = in.uv.y;
        if u > bw && u < (1.0 - bw) && v > bw && v < (1.0 - bw) {
            discard;
        }
        return HIGHLIGHT_BORDER_COLOR;
    }

    let color = textureSample(icon_tex, icon_sampler, in.uv);
    if color.a < 0.01 {
        discard;
    }
    return color;
}
