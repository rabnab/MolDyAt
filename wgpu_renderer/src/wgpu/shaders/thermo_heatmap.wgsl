// Thermo heatmap overlay shader
//
// Renders a single quad covering the simulation world area.  The scalar field
// (temperature or pressure, NaN-filled, smoothed and normalised to [0,1]) is
// stored in a 1-channel R8Unorm texture with one texel per cell (0..255 maps
// to 0..1; there is no "no data" sentinel any more).
//
// The field is reconstructed with a cubic B-spline filter instead of plain
// bilinear filtering. It is C2-continuous, so a coarse grid looks like a smooth
// surface rather than a mosaic, and unlike Catmull-Rom it never overshoots the
// data range, so it can't create ringing artefacts in a noisy field. It costs
// four bilinear taps per pixel (Sigg & Hadwiger, GPU Gems 2, ch. 20).
//
// Convention:
//   * grid_origin is the world-space bottom-left corner (min_x, min_y).
//   * grid_size   is (total_world_width, total_world_height), both positive.
//   * UV (0,0) = bottom-left of grid; UV (1,1) = top-right.
//   * Texture row 0 is written from ThermoGrid row 0 (= world bottom), so
//     texture V matches UV.y directly, no flip required.

const OVERLAY_ALPHA: f32 = 0.55;

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

// Rainbow colormap: blue (t=0) -> cyan -> green -> yellow -> red (t=1).
fn rainbow(t: f32) -> vec3<f32> {
    let s = clamp(t, 0.0, 1.0) * 4.0;
    // Cap the segment index at 3: at t = 1.0 exactly, floor(s) would be 4, which
    // wrapped back to yellow (and values a hair below 1.0 stayed red).
    let i = min(floor(s), 3.0);
    let f = s - i;
    var r = 0.0; var g = 0.0; var b = 0.0;
    if i < 1.0 {         // blue -> cyan
        r = 0.0; g = f;   b = 1.0;
    } else if i < 2.0 {  // cyan -> green
        r = 0.0; g = 1.0; b = 1.0 - f;
    } else if i < 3.0 {  // green -> yellow
        r = f;   g = 1.0; b = 0.0;
    } else {             // yellow -> red
        r = 1.0; g = 1.0 - f; b = 0.0;
    }
    return vec3<f32>(r, g, b);
}

// Cubic B-spline weights for the four texels around a sample point,
// where `f` in [0,1) is the fractional position between the middle two.
fn bspline_weights(f: f32) -> vec4<f32> {
    let f2 = f * f;
    let f3 = f2 * f;
    return vec4<f32>(
        (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0,
        (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0,
        (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0,
        f3 / 6.0,
    );
}

// Bicubic B-spline reconstruction of the scalar field using 4 bilinear taps.
fn sample_field(uv: vec2<f32>) -> f32 {
    let dims = vec2<f32>(textureDimensions(field_texture));

    // Texel space with texel centres at integer coordinates.
    let p = uv * dims - 0.5;
    let base = floor(p);
    let f = p - base;

    let wx = bspline_weights(f.x);
    let wy = bspline_weights(f.y);

    // Merge each pair of neighbouring weights into one bilinear tap.
    let gx0 = wx.x + wx.y;
    let gx1 = wx.z + wx.w;
    let gy0 = wy.x + wy.y;
    let gy1 = wy.z + wy.w;
    let hx0 = wx.y / gx0;
    let hx1 = wx.w / gx1;
    let hy0 = wy.y / gy0;
    let hy1 = wy.w / gy1;

    // Tap positions back in UV space (+0.5 = texel centre).
    let x0 = (base.x - 1.0 + hx0 + 0.5) / dims.x;
    let x1 = (base.x + 1.0 + hx1 + 0.5) / dims.x;
    let y0 = (base.y - 1.0 + hy0 + 0.5) / dims.y;
    let y1 = (base.y + 1.0 + hy1 + 0.5) / dims.y;

    let t00 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(x0, y0), 0.0).r;
    let t10 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(x1, y0), 0.0).r;
    let t01 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(x0, y1), 0.0).r;
    let t11 = textureSampleLevel(field_texture, field_sampler, vec2<f32>(x1, y1), 0.0).r;

    return (t00 * gx0 + t10 * gx1) * gy0 + (t01 * gx0 + t11 * gx1) * gy1;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = sample_field(in.uv);
    return vec4<f32>(rainbow(t), OVERLAY_ALPHA);
}
