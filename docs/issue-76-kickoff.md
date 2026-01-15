# Issue #76 Kickoff: Basic Grid Rendering

This branch implements issue #76 and targets PR #83 (wgpu_renderer).

- Parent PR: #83 (Implement WGPU renderer backend)
- Dependency: #75 (WGPU Context Setup) - already merged into PR #83
- Scope: Port background grid rendering to WGPU with procedural shader

Tasks:
1. Create Grid Pipeline (RenderPipeline)
2. WGSL shader for procedural grid
3. Uniform buffers for viewport transform
4. Single draw call for grid (instanced quads)

Success Criteria:
- Grid renders with alternating cell colors
- Grid respects viewport pan/zoom
- `cargo clippy -- -D warnings` passes
- `cargo fmt` applied
