// Highlight shader: one instance = a glowing outline just outside a capsule
// (a segment p0..p1 inflated by `radius`; p0 == p1 gives a circle). Drawn
// around the object the active tool has under the cursor.
// Requires common.wgsl to be prepended (Uniforms `u`, inset_band()).

const HIGHLIGHT_RGB: vec3<f32> = vec3<f32>(1.0, 0.85, 0.2);
const BAND_ALPHA: f32 = 0.95;
const GLOW_ALPHA: f32 = 0.35;
const GLOW_REACH: f32 = 3.0;   // glow fades out over this many band widths

struct Inst {
    @location(0) p0: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) radius: f32,
    @location(3) width: f32,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // Position in the capsule's own frame: x along it from p0, y across it.
    @location(0) q: vec2<f32>,
    // (length, radius, band width, unused), constant per instance.
    @location(1) @interpolate(flat) shape: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let delta = inst.p1 - inst.p0;
    let len = length(delta);
    let dir = select(vec2<f32>(1.0, 0.0), delta / max(len, 1e-6), len > 1e-6);
    let nrm = vec2<f32>(-dir.y, dir.x);

    let reach = inst.radius + inst.width * (GLOW_REACH + 1.0);

    let c = quad_corner(vi);
    let along = mix(-reach, len + reach, c.x);
    let across = mix(-reach, reach, c.y);

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.p0 + dir * along + nrm * across, 0.0, 1.0);
    out.q = vec2<f32>(along, across);
    out.shape = vec4<f32>(len, inst.radius, inst.width, 0.0);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let len = in.shape.x;
    let width = max(in.shape.z, 1e-6);

    // Signed distance to the capsule surface (negative inside).
    let nearest = vec2<f32>(clamp(in.q.x, 0.0, len), 0.0);
    let d = length(in.q - nearest) - in.shape.y;

    // Solid band hugging the outside of the shape, plus a soft outer glow.
    let band = inset_band(d - width, width);
    let glow = (1.0 - smoothstep(0.0, GLOW_REACH * width, d)) * step(0.0, d);

    return vec4<f32>(HIGHLIGHT_RGB, max(band * BAND_ALPHA, glow * GLOW_ALPHA));
}
