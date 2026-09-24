// Frame outline shader: LineStrip around the simulation viewport, in NDC
// (u.view_proj is the identity).
//
// u.params: unused

const FRAME_COLOR: vec4<f32> = vec4<f32>(0.2, 0.4, 1.0, 1.0);

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> @builtin(position) vec4<f32> {
    return u.view_proj * vec4<f32>(position, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return FRAME_COLOR;
}
