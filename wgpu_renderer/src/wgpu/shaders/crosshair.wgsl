// Crosshair shader for WGPU renderer
// Renders a simple crosshair at the cursor position
// Also can render a ball outline when placing a ball

struct Uniforms {
    // 4x4 view-projection matrix (world -> NDC)
    view_proj: mat4x4<f32>,
    // Cursor position in world coordinates
    cursor_pos: vec2<f32>,
    // Ball radius to show (0 = don't show ball outline)
    ball_radius: f32,
    // Show ball outline flag (1.0 = show, 0.0 = hide)
    show_ball_outline: f32,
    // Equilibrium radius for ball outline
    eq_radius: f32,
    // Cutoff radius for ball outline
    cutoff_radius: f32,
    // Padding for alignment
    _padding: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) @interpolate(flat) vertex_type: u32, // 0 = crosshair, 1 = ball outline
}

// Crosshair size in world units
const CROSSHAIR_SIZE: f32 = 1.0;
// Line thickness in world units
const LINE_THICKNESS: f32 = 0.1;
// Stroke thickness for circle outlines in world units
const STROKE_WIDTH: f32 = 0.15;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    // Instance 0: crosshair (4 line segments = 8 triangles = 24 vertices)
    // Instance 1: ball outline (if enabled)

    if instance_index == 0u {
        // Crosshair rendering - 4 quads for the cross
        out.vertex_type = 0u;

        var local_pos: vec2<f32>;
        let quad_idx = vertex_index / 6u;
        let vert_in_quad = vertex_index % 6u;

        // Define the 4 quads for the crosshair
        // Quad 0: horizontal line left
        // Quad 1: horizontal line right
        // Quad 2: vertical line top
        // Quad 3: vertical line bottom

        var quad_center: vec2<f32>;
        var quad_size: vec2<f32>;

        switch quad_idx {
            case 0u: { // Left horizontal
                quad_center = vec2<f32>(-CROSSHAIR_SIZE * 0.5, 0.0);
                quad_size = vec2<f32>(CROSSHAIR_SIZE * 0.4, LINE_THICKNESS);
            }
            case 1u: { // Right horizontal
                quad_center = vec2<f32>(CROSSHAIR_SIZE * 0.5, 0.0);
                quad_size = vec2<f32>(CROSSHAIR_SIZE * 0.4, LINE_THICKNESS);
            }
            case 2u: { // Top vertical
                quad_center = vec2<f32>(0.0, CROSSHAIR_SIZE * 0.5);
                quad_size = vec2<f32>(LINE_THICKNESS, CROSSHAIR_SIZE * 0.4);
            }
            case 3u: { // Bottom vertical
                quad_center = vec2<f32>(0.0, -CROSSHAIR_SIZE * 0.5);
                quad_size = vec2<f32>(LINE_THICKNESS, CROSSHAIR_SIZE * 0.4);
            }
            default: {
                quad_center = vec2<f32>(0.0, 0.0);
                quad_size = vec2<f32>(0.0, 0.0);
            }
        }

        // Generate quad vertices
        var offset: vec2<f32>;
        switch vert_in_quad {
            case 0u: { offset = vec2<f32>(-0.5, 0.5); }
            case 1u: { offset = vec2<f32>(-0.5, -0.5); }
            case 2u: { offset = vec2<f32>(0.5, 0.5); }
            case 3u: { offset = vec2<f32>(0.5, 0.5); }
            case 4u: { offset = vec2<f32>(-0.5, -0.5); }
            case 5u: { offset = vec2<f32>(0.5, -0.5); }
            default: { offset = vec2<f32>(0.0, 0.0); }
        }

        local_pos = quad_center + offset * quad_size;
        out.local_pos = local_pos;

        let world_pos = uniforms.cursor_pos + local_pos;
        out.position = uniforms.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    } else {
        // Ball outline rendering - circle at cursor
        out.vertex_type = 1u;

        // Generate a quad large enough for the cutoff circle
        var local_uv: vec2<f32>;
        switch vertex_index {
            case 0u: { local_uv = vec2<f32>(-1.0, 1.0); }
            case 1u: { local_uv = vec2<f32>(-1.0, -1.0); }
            case 2u: { local_uv = vec2<f32>(1.0, 1.0); }
            case 3u: { local_uv = vec2<f32>(1.0, 1.0); }
            case 4u: { local_uv = vec2<f32>(-1.0, -1.0); }
            case 5u: { local_uv = vec2<f32>(1.0, -1.0); }
            default: { local_uv = vec2<f32>(0.0, 0.0); }
        }

        // Expand by half-stroke for the same reason as ball.wgsl
        let quad_radius = uniforms.cutoff_radius + STROKE_WIDTH * 0.5;
        out.local_pos = local_uv * quad_radius;
        let world_pos = uniforms.cursor_pos + out.local_pos;
        out.position = uniforms.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    }

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.vertex_type == 0u {
        // Crosshair - solid white
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    } else {
        // Ball outline - draw circles
        if uniforms.show_ball_outline < 0.5 {
            return vec4<f32>(0.0, 0.0, 0.0, 0.0);
        }

        let dist = length(in.local_pos);

        // Colors
        let white = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        let magenta = vec4<f32>(1.0, 0.0, 1.0, 1.0);
        let gray = vec4<f32>(0.43, 0.43, 0.43, 1.0);

        // Cutoff circle (gray)
        let cutoff_inner = uniforms.cutoff_radius - STROKE_WIDTH * 0.5;
        let cutoff_outer = uniforms.cutoff_radius + STROKE_WIDTH * 0.5;
        if dist > cutoff_inner && dist < cutoff_outer {
            let edge = smoothstep(cutoff_inner, cutoff_inner + 0.05, dist)
                     * (1.0 - smoothstep(cutoff_outer - 0.05, cutoff_outer, dist));
            return vec4<f32>(gray.rgb, edge);
        }

        // Equilibrium circle (magenta)
        let eq_inner = uniforms.eq_radius - STROKE_WIDTH * 0.5;
        let eq_outer = uniforms.eq_radius + STROKE_WIDTH * 0.5;
        if dist > eq_inner && dist < eq_outer {
            let edge = smoothstep(eq_inner, eq_inner + 0.05, dist)
                     * (1.0 - smoothstep(eq_outer - 0.05, eq_outer, dist));
            return vec4<f32>(magenta.rgb, edge);
        }

        // Inner circle outline (white)
        let ball_inner = uniforms.ball_radius - STROKE_WIDTH * 0.5;
        let ball_outer = uniforms.ball_radius + STROKE_WIDTH * 0.5;
        if dist > ball_inner && dist < ball_outer {
            let edge = smoothstep(ball_inner, ball_inner + 0.05, dist)
                     * (1.0 - smoothstep(ball_outer - 0.05, ball_outer, dist));
            return vec4<f32>(white.rgb, edge);
        }

        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}
