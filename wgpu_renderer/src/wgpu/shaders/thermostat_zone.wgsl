// Thermostat zone shader: one instance = one thermostat's range of influence.
//   active   -> semi-transparent colour that pulses over time
//   inactive -> constant semi-transparent colour
// Colour follows the target temperature (blue = cold, red = hot).
// Requires common.wgsl to be prepended (Uniforms `u`, aa()).
//
// u.params = (time in seconds, unused, unused, unused)

const TAU: f32 = 6.28318530718;

// Tuning
const PULSE_HZ: f32 = 0.8;           // pulses per second when active
const ALPHA_PULSE_MIN: f32 = 0.10;   // fill alpha at the dimmest point of the pulse
const ALPHA_PULSE_MAX: f32 = 0.40;   // fill alpha at the brightest point of the pulse
const ALPHA_INACTIVE: f32 = 0.20;    // constant fill alpha when inactive
const RING_ALPHA: f32 = 0.6;         // alpha of the thin outline that marks the range

struct Inst {
    @location(0) center: vec2<f32>,
    @location(1) radius: f32,
    @location(2) temp: f32,     // target temperature, 0..1
    @location(3) is_on: f32,    // 1.0 = active, 0.0 = inactive
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) q: vec2<f32>,                          // offset from the centre (world units)
    @location(1) @interpolate(flat) shape: vec4<f32>,   // (radius, temp, is_on, unused)
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let reach = inst.radius * 1.1;   // room for the anti-aliased edge

    let q = (quad_corner(vi) * 2.0 - 1.0) * reach;

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + q, 0.0, 1.0);
    out.q = q;
    out.shape = vec4<f32>(inst.radius, inst.temp, inst.is_on, 0.0);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let radius = max(in.shape.x, 1e-6);
    let temp = in.shape.y;
    let is_on = in.shape.z;

    let r = length(in.q);
    let d = r - radius;                       // signed distance to the disc edge

    // Derivative-based values first, no branching (keeps uniform control flow).
    let px = fwidth(d);
    let inside = aa(d);
    let ring = max(inside - aa(d + 2.5 * px), 0.0);   // ~2.5 px outline just inside the edge

    // Pulse (0..1) and the resulting fill alpha, blended by `is_on` without branching.
    let pulse = 0.5 + 0.5 * sin(u.params.x * TAU * PULSE_HZ);
    let alpha_active = mix(ALPHA_PULSE_MIN, ALPHA_PULSE_MAX, pulse);
    let fill_alpha = mix(ALPHA_INACTIVE, alpha_active, is_on);

    // Slightly stronger towards the centre.
    let falloff = 1.0 - 0.5 * smoothstep(0.0, 1.0, r / radius);

    let rgb = vec3<f32>(temp, 0.2, 1.0 - temp);
    let alpha = clamp(inside * fill_alpha * falloff + ring * RING_ALPHA, 0.0, 1.0);
    return vec4<f32>(rgb, alpha);
}
