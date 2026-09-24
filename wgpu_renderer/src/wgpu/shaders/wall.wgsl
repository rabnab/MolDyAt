// Wall shader: one instance = one wall segment.
// Draws a capsule (rectangle with rounded ends) of real world-space thickness.
// Requires common.wgsl to be prepended (Uniforms `u`, aa()).

struct Inst {
    @location(0) p0: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) thickness: f32,
    @location(3) color: vec4<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // Position in the wall's own frame: x along the wall from p0, y across it.
    @location(0) q: vec2<f32>,
    // (wall length, half thickness), constant per instance.
    @location(1) @interpolate(flat) shape: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let delta = inst.p1 - inst.p0;
    let len = length(delta);
    // Guard zero-length walls (a dot): fall back to the x axis.
    let dir = select(vec2<f32>(1.0, 0.0), delta / max(len, 1e-6), len > 1e-6);
    let nrm = vec2<f32>(-dir.y, dir.x);

    let hw = inst.thickness * 0.5;
    let pad = inst.thickness * 0.5;   // extra room so the anti-aliased edge isn't clipped

    let c = quad_corner(vi);
    let along = mix(-hw - pad, len + hw + pad, c.x);
    let across = mix(-hw - pad, hw + pad, c.y);

    let world = inst.p0 + dir * along + nrm * across;

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.q = vec2<f32>(along, across);
    out.shape = vec2<f32>(len, hw);
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let len = in.shape.x;
    let hw = max(in.shape.y, 1e-6);

    // Signed distance to a capsule running along x from 0 to len.
    let nearest = vec2<f32>(clamp(in.q.x, 0.0, len), 0.0);
    let d = length(in.q - nearest) - hw;

    // 0 at the edge, 1 at the centre line: darker rim, brighter core.
    let depth = clamp(-d / hw, 0.0, 1.0);
    let shade = mix(0.55, 1.0, smoothstep(0.0, 0.4, depth));

    return vec4<f32>(in.color.rgb * shade, in.color.a * aa(d));
}
