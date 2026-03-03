// Simple monochrome text shader for screen-space quads

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0)
var text_tex: texture_2d<f32>;
@group(0) @binding(1)
var text_sampler: sampler;

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(text_tex, text_sampler, in.uv).r;
    if alpha < 0.01 {
        discard;
    }
    // Light gray text
    return vec4<f32>(0.7, 0.7, 0.7, alpha);
}
