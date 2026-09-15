// Endpoint marker shader: one instance = one square marker.
// Red fill with a black outline, anti-aliased.
// Requires common.wgsl to be prepended (Uniforms `u`, aa()).

struct Inst {
    @location(0) center: vec2<f32>,
    @location(1) half_size: f32,   // half the side of the red square (world units)
    @location(2) border: f32,      // outline width (world units)
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) q: vec2<f32>,                          // offset from marker centre
    @location(1) @interpolate(flat) shape: vec2<f32>,   // (half_size, border)
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    // Room for the red square, the outline, and the anti-aliased edge.
    let reach = inst.half_size + 2.0 * inst.border;

    // TriangleStrip corners: (0,0) (1,0) (0,1) (1,1)
    let cx = f32(vi & 1u);
    let cy = f32((vi >> 1u) & 1u);
    let q = vec2<f32>(mix(-reach, reach, cx), mix(-reach, reach, cy));

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + q, 0.0, 1.0);
    out.q = q;
    out.shape = vec2<f32>(inst.half_size, inst.border);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Signed distance to the red square (Chebyshev distance = square shape).
    let d = max(abs(in.q.x), abs(in.q.y)) - in.shape.x;

    let red = vec3<f32>(0.9, 0.05, 0.05);
    let black = vec3<f32>(0.0, 0.0, 0.0);

    // Inside the square: red. In the border band: black. Outside the border: transparent.
    let rgb = mix(black, red, aa(d));
    let alpha = aa(d - in.shape.y);
    return vec4<f32>(rgb, alpha);
}
