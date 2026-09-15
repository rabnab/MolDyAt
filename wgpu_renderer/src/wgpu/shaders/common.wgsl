// Shared header for all instanced object shaders.
// Prepended to each object's shader at pipeline creation (WGSL has no #include).
// Must match `SceneUniforms` in instanced_gpu.rs.

struct Uniforms {
    view_proj: mat4x4<f32>,
    params: vec4<f32>,   // x = time (seconds)
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

// Anti-aliased coverage from a signed distance `d` in world units
// (negative inside the shape). fwidth(d) is roughly one pixel in world units,
// so edges stay ~1 px soft at any zoom. Fragment stage only.
fn aa(d: f32) -> f32 {
    let w = max(fwidth(d), 1e-6);
    return 1.0 - smoothstep(-w, w, d);
}
