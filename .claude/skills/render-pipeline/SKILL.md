---
name: render-pipeline
description: Contract for adding or changing a wgpu render pipeline, WGSL shader, GpuBatch use, draw order, or the snapshot data a pipeline reads. Use before editing wgpu_renderer/src/wgpu/render/ (pipelines/, gpu/batch.rs, gpu/shaders.rs, shaders/*.wgsl, wgpu_pipelines.rs, render_graph.rs, frame.rs), or adding a field to RendererSnapshot for rendering.
---

# Render Pipeline Contract

Every visual layer is one `RenderPipeline` (`wgpu_renderer/src/wgpu/render/render_graph.rs`). `docs/architecture.md` (Render pipelines) describes the pattern; this skill lists the rules.

## Where the code goes (#372)
The backend is two modules, shell → render. Pipelines, `gpu/`, shaders, the render graph, `WgpuContext`, surfaces and the frame recording (`render/frame.rs`) live in `wgpu_renderer/src/wgpu/render/`, which must not use winit: `tests/render_guard.rs` fails on a `winit` outside comments there. Anything that needs a window (sizes, redraw requests, events, creating windows) belongs in `wgpu/shell/`; the renderer reaches a window only through `RenderWindow` (`render/surface_window.rs`) or values the shell passes in.

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
- `new(device, queue, format)` (a text pipeline: `new(device, queue, format, glyphs)`, see Text below) builds a `GpuBatch<T>` from a `BatchDesc` (`BatchDesc::quads(...)` for instanced quads, `BatchDesc::vertices(...)` for vertex lists, optional `.texture(view, filter)`).
- `prepare()` clears a reused `Vec<T>` field (no per-frame allocation), fills it with `#[repr(C)] #[derive(Pod, Zeroable)]` instances or vertices, calls `gpu.set_uniforms(...)` and `gpu.flush(device, queue, &instances)`.
- `render()` only calls `gpu.draw(render_pass)`.
- `gpu/batch.rs` (`GpuBatch`) is the only place that creates wgpu pipelines, bind groups and buffers. Do not call `device.create_*` in a pipeline.
- Text: a pipeline that draws text takes the renderer's one `GlyphAtlas` (`gpu/text.rs`, printable ASCII, built once in `WgpuContext`, #303) as a fourth `new` argument and builds a `TextBatch` from it (one batch per colour); `WgpuPipelines::new` passes it on. Lay labels out with `push_label_vertices` (a space is a blank glyph quad, so label backgrounds span the whole text; non-ASCII characters emit no quad). Never build a glyph texture of your own.

## Shaders
- Add the file to `wgpu_renderer/src/wgpu/render/shaders/` and register it in the `shaders!` macro in `gpu/shaders.rs`; pipelines take shader source only from there. `common.wgsl` is prepended automatically: it provides the uniforms `u.view_proj` / `u.params`, `quad_corner()`, `aa()` and `ring_band()`. Do not redeclare them.
- Document what `u.params` holds at the top of the shader. An optional texture lives at `@group(1)`.
- The vertex attribute array in Rust must match the WGSL `Inst`/vertex struct field by field (types and order).

## Space and layers
- World-space pipelines use `ViewportMode::Inset` and `ctx.view_proj`. Screen-space layers (sidebar, text, labels, frame) build NDC positions and use `SceneUniforms::ndc(...)` with the identity `view_proj`.
- Sizes that should look the same at every zoom are derived from the world extent (`gpu/geometry.rs`), not hard-coded in world units.
- Return the right `layer()`: `BALLS`, `THERMO`, `READOUTS` or `BASE`. Pipelines outside the frame's layers are skipped; a pipeline that mixes layers checks `ctx.layers` for its own parts. Operators, previews, sidebar and text are `BASE` and must always draw (#216).
- Data comes only from `PrepareContext`: the snapshot's sections (`frame`, `balls`, `operators`, `fields`, `debug`; see `docs/architecture.md#snapshot-flow`), `operator_previews`, `cursor_world_pos`, margins, `thermo_display`. Read only the sections the pipeline draws. New simulation data goes into the section it belongs to, or a new section, never a new top-level field of `RendererSnapshot` (built in `crates/thermo_runtime/src/application/snapshot.rs` by `fill_snapshot`, which refills a reused snapshot: clear and refill its vectors, do not replace them). Costly data goes behind a `SceneLayers` flag (describe a new layer in `LAYERS`) so the physics thread builds it only when drawn; debug data only for verbose balls, in `DebugSnapshot`.

## Registration and draw order
- Register in `WgpuPipelines::new` (`render/wgpu_pipelines.rs`). Registration order is draw order: background (sidebar, frame, grid, thermo) → walls and operators → balls → cursor and labels → text. Store a handle in `PipelineHandles` only if other code must reach the pipeline (for example a key that changes its mode, handled in `shell/wgpu_input.rs`).
- Export the module in `render/pipelines/mod.rs`.

## Tests
- `render/gpu/tests/shaders.rs` validates every registered WGSL with naga (runs everywhere, no GPU needed).
- `wgpu_renderer/src/wgpu/tests/gpu_smoke.rs` builds every pipeline on a real device and records a populated frame with the main window's `encode_frame` into an offscreen target; it is skipped without a GPU adapter, so a green CI run does not prove it ran. Run it locally on a machine with a GPU after pipeline changes.
- Unit-test geometry builders and instance generation without a device (see `tests/highlight_pipeline.rs`, `tests/sidebar_pipeline.rs`, `tests/thermostat_label_pipeline.rs`).
- Commands: `cargo test -p moldyat_wgpu`.

## The energy plot window
`PlotPipeline` (`render/pipelines/plot_pipeline.rs`) is the one pipeline outside the render graph: it draws a `PlotScene` from `thermo_ui` (window pixels, laid out and unit-tested there) into the plot window's own surface (`render/plot_surface.rs`, kept per window by an opaque id; the shell owns the OS window). It still builds only `GpuBatch`es and takes its shaders (`plot_segment.wgsl`, `plot_text.wgsl`) from `gpu/shaders.rs`. A change to what the plot shows belongs in `PlotScene::build`, not in the pipeline.

## Related skills
Operator visuals: `operator-framework`. Sidebar icons and atlas: `operator-framework` (Sidebar And Tool Order).
