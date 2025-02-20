struct VertexInput{
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(
   in: VertexInput,
) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(in.uv, 0. , 1.);
    output.uv = in.uv;
    output.color = in.color;
    return output;
}


