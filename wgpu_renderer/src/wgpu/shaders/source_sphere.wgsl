// Source sphere icon shader.
//
// Renders icon quads with world-space coordinates transformed by the
// view-projection matrix.  The icon atlas contains two slots:
//   UV [0.0, 0.5) → sourceOpen  (emitting / On state)
//   UV [0.5, 1.0) → sourceClosed (dormant / Off state)

struct Uniforms {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var icon_tex: texture_2d<f32>;

@group(0) @binding(2)
var icon_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = uniforms.view_proj * vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(icon_tex, icon_sampler, in.uv);
    if color.a < 0.01 {
        discard;
    }
    return color;
}
