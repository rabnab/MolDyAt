// Ball shader for WGPU renderer
// Renders balls as instanced quads with SDF circles for anti-aliased edges
// Uses storage buffer for ball data (position, radius, color)
// Draws three circles per ball:
//   1. Inner filled circle (ball radius) - ball color
//   2. Equilibrium outline (magenta) - where equal-size balls reach equilibrium
//   3. Cutoff outline (gray) - interaction range

// Per-ball data stored in a read-only storage buffer
struct BallData {
    // Center position in world coordinates (x, y)
    position: vec2<f32>,
    // Ball radius in world units
    radius: f32,
    // Packed RGBA color as u32 (0xAABBGGRR format)
    color: u32,
}

// Uniform buffer for viewport transformation
struct Uniforms {
    // 4x4 view-projection matrix (world -> NDC)
    view_proj: mat4x4<f32>,
    // Screen size for aspect ratio corrections (if needed)
    screen_size: vec2<f32>,
    // Equilibrium edge distance (from LJ potential, typically ~0.56 * lj_sig)
    eq_edge_distance: f32,
    // Cutoff distance for interactions (typically 4.0)
    cutoff_distance: f32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var<storage, read> balls: array<BallData>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    // UV coordinates for SDF circle (-1 to 1 range, scaled by outer radius)
    @location(0) uv: vec2<f32>,
    // Ball color passed to fragment shader
    @location(1) color: vec4<f32>,
    // Ball radius in world units (for inner circle)
    @location(2) ball_radius: f32,
    // Equilibrium radius in world units (for magenta outline)
    @location(3) eq_radius: f32,
    // Cutoff radius in world units (for gray outline)
    @location(4) cutoff_radius: f32,
}

// Unpack RGBA color from u32 (0xAABBGGRR format, little-endian)
fn unpack_color(packed: u32) -> vec4<f32> {
    let r = f32((packed >> 0u) & 0xFFu) / 255.0;
    let g = f32((packed >> 8u) & 0xFFu) / 255.0;
    let b = f32((packed >> 16u) & 0xFFu) / 255.0;
    let a = f32((packed >> 24u) & 0xFFu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    // Get ball data from storage buffer
    let ball = balls[instance_index];

    // Calculate the three radii for this ball
    let ball_radius = ball.radius;
    // Equilibrium radius: radius + half of equilibrium edge distance
    let eq_radius = ball_radius + uniforms.eq_edge_distance * 0.5;
    // Cutoff radius: equilibrium radius + cutoff distance
    let cutoff_radius = eq_radius + uniforms.cutoff_distance;

    // Generate quad vertices (6 vertices = 2 triangles)
    // The quad must be large enough to contain the cutoff circle
    var local_uv: vec2<f32>;
    switch vertex_index {
        case 0u: { local_uv = vec2<f32>(-1.0, 1.0); }   // top-left
        case 1u: { local_uv = vec2<f32>(-1.0, -1.0); }  // bottom-left
        case 2u: { local_uv = vec2<f32>(1.0, 1.0); }    // top-right
        case 3u: { local_uv = vec2<f32>(1.0, 1.0); }    // top-right
        case 4u: { local_uv = vec2<f32>(-1.0, -1.0); }  // bottom-left
        case 5u: { local_uv = vec2<f32>(1.0, -1.0); }   // bottom-right
        default: { local_uv = vec2<f32>(0.0, 0.0); }
    }

    // Calculate world position: center + offset by cutoff_radius (largest circle)
    let world_pos = ball.position + local_uv * cutoff_radius;

    // Transform to clip space
    out.position = uniforms.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    // UV is in range [-cutoff_radius, cutoff_radius], we pass it as world units
    out.uv = local_uv * cutoff_radius;
    out.color = unpack_color(ball.color);
    out.ball_radius = ball_radius;
    out.eq_radius = eq_radius;
    out.cutoff_radius = cutoff_radius;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Distance from center in world units
    let dist = length(in.uv);

    // Stroke thickness in world units (approximately 2 pixels worth)
    // This is a rough approximation; could be made more precise with screen_size
    let stroke_width = 0.15;

    // Discard pixels outside the cutoff circle
    if dist > in.cutoff_radius + stroke_width * 0.5 {
        discard;
    }

    // Colors for the circles
    let magenta = vec4<f32>(1.0, 0.0, 1.0, 1.0);
    let gray = vec4<f32>(0.43, 0.43, 0.43, 1.0); // RGB(110, 110, 110)

    // Check which region we're in (from outside to inside)
    // 1. Cutoff circle outline (gray)
    let cutoff_inner = in.cutoff_radius - stroke_width * 0.5;
    let cutoff_outer = in.cutoff_radius + stroke_width * 0.5;
    if dist > cutoff_inner && dist < cutoff_outer {
        let edge = smoothstep(cutoff_inner, cutoff_inner + 0.05, dist)
                 * (1.0 - smoothstep(cutoff_outer - 0.05, cutoff_outer, dist));
        return vec4<f32>(gray.rgb, edge);
    }

    // 2. Equilibrium circle outline (magenta)
    let eq_inner = in.eq_radius - stroke_width * 0.5;
    let eq_outer = in.eq_radius + stroke_width * 0.5;
    if dist > eq_inner && dist < eq_outer {
        let edge = smoothstep(eq_inner, eq_inner + 0.05, dist)
                 * (1.0 - smoothstep(eq_outer - 0.05, eq_outer, dist));
        return vec4<f32>(magenta.rgb, edge);
    }

    // 3. Inner filled circle (ball color)
    if dist < in.ball_radius {
        // Anti-aliased edge
        let alpha = 1.0 - smoothstep(in.ball_radius - 0.05, in.ball_radius, dist);
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }

    // Between circles - transparent
    discard;
}
