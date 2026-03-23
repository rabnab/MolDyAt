// Thermo heatmap overlay shader
//
// Renders a single quad covering the simulation world area.  The raw scalar
// field (temperature or pressure, NaN-filled and normalised to [0,1]) is
// stored in a 1-channel R8Unorm texture with one texel per cell.  The GPU
// samples it with a linear (bilinear) filter, so the overlay appears smooth
// and continuous rather than blocky.
//
// Convention:
//   • grid_origin is the world-space bottom-left corner (min_x, min_y).
//   • grid_size   is (total_world_width, total_world_height), both positive.
//   • UV (0,0) = bottom-left of grid; UV (1,1) = top-right.
//   • Texture row 0 is written from ThermoGrid row 0 (= world bottom), so
//     texture V matches UV.y directly — no flip required.

struct Uniforms {
    view_proj:   mat4x4<f32>,
    grid_origin: vec2<f32>,
    grid_size:   vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var field_texture: texture_2d<f32>;
@group(0) @binding(2) var field_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

// Six vertices for two triangles spanning the grid world area.
// UV (0,0) is bottom-left, (1,1) is top-right.
var<private> UV_TABLE: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
    vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let uv = UV_TABLE[vi];
    let world_pos = uniforms.grid_origin + uv * uniforms.grid_size;
    var out: VertexOutput;
    out.position = uniforms.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.uv = uv;
    return out;
}

// Rainbow colormap: blue (t=0) → cyan → green → yellow → red (t=1).
fn rainbow(t: f32) -> vec3<f32> {
    let s = clamp(t, 0.0, 1.0) * 4.0;
    let i = floor(s);
    let f = s - i;
    var r = 0.0; var g = 0.0; var b = 0.0;
    if i < 1.0 {         // blue → cyan
        r = 0.0; g = f;   b = 1.0;
    } else if i < 2.0 {  // cyan → green
        r = 0.0; g = 1.0; b = 1.0 - f;
    } else if i < 3.0 {  // green → yellow
        r = f;   g = 1.0; b = 0.0;
    } else {             // yellow → red
        r = 1.0; g = 1.0 - f; b = 0.0;
    }
    return vec3<f32>(r, g, b);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // R8Unorm stores 0 as "no data" sentinel and 1..255 (→ 1/255..1) as data.
    let raw = textureSample(field_texture, field_sampler, in.uv).r;
    if raw < 0.004 {  // below 1/255 → NaN sentinel cell
        discard;
    }
    // Remap 1/255..1 back to 0..1 for the colormap.
    let t = (raw - 1.0 / 255.0) * (255.0 / 254.0);
    return vec4<f32>(rainbow(clamp(t, 0.0, 1.0)), 0.55);
}
