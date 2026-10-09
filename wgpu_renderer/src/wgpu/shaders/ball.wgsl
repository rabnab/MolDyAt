// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Ball shader: one instance = one ball.
// Draws up to three circles per ball, from the outside in:
//   1. cutoff outline (gray)       - interaction range, ring mode 1 only
//   2. equilibrium outline (magenta) - where equal-size balls reach equilibrium,
//                                    hidden in ring mode 2
//   3. filled ball, shaded as a sphere
//
// u.params = (equilibrium edge distance, cutoff distance, ring mode, unused)
//   ring mode: 0 = equilibrium only, 1 = equilibrium + cutoff, 2 = no rings

// Stroke thickness of the outlines in world units (~2 px at default zoom).
const STROKE_WIDTH: f32 = 0.15;

struct Inst {
    @location(0) center: vec2<f32>,
    @location(1) radius: f32,
    // Packed RGBA, 0xAABBGGRR
    @location(2) color: u32,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // Offset from the ball centre (world units)
    @location(0) q: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    // (ball radius, equilibrium radius, cutoff radius, unused)
    @location(2) @interpolate(flat) radii: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let eq_radius = inst.radius + u.params.x * 0.5;
    let cutoff_radius = eq_radius + u.params.y;

    // Half a stroke of extra room so the outer ring is never clipped by the
    // quad edge (that caused holes and flicker at the quadrant points).
    let reach = cutoff_radius + STROKE_WIDTH * 0.5;
    let q = (quad_corner(vi) * 2.0 - 1.0) * reach;

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(inst.center + q, 0.0, 1.0);
    out.q = q;
    out.color = unpack4x8unorm(inst.color);
    out.radii = vec4<f32>(inst.radius, eq_radius, cutoff_radius, 0.0);
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let dist = length(in.q);
    let ring_mode = u32(u.params.z + 0.5);

    if ring_mode == 1u {
        let a = ring_band(dist, in.radii.z, STROKE_WIDTH);
        if a > 0.0 {
            return vec4<f32>(0.43, 0.43, 0.43, a);
        }
    }

    if ring_mode != 2u {
        let a = ring_band(dist, in.radii.y, STROKE_WIDTH);
        if a > 0.0 {
            return vec4<f32>(1.0, 0.0, 1.0, a);
        }
    }

    let radius = in.radii.x;
    if dist < radius + 0.05 {
        let alpha = 1.0 - smoothstep(radius - 0.05, radius, dist);

        // Normal of a sphere seen from above; clamped so sqrt stays real.
        let p = in.q / radius;
        let z = sqrt(1.0 - min(dot(p, p), 1.0));
        let normal = normalize(vec3<f32>(p, z));

        let light_dir = normalize(vec3<f32>(-0.4, 0.4, 0.8));
        let diffuse = max(dot(normal, light_dir), 0.0);
        let half_vec = normalize(light_dir + vec3<f32>(0.0, 0.0, 1.0));
        let specular = pow(max(dot(normal, half_vec), 0.0), 32.0);

        let shaded = in.color.rgb * (0.35 + diffuse * 0.65) + vec3<f32>(specular * 0.5);
        return vec4<f32>(shaded, in.color.a * alpha);
    }

    return vec4<f32>(0.0);
}
