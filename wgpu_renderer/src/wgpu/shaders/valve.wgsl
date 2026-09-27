// Valve shader: one instance = one valve rectangle laid over a wall.
//   style 0 = passable (open):  dark green outline, no fill
//   style 1 = blocking (closed): dark red outline + diagonal stripe fill
//   style 2 = preview (while drawing): magenta outline, no fill
// Requires common.wgsl to be prepended (Uniforms `u`, aa(), inset_band()).

struct Inst {
    @location(0) p0: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) half_width: f32,
    @location(3) border: f32,
    @location(4) style: f32,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // Position in the valve's own frame: x along the valve from p0, y across it.
    @location(0) q: vec2<f32>,
    // (length, half width, border, style), constant per instance.
    @location(1) @interpolate(flat) shape: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let delta = inst.p1 - inst.p0;
    let len = length(delta);
    // Guard zero-length valves: fall back to the x axis.
    let dir = select(vec2<f32>(1.0, 0.0), delta / max(len, 1e-6), len > 1e-6);
    let nrm = vec2<f32>(-dir.y, dir.x);

    let pad = 2.0 * inst.border;   // room for the outline and the anti-aliased edge

    let c = quad_corner(vi);
    let along = mix(-pad, len + pad, c.x);
    let across = mix(-inst.half_width - pad, inst.half_width + pad, c.y);

    let world = inst.p0 + dir * along + nrm * across;

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.q = vec2<f32>(along, across);
    out.shape = vec4<f32>(len, inst.half_width, inst.border, inst.style);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let len = in.shape.x;
    let hw = max(in.shape.y, 1e-6);
    let border = in.shape.z;
    let style = in.shape.w;

    // Signed distance to the rectangle (negative inside).
    let half_extent = vec2<f32>(0.5 * len, hw);
    let p = vec2<f32>(in.q.x - 0.5 * len, in.q.y);
    let e = abs(p) - half_extent;
    let d = length(max(e, vec2<f32>(0.0))) + min(max(e.x, e.y), 0.0);

    // NOTE: everything that uses derivatives (aa, fwidth) is computed here,
    // before any branching, to keep WGSL uniform control flow valid.
    let inner = aa(d + border);         // inside the outline band
    let ring = inset_band(d, border);

    // Diagonal stripes, 45 degrees in the valve's own frame.
    let period = max(hw * 0.7, 1e-4);
    let s = (in.q.x - in.q.y) / period;
    let f = abs(fract(s) - 0.5);                  // 0 at stripe centre
    let w = min(fwidth(s), 0.25);
    let stripe = 1.0 - smoothstep(0.25 - w, 0.25 + w, f);

    let dark_red = vec3<f32>(0.55, 0.05, 0.05);
    let dark_green = vec3<f32>(0.0, 0.45, 0.12);
    let magenta = vec3<f32>(1.0, 0.0, 1.0);

    var rgb = dark_green;
    var alpha = ring;                              // passable: outline only
    if style > 1.5 {
        rgb = magenta;                             // preview
        alpha = ring * 0.8;
    } else if style > 0.5 {
        rgb = dark_red;                            // blocking: outline + stripes
        alpha = clamp(ring + inner * stripe, 0.0, 1.0);
    }
    return vec4<f32>(rgb, alpha);
}
