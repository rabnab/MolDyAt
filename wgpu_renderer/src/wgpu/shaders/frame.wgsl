// Simple frame outline shader (NDC space)

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    // Match ggez boundary frame color (blue-ish)
    return vec4<f32>(0.2, 0.4, 1.0, 1.0);
}
