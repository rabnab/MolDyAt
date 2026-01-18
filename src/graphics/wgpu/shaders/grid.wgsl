// Grid shader for WGPU renderer
// Renders a procedural checkerboard grid using a full-screen quad

struct Uniforms {
    // Viewport transform (world -> NDC)
    view_proj: mat4x4<f32>,
    // Grid parameters
    grid_origin: vec2<f32>,      // Top-left corner in world coords
    grid_cell_size: vec2<f32>,   // Cell width/height in world coords
    grid_dims: vec2<u32>,        // (cols, rows)
    // Colors for alternating cells (RGBA)
    color_even: vec4<f32>,
    color_odd: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_pos: vec2<f32>,
}

// Full-screen quad vertices (two triangles)
// We use instance_index to determine which cell we're drawing
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    // Calculate which cell this instance represents
    let col = instance_index % uniforms.grid_dims.x;
    let row = instance_index / uniforms.grid_dims.x;

    // Calculate cell corners in world coordinates
    // Note: Y decreases as row increases (Y axis points upward)
    let cell_origin = vec2<f32>(
        uniforms.grid_origin.x + f32(col) * uniforms.grid_cell_size.x,
        uniforms.grid_origin.y - f32(row) * uniforms.grid_cell_size.y
    );

    // Quad vertices (counter-clockwise from top-left)
    // 0: top-left, 1: bottom-left, 2: top-right, 3: bottom-right
    // Triangle 1: 0, 1, 2
    // Triangle 2: 2, 1, 3
    var local_pos: vec2<f32>;
    switch vertex_index {
        case 0u: { local_pos = vec2<f32>(0.0, 0.0); }                                        // top-left
        case 1u: { local_pos = vec2<f32>(0.0, -uniforms.grid_cell_size.y); }                 // bottom-left
        case 2u: { local_pos = vec2<f32>(uniforms.grid_cell_size.x, 0.0); }                  // top-right
        case 3u: { local_pos = vec2<f32>(uniforms.grid_cell_size.x, 0.0); }                  // top-right
        case 4u: { local_pos = vec2<f32>(0.0, -uniforms.grid_cell_size.y); }                 // bottom-left
        case 5u: { local_pos = vec2<f32>(uniforms.grid_cell_size.x, -uniforms.grid_cell_size.y); } // bottom-right
        default: { local_pos = vec2<f32>(0.0, 0.0); }
    }

    let world_pos = cell_origin + local_pos;
    out.world_pos = world_pos;

    // Transform to clip space
    out.position = uniforms.view_proj * vec4<f32>(world_pos, 0.0, 1.0);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Determine which cell we're in based on world position
    let cell_x = floor((in.world_pos.x - uniforms.grid_origin.x) / uniforms.grid_cell_size.x);
    let cell_y = floor((uniforms.grid_origin.y - in.world_pos.y) / uniforms.grid_cell_size.y);

    // --- Outer border in world space (reddish) ---
    let grid_width_cells = f32(uniforms.grid_dims.x);
    let grid_height_cells = f32(uniforms.grid_dims.y);
    let min_cell = min(uniforms.grid_cell_size.x, uniforms.grid_cell_size.y);
    let border_thickness = 0.08 * min_cell; // world units
    let border_x = border_thickness / uniforms.grid_cell_size.x;
    let border_y = border_thickness / uniforms.grid_cell_size.y;

    // Distances from current point (in cell units) to each side
    let dist_left = (in.world_pos.x - uniforms.grid_origin.x) / uniforms.grid_cell_size.x;
    let dist_right = grid_width_cells - dist_left;
    let dist_top = (uniforms.grid_origin.y - in.world_pos.y) / uniforms.grid_cell_size.y;
    let dist_bottom = grid_height_cells - dist_top;

    if dist_left < border_x || dist_right < border_x || dist_top < border_y || dist_bottom < border_y {
        return vec4<f32>(0.9, 0.25, 0.25, 0.8); // reddish border
    }

    // Use (row + col) modulo palette size; handle negatives safely
    let sum = i32(cell_x) + i32(cell_y);
    let idx = (sum % 6 + 6) % 6;

    // Match ggez pastel palette (6 colors, faint alpha)
    switch idx {
        case 0: { return vec4<f32>(0.8, 0.8, 1.0, 0.15); } // faint blue
        case 1: { return vec4<f32>(0.8, 1.0, 0.8, 0.15); } // faint green
        case 2: { return vec4<f32>(1.0, 0.8, 0.8, 0.15); } // faint red
        case 3: { return vec4<f32>(1.0, 1.0, 0.8, 0.15); } // faint yellow
        case 4: { return vec4<f32>(0.8, 1.0, 1.0, 0.15); } // faint cyan
        default: { return vec4<f32>(1.0, 0.8, 1.0, 0.15); } // faint magenta (case 5)
    }
}
