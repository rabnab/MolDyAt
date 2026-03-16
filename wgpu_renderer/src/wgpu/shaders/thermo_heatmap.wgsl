// Thermo heatmap overlay shader
//
// Renders per-cell coloured quads from a storage buffer of RGBA values.
// Each cell is an instanced quad; the fragment shader reads the colour
// from the storage buffer using the flat instance index.

struct Uniforms {
    view_proj: mat4x4<f32>,
    grid_origin: vec2<f32>,
    grid_cell_size: vec2<f32>,
    grid_dims: vec2<u32>,
    _padding: vec2<u32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var<storage, read> cell_colors: array<vec4<f32>>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) cell_index: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let col = instance_index % uniforms.grid_dims.x;
    let row = instance_index / uniforms.grid_dims.x;

    // Cell origin is bottom-left corner in world space (Y-up; origin_y is min-y of the grid).
    // Row 0 is the bottom row; row index increases upward.
    let cell_origin = vec2<f32>(
        uniforms.grid_origin.x + f32(col) * uniforms.grid_cell_size.x,
        uniforms.grid_origin.y + f32(row) * uniforms.grid_cell_size.y
    );

    var local_pos: vec2<f32>;
    switch vertex_index {
        case 0u: { local_pos = vec2<f32>(0.0, 0.0); }
        case 1u: { local_pos = vec2<f32>(0.0, uniforms.grid_cell_size.y); }
        case 2u: { local_pos = vec2<f32>(uniforms.grid_cell_size.x, 0.0); }
        case 3u: { local_pos = vec2<f32>(uniforms.grid_cell_size.x, 0.0); }
        case 4u: { local_pos = vec2<f32>(0.0, uniforms.grid_cell_size.y); }
        case 5u: { local_pos = vec2<f32>(uniforms.grid_cell_size.x, uniforms.grid_cell_size.y); }
        default: { local_pos = vec2<f32>(0.0, 0.0); }
    }

    let world_pos = cell_origin + local_pos;
    out.position = uniforms.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.cell_index = instance_index;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return cell_colors[in.cell_index];
}
