struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    //let center = in.position.xy; //Center of the screen in NDC
    //let uv = in.uv;
    let distance = 0.15; 

    let radius = 0.2; // radius in relative coordinates (1 is screen-size, 0.5 half)

    let alpha = 1.0 - clamp(distance / radius, 0.0, 1.0); // Transparency based on distance

    return vec4<f32>(0.1, 0.8, 0.3, 0.); // Eerie green with decreasing transparency
}
