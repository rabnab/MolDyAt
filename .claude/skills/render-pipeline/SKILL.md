---
name: render-pipeline
description: Contract for adding or changing a wgpu render pipeline, WGSL shader, GpuBatch use, draw order, or the snapshot data a pipeline reads. Use before editing wgpu_renderer/src/wgpu/pipelines/, gpu/batch.rs, gpu/shaders.rs, shaders/*.wgsl, wgpu_pipelines.rs or render_graph.rs, or adding a field to RendererSnapshot for rendering.
---

# Render Pipeline Contract

Every visual layer is one `RenderPipeline` (`wgpu_renderer/src/wgpu/render_graph.rs`). `docs/architecture.md` (Render pipelines) describes the pattern; this skill lists the rules.

## The pattern
```rust
pub trait RenderPipeline: Any {
    fn name(&self) -> &'static str;
    fn viewport_mode(&self) -> ViewportMode;          // FullWindow or Inset
    fn layer(&self) -> SceneLayers { SceneLayers::BASE }
    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult;
    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>);
}
```
- `new(device, queue, format)` builds a `GpuBatch<T>` from a `BatchDesc` (`BatchDesc::quads(...)` for instanced quads, `BatchDesc::vertices(...)` for vertex lists, optional `.texture(view, filter)`).
- `prepare()` clears a reused `Vec<T>` field (no per-frame allocation), fills it with `#[repr(C)] #[derive(Pod, Zeroable)]` instances or vertices, calls `gpu.set_uniforms(...)` and `gpu.flush(device, queue, &instances)`.
- `render()` only calls `gpu.draw(render_pass)`.
- `gpu/batch.rs` (`GpuBatch`) is the only place that creates wgpu pipelines, bind groups and buffers. Do not call `device.create_*` in a pipeline.

## Shaders
- Add the file to `wgpu_renderer/src/wgpu/shaders/` and register it in the `shaders!` macro in `gpu/shaders.rs`; pipelines take shader source only from there. `common.wgsl` is prepended automatically: it provides the uniforms `u.view_proj` / `u.params`, `quad_corner()`, `aa()` and `ring_band()`. Do not redeclare them.
- Document what `u.params` holds at the top of the shader. An optional texture lives at `@group(1)`.
- The vertex attribute array in Rust must match the WGSL `Inst`/vertex struct field by field (types and order).

## Space and layers
- World-space pipelines use `ViewportMode::Inset` and `ctx.view_proj`. Screen-space layers (sidebar, text, labels, frame) build NDC positions and use `SceneUniforms::ndc(...)` with the identity `view_proj`.
- Sizes that should look the same at every zoom are derived from the world extent (`gpu/geometry.rs`), not hard-coded in world units.
- Return the right `layer()`: `BALLS`, `THERMO`, `READOUTS` or `BASE`. Pipelines outside the frame's layers are skipped; a pipeline that mixes layers checks `ctx.layers` for its own parts. Operators, previews, sidebar and text are `BASE` and must always draw (#216).
- Data comes only from `PrepareContext` (`snapshot`, `operator_previews`, `cursor_world_pos`, margins, `thermo_display`). New simulation data goes into `RendererSnapshot` (built in `crates/thermo_runtime/src/application/snapshot.rs`) and, if it is costly, behind a `SceneLayers` flag so the physics thread builds it only when drawn.

## Registration and draw order
- Register in `WgpuPipelines::new` (`wgpu_pipelines.rs`). Registration order is draw order: background (sidebar, frame, grid, thermo) → walls and operators → balls → cursor and labels → text. Store a handle in `PipelineHandles` only if other code must reach the pipeline (for example a key that changes its mode, handled in `wgpu_input.rs`).
- Export the module in `pipelines/mod.rs`.

## Tests
- `gpu/tests/shaders.rs` validates every registered WGSL with naga (runs everywhere, no GPU needed).
- `wgpu_renderer/src/wgpu/tests/gpu_smoke.rs` builds every pipeline on a real device and renders a populated frame; it is skipped without a GPU adapter, so a green CI run does not prove it ran. Run it locally on a machine with a GPU after pipeline changes.
- Unit-test geometry builders and instance generation without a device (see `tests/highlight_pipeline.rs`, `tests/sidebar_pipeline.rs`, `tests/thermostat_label_pipeline.rs`).
- Commands: `cargo test -p moldyat_wgpu`, then `cargo fmt` and `cargo clippy --workspace -- -D warnings`. New `.rs` and `.wgsl` files start with the MPL-2.0 header.

## The energy plot window
`PlotPipeline` (`pipelines/plot_pipeline.rs`) is the one pipeline outside the render graph: it draws a `PlotScene` from `thermo_ui` (window pixels, laid out and unit-tested there) into the plot window's own surface (`plot_surface.rs`). It still builds only `GpuBatch`es and takes its shaders (`plot_segment.wgsl`, `plot_text.wgsl`) from `gpu/shaders.rs`. A change to what the plot shows belongs in `PlotScene::build`, not in the pipeline.

## Related skills
Operator visuals: `operator-framework`. Sidebar icons and atlas: `operator-framework` (Sidebar And Tool Order).
