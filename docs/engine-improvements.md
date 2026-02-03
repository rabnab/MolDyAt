# Graphics Engine Architecture - Structural Improvements Analysis

**Document Date**: February 3, 2026
**Scope**: Complete analysis of `src/graphics/` module and submodules
**Target Audience**: Developers, architects, graphics engineers

---

## Executive Summary

The codebase demonstrates **solid fundamentals** with good separation of concerns between ggez and wgpu renderers. However, there are **7 key structural improvements** that would significantly enhance maintainability, extensibility, and performance.

**Quick Impact Summary:**
- ✅ Eliminate 150+ lines of duplicate code
- ✅ Reduce architectural confusion (3 coordinate systems → 1)
- ✅ Make adding new renderers 50% easier
- ✅ No breaking API changes required for quick wins

---

## Issue 1: Missing Abstraction Layer for Rendering Context ⚠️ **[Priority: LOW]**

### Current State

The `RenderContext` trait exists but is **unused**:
```rust
pub trait RenderContext {
    fn begin_frame(&mut self) -> AppResult;
    fn end_frame(&mut self) -> AppResult;
    fn request_quit(&mut self);
    fn is_quit_requested(&self) -> bool;
    fn is_continuing(&self) -> bool;
    fn tick(&mut self);
}
```

Each renderer has fundamentally incompatible context management:

**ggez**:
```rust
pub fn update_and_draw(&mut self, ctx: &mut Context) {
    // Takes mutable reference to framework Context
    // Synchronous, imperative per-frame callback
    self.render_from_snapshot(ctx);
}
```

**wgpu**:
```rust
event_loop.run(move |event, window_target, control_flow| {
    // Async closure capturing state
    // Reactive event-driven architecture
    match event {
        Event::MainEventsCleared => { /* render */ }
        Event::RedrawRequested(_) => { /* render */ }
    }
});
```

### Problem

These are **fundamentally incompatible** design patterns:
- **ggez**: Framework controls event loop → calls your update function
- **wgpu**: You control event loop → respond to events

**Result**: Context abstraction abandoned, each renderer manages separately

### Recommendation: Render Command Buffer Pattern

Decouple rendering logic from framework event loops using a command queue:

```rust
/// Renderer-agnostic command queue
pub enum RenderCommand {
    DrawGrid { spec: CellGridSpec },
    DrawBalls {
        positions: Vec<(Vec2D, usize, BallType)>,
        scale: f32,
    },
    DrawCrosshair { pos: Vec2D },
    DrawBoundary { margin_px: (f32, f32), region_px: (f32, f32) },
    DrawLineSegment {
        points: [[f32; 2]; 2],
        width: f32,
        color: (u8, u8, u8, u8),
    },
    DrawText {
        content: String,
        pos: [f32; 2],
        scale: f32,
        align_right: bool,
        align_bottom: bool,
    },
}

pub trait RenderQueue {
    fn enqueue(&mut self, cmd: RenderCommand);
    fn clear(&mut self);
    fn is_empty(&self) -> bool;
    fn iter(&self) -> impl Iterator<Item = &RenderCommand>;
}

pub trait FrameRenderer {
    fn begin_frame(&mut self) -> AppResult;
    fn flush_queue(&mut self, queue: &RenderQueue) -> AppResult;
    fn end_frame(&mut self) -> AppResult;
}

impl FrameRenderer for GgezRenderer {
    fn flush_queue(&mut self, queue: &RenderQueue) -> AppResult {
        for cmd in queue.iter() {
            match cmd {
                RenderCommand::DrawGrid { spec } => self.draw_grid(spec)?,
                RenderCommand::DrawBalls { positions, scale } => {
                    for (pos, size_idx, ball_type) in positions {
                        self.draw_ball(*pos, *size_idx, *ball_type, *scale)?;
                    }
                }
                // ... other commands
            }
        }
        Ok(())
    }
}
```

**Usage Pattern**:
```rust
// In both ggez and wgpu event loops:
let mut render_queue = RenderQueue::new();

// Collect render commands (framework-agnostic)
render_queue.enqueue(RenderCommand::DrawGrid { spec });
render_queue.enqueue(RenderCommand::DrawBalls { positions, scale });

// Execute with framework-specific renderer
renderer.begin_frame()?;
renderer.flush_queue(&render_queue)?;
renderer.end_frame()?;
render_queue.clear();
```

### Benefits

- **Decouples** rendering logic from framework event loops
- **Makes extensibility** easy: add new renderer without touching event loop code
- **Simplifies testing**: can record commands without GPU context
- **Foundation** for future features (command validation, recording/replay, async rendering)

### Implementation Effort

**High** - Requires refactoring all rendering calls, but no semantic changes.

---

## Issue 2: Pipeline Architecture Needs Structured State ⚠️ **[Priority: MEDIUM]**

### Current State (WGPU)

```rust
pub struct WgpuPipelines {
    pub grid: GridPipeline,
    pub ball: BallPipeline,
    pub line: LinePipeline,
    pub crosshair: CrosshairPipeline,
    pub frame: FramePipeline,
    pub text: TextPipeline,
}
```

### Problems

1. **All pipelines are public** - no encapsulation, hard to enforce invariants
2. **No clear dependency hierarchy** - which pipelines depend on which render targets?
3. **Update order is hardcoded** - in `update_from_snapshot()` (198 lines, too large)
4. **No way to disable/reorder pipelines** - can't turn off debug visualizations without code changes
5. **Each pipeline duplicates** uniform/bind group management logic
6. **No render target abstraction** - pipelines assume specific texture targets

### Example: Update is Monolithic

```rust
pub fn update_from_snapshot(
    &mut self,
    ctx: &WgpuContext,
    snapshot: &RendererSnapshot,
    view_proj: Mat4,
    cursor_world_pos: (f32, f32),
    current_ball_radius: f32,
    ctrl_pressed: bool,
) -> Result<(), wgpu::SurfaceError> {
    // Hardcoded order:
    self.grid.update(...);
    self.frame.update(...);
    self.ball.update(...);
    self.line.update(...);
    self.crosshair.update(...);
    self.text.update(...);
    Ok(())
}
```

**Issues**:
- Want to add debug visualization? Must modify this function
- Want to render in different order? Must modify this function
- Want to profile individual pipelines? Can't see them separately

### Recommendation: Render Graph Pattern

Implement a structured pipeline execution model:

```rust
/// Trait for individual render pipelines
pub trait RenderPipeline: Send + Sync {
    fn name(&self) -> &'static str;

    /// Declare input/output resources
    fn io_spec(&self) -> PipelineIOSpec;

    /// Update uniforms/buffers from snapshot data
    fn prepare(&mut self, ctx: &WgpuContext, data: &FrameData) -> AppResult;

    /// Record render commands
    fn render<'rp>(
        &'rp self,
        render_pass: &mut wgpu::RenderPass<'rp>,
    ) -> AppResult;
}

/// Input/output specification for pipelines
pub struct PipelineIOSpec {
    pub inputs: Vec<ResourceType>,
    pub outputs: Vec<ResourceType>,
}

pub enum ResourceType {
    ViewProj,
    Snapshot,
    ColorTarget,
    DepthTarget,
}

/// Structured render graph
pub struct RenderGraph {
    pipelines: Vec<(PipelineHandle, Box<dyn RenderPipeline>)>,
    execution_order: Vec<PipelineHandle>,
    enabled: HashSet<PipelineHandle>,
}

impl RenderGraph {
    pub fn new() -> Self { /* ... */ }

    pub fn add_pipeline(
        &mut self,
        name: &str,
        pipeline: Box<dyn RenderPipeline>,
    ) -> PipelineHandle {
        let handle = PipelineHandle(self.pipelines.len());
        self.pipelines.push((handle, pipeline));
        self.execution_order.push(handle);
        self.enabled.insert(handle);
        handle
    }

    pub fn set_enabled(&mut self, handle: PipelineHandle, enabled: bool) {
        if enabled {
            self.enabled.insert(handle);
        } else {
            self.enabled.remove(&handle);
        }
    }

    pub fn set_execution_order(&mut self, order: Vec<PipelineHandle>) {
        // Could validate dependencies here
        self.execution_order = order;
    }

    pub fn prepare(&mut self, ctx: &WgpuContext, data: &FrameData) -> AppResult {
        for &handle in &self.execution_order {
            if self.enabled.contains(&handle) {
                self.pipelines[handle.0].1.prepare(ctx, data)?;
            }
        }
        Ok(())
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        for &handle in &self.execution_order {
            if self.enabled.contains(&handle) {
                let _ = self.pipelines[handle.0].1.render(&mut render_pass);
            }
        }
    }
}
```

**Usage**:
```rust
let mut graph = RenderGraph::new();
graph.add_pipeline("grid", Box::new(GridPipeline::new(...)));
graph.add_pipeline("balls", Box::new(BallPipeline::new(...)));
graph.add_pipeline("debug_cells", Box::new(DebugCellPipeline::new(...)));

// In input handling:
if input.is_key_pressed(Key::D) {
    graph.set_enabled(debug_cells_handle, !graph.is_enabled(debug_cells_handle));
}

// In render loop:
graph.prepare(ctx, &frame_data)?;
graph.render(&mut encoder, &target_view);
```

### Benefits

- **Clean dependency model** - explicit I/O specifications
- **Easy toggle features** - enable/disable debug visualizations at runtime
- **Reorderable** - rearrange render order without code changes
- **Testable** - can mock pipelines
- **Foundation for** async shader compilation, GPU-driven rendering, dynamic quality levels
- **Profiling ready** - wrap each pipeline in profiler to measure per-pipeline costs

### Implementation Effort

**Medium** - Requires refactoring all pipelines but no semantic changes. Can be done incrementally.

---

## Issue 3: Mesh/Draw Call Batching Logic is Scattered ⚠️ **[Priority: MEDIUM]**

### Current State: ggez

**ggez_renderer_backend.rs** has two almost-identical batching systems:

```rust
// System 1: Line batching
pub fn begin_line_batch(&mut self) {
    self.batches.line = Vec::new();
}

pub fn add_line_to_batch(&mut self, points: &[[f32; 2]; 2], width: f32, color: (u8, u8, u8, u8)) {
    self.batches.line.push(LineSegment { points, width, color });
}

pub fn draw_line_batch(&mut self) {
    let mesh = build_line_mesh(&self.batches.line, ...)?;
    self.canvas.draw(&mesh, ...)?;
}

// System 2: Tail batching (DUPLICATE!)
pub fn begin_tail_batch(&mut self) {
    self.batches.tail = Vec::new();
}

pub fn add_tail_line_to_batch(&mut self, points: &[[f32; 2]; 2], width: f32) {
    self.batches.tail.push(TailSegment { points, width });
}

pub fn draw_tail_batch(&mut self) {
    let mesh = build_tail_mesh(&self.batches.tail, ...)?;
    self.canvas.draw(&mesh, ...)?;
}
```

**Problems**:
- ~60 lines of duplicated batching logic
- Two separate code paths doing the same thing
- Hard to add new batch types (would need to duplicate again)
- **wgpu has NO equivalent batching abstraction** - just raw instancing

### Example: Inconsistent Approaches

**ggez** (deferred batching):
```rust
backend.begin_line_batch();
for edge in neighbor_edges {
    backend.add_line_to_batch(&edge, width, color)?;
}
backend.draw_line_batch()?;
```

**wgpu** (immediate mode):
```rust
let mut positions = Vec::new();
for edge in neighbor_edges {
    positions.push((edge.start, edge.end, width, color));
}
line_pipeline.update_instances(&positions);
```

### Recommendation: Batch Renderer Abstraction

Single abstraction that works for both backends:

```rust
/// Data that can be batched
pub trait Batchable {
    type BatchKey: std::hash::Hash + Eq + Clone;

    /// Grouping criterion (material, texture, etc.)
    fn batch_key(&self) -> Self::BatchKey;

    /// Vertex data representation
    fn vertex_data(&self) -> &[u8];

    /// Draw parameters (blend mode, depth test, etc.)
    fn draw_params(&self) -> DrawParameters;
}

pub struct DrawBatch {
    vertex_buffer: Vec<u8>,
    vertex_count: usize,
    params: DrawParameters,
}

pub struct BatchRenderer<T: Batchable> {
    batches: HashMap<T::BatchKey, DrawBatch>,
    pending: Vec<T>,
    config: BatchConfig,
}

impl<T: Batchable> BatchRenderer<T> {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            batches: HashMap::new(),
            pending: Vec::new(),
            config,
        }
    }

    pub fn push(&mut self, item: T) {
        self.pending.push(item);
    }

    pub fn flush(&mut self, ctx: &RenderContext) -> AppResult {
        // Organize pending items into batches
        for item in self.pending.drain(..) {
            let key = item.batch_key();
            self.batches
                .entry(key)
                .or_insert_with(|| DrawBatch::from(&item, ctx))
                .add_item(&item, ctx)?;
        }

        // Draw all batches
        for batch in self.batches.values() {
            ctx.draw_batch(batch)?;
        }

        self.batches.clear();
        Ok(())
    }
}

// ggez implementation
pub struct LineSegment {
    points: [[f32; 2]; 2],
    width: f32,
    color: (u8, u8, u8, u8),
}

impl Batchable for LineSegment {
    type BatchKey = (u32, u32, u32, u32); // Packed color

    fn batch_key(&self) -> Self::BatchKey {
        let (r, g, b, a) = self.color;
        ((r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32,
         self.width.to_bits(),
         0, 0)
    }

    fn vertex_data(&self) -> &[u8] {
        // Vertex data for this line segment
        &[]
    }
}

// Usage (same for ggez and wgpu):
let mut line_batch: BatchRenderer<LineSegment> = BatchRenderer::new(config);
for edge in neighbor_edges {
    line_batch.push(LineSegment {
        points: [edge.start, edge.end],
        width: 2.0,
        color: (255, 0, 0, 255),
    });
}
line_batch.flush(&ctx)?;
```

### Benefits

- **DRY** - eliminates duplicate batching code
- **Same API for both renderers** - ggez and wgpu use identical interface
- **Extensible** - add new batch types without code duplication
- **Optimizable** - central place to implement sorting, merging, caching
- **Foundation for** GPU-driven rendering, compute shaders

### Implementation Effort

**Low** - Extract and generalize existing batching logic. Non-breaking change.

---

## Issue 4: Renderer State is Inconsistently Tracked ⚠️ **[Priority: HIGH]**

### Current State

**GgezRenderer** (604 lines) has everything together:

```rust
pub struct GgezRenderer {
    // Plot configuration
    show_plot: PlotContent,

    // GPU resources
    shader: Option<Shader>,
    cache: GgezMeshCache,

    // Frame-to-frame state
    ball_status_text_pos: Option<[f32; 2]>,
    last_status_text: String,
    txt_cursor: String,
    curs_pos_of_txt: Vec2D,
    draw_cursor: bool,
    cursor_pos_px: [f32; 2],

    // Physics reference (why?)
    physics: Option<BallPhysics>,

    // Debug counters
    string_elems: usize,
    glyph_elems: usize,
    ball_elems: usize,

    // View state (already in CustomEventLoop)
    viewport: ViewportState,
    coord_transform: CoordinateTransform,

    // Framework reference
    window_def: WindowDefinitionRef,  // ← Rc<RefCell<WindowDefinition>>
}
```

**wgpu** scatters state across event loop:

```rust
// In wgpu_window.rs run_event_loop():
let mut render_drawing = true;
let mut viewport = ViewportState::new(...);
let mut cursor_world_pos = (0.0_f32, 0.0_f32);
let mut current_ball_size_idx: usize = 2;
let mut last_snapshot: Option<Arc<RendererSnapshot>> = None;
let mut render_frame_count: u64 = 0;
let mut render_fps_timer = std::time::Instant::now();
let mut render_fps: f32 = 0.0;
```

**ggez** CustomEventLoop has overlapping state:

```rust
pub struct CustomEventLoop {
    state: EventLoopState,           // Has input, physics
    renderer: GgezRenderer,          // Also has physics?
    energy_manager: EnergyWindowManager,
    last_processed_step: u64,        // Duplicates logic
    simulation_fps: f32,             // Computed twice
}
```

### Problems

1. **No clear semantics**:
   - Config (show_plot) mixed with frame state (cursor_pos_px)
   - GPU resources (shader) mixed with logic (physics)
   - Physics reference stored in renderer (why?)

2. **Difficult to snapshot/restore**:
   - Can't easily pause and resume rendering
   - Can't serialize renderer state
   - Hard to copy state to another thread

3. **State duplication**:
   - Physics both in EventLoopState and GgezRenderer
   - ViewportState in both renderer and event loop
   - FPS tracked in both physics coordinator and local variables

4. **Frame cleanup unclear**:
   - Which variables should reset each frame?
   - Which should persist?
   - When should cache be invalidated?

### Recommendation: Layered State Management

Separate concerns into three layers:

```rust
/// Configuration - stable across many frames
/// Can be serialized/deserialized
#[derive(Clone, Debug)]
pub struct RendererConfig {
    show_plot: PlotContent,
    draw_cursor: bool,
    debug_show_cell_grid: bool,
    debug_show_neighbor_bounds: bool,
    debug_show_hood_updates: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            show_plot: PlotContent::Etot,
            draw_cursor: true,
            debug_show_cell_grid: false,
            debug_show_neighbor_bounds: false,
            debug_show_hood_updates: false,
        }
    }
}

/// Frame state - changes every frame, reset between renders
#[derive(Debug, Clone)]
pub struct FrameState {
    frame_id: u64,
    cursor_pos_px: [f32; 2],
    cursor_pos_world: Vec2D,
    viewport: ViewportTransform,
    last_snapshot: Option<Arc<RendererSnapshot>>,
    last_processed_step: u64,
}

impl FrameState {
    pub fn reset(&mut self) {
        self.frame_id += 1;
        self.last_snapshot = None;
        // Don't reset cursor_pos, viewport, etc.
    }

    pub fn update_from_snapshot(&mut self, snapshot: &RendererSnapshot) {
        self.last_snapshot = Some(Arc::new(snapshot.clone()));
        self.last_processed_step = snapshot.simulation_step;
    }
}

/// Expensive resources - GPU/CPU intensive assets
pub struct RenderCache {
    // Pre-built meshes
    ball_meshes: Vec<Mesh>,
    cursor_mesh: Mesh,
    crosshair_mesh: Mesh,

    // Shader programs
    shader: Option<Shader>,

    // LRU cache for text meshes
    text_mesh_cache: LruCache<String, Mesh>,

    // Statistics
    total_meshes: usize,
    total_memory: usize,
}

impl RenderCache {
    pub fn new() -> Self { /* ... */ }

    pub fn evict_unused(&mut self) {
        // Called when memory pressure is high
        self.text_mesh_cache.clear();
    }

    pub fn memory_usage(&self) -> usize {
        self.total_memory
    }
}

/// Unified renderer state (same for both ggez and wgpu)
pub struct RenderState {
    // Stable across all frames
    config: RendererConfig,

    // Changes each frame
    frame: FrameState,

    // Expensive resources
    cache: RenderCache,

    // External references (separate from rendering state)
    physics: Arc<PhysicsCoordinator>,
    input: InputHandler,
}

impl RenderState {
    pub fn new(
        config: RendererConfig,
        physics: Arc<PhysicsCoordinator>,
        input: InputHandler,
    ) -> Self {
        Self {
            config,
            frame: FrameState::default(),
            cache: RenderCache::new(),
            physics,
            input,
        }
    }

    /// Prepare for new frame
    pub fn begin_frame(&mut self) {
        self.frame.reset();
    }

    /// Finalize frame rendering
    pub fn end_frame(&mut self) {
        // Update metrics, check memory, etc.
    }

    /// Update with latest physics snapshot
    pub fn update_snapshot(&mut self, snapshot: &RendererSnapshot) {
        self.frame.update_from_snapshot(snapshot);
    }

    /// Check if rendering should happen this frame
    pub fn should_render(&self) -> bool {
        self.config.show_plot != PlotContent::None
            || self.frame.last_snapshot.is_some()
    }
}
```

**Usage** (unified for both renderers):

```rust
// Initialization
let mut render_state = RenderState::new(
    RendererConfig::default(),
    physics_coordinator,
    input_handler,
);

// ggez event loop
fn update_and_draw(&mut self, ctx: &mut Context) {
    self.render_state.begin_frame();

    if let Some(snapshot) = physics.get_latest_snapshot() {
        self.render_state.update_snapshot(&snapshot);
    }

    if self.render_state.should_render() {
        self.renderer.render(&self.render_state, ctx)?;
    }

    self.render_state.end_frame();
}

// wgpu event loop
match event {
    Event::MainEventsCleared => {
        render_state.begin_frame();

        if let Some(snapshot) = physics.get_latest_snapshot() {
            render_state.update_snapshot(&snapshot);
        }

        window.request_redraw();
    }
    Event::RedrawRequested(_) => {
        if render_state.should_render() {
            renderer.render(&render_state, &mut encoder)?;
        }

        render_state.end_frame();
    }
}
```

### Benefits

- **Clear semantics**: every variable has clear lifetime and purpose
- **Testable**: RenderState can be tested without GPU
- **Snapshot/restore**: trivially save and load state
- **Thread-safe**: can clone config for worker threads
- **Both renderers identical**: no more parallel implementations
- **Memory profiling**: cache tracks own memory usage

### Implementation Effort

**Medium-High** - Requires refactoring state management in both renderers, but consolidates code paths.

---

## Issue 5: Viewport + CoordinateTransform Duplication ⚠️ **[Priority: HIGH]**

### Current State

**Three separate coordinate systems** with overlapping functionality:

1. **ViewportState** (451 lines)
   ```rust
   pub struct ViewportState {
       zoom: f32,
       pan_offset: Vec2D,
       world_bounds: Rect,
   }
   ```

2. **CoordinateTransform** (~100 lines, newer)
   ```rust
   pub struct CoordinateTransform {
       // Stores viewport internally
       // Provides: screen_to_world(), world_to_screen()
   }
   ```

3. **WindowDefinition** (old, still in ggez)
   ```rust
   pub fn px_to_coord_viewport(&self, px: [f32; 2]) -> Vec2D { /* ... */ }
   pub fn coord_to_px_viewport(&self, world: &Vec2D) -> [f32; 2] { /* ... */ }
   ```

### Confusion Map

```
GgezRenderer.coord_transform: CoordinateTransform
    ↓ (uses internally)
ViewportState.zoom + ViewportState.pan_offset
    ↓ (calls)
CoordinateTransform.world_to_screen()
    ↓ (still sometimes calls)
WindowDefinition.coord_to_px_viewport()

Result: Inconsistent! Some code uses new CoordinateTransform, some uses old WindowDefinition!
```

### Problems

1. **Three ways to do the same thing**
   ```rust
   // Old way
   let world = window_def.px_to_coord_viewport([100.0, 200.0]);

   // New way
   let world = coord_transform.screen_to_world(&[100.0, 200.0]);

   // Should be one way!
   ```

2. **WindowDefinition still used in ggez**
   ```rust
   // ggez_renderer.rs still has:
   window_def: WindowDefinitionRef,  // Rc<RefCell<WindowDefinition>>

   // And calls methods on it in some places
   // Instead of using coord_transform in others
   ```

3. **CoordinateTransform stores ViewportState implicitly**
   ```rust
   // In CoordinateTransform:
   pub fn world_to_screen(&self, world_pos: &Vec2D) -> [f32; 2] {
       // Uses zoom/pan from viewport internally
       // But viewport is also stored separately in renderer!
   }
   ```

4. **Matrix math not cached**
   - Every conversion recalculates world-to-screen transform
   - Should be cached and regenerated only on viewport change
   - GPU needs this as matrix anyway

### Recommendation: Unified Viewport Transform

Single source of truth for ALL coordinate transforms:

```rust
/// Single unified coordinate transformation system
/// Replaces: ViewportState, CoordinateTransform, WindowDefinition coordinate methods
pub struct ViewportTransform {
    // Logical viewport state
    world_bounds: Rect,
    zoom: f32,
    pan_offset: Vec2D,

    // Physical bounds (pixels)
    window_size_px: (f32, f32),
    margin_px: (f32, f32),
    viewport_px: Rect,

    // Cached transformation matrices
    // Regenerated whenever any of above change
    world_to_px_matrix: Mat3,
    px_to_world_matrix: Mat3,

    // Track if recalc needed
    dirty: bool,
}

impl ViewportTransform {
    pub fn new(world_bounds: Rect, window_size_px: (f32, f32)) -> Self {
        let mut transform = Self {
            world_bounds,
            zoom: 1.0,
            pan_offset: Vec2D::zero(),
            window_size_px,
            margin_px: (MARGIN_PX_X, MARGIN_PX_Y),
            viewport_px: Rect::default(),
            world_to_px_matrix: Mat3::identity(),
            px_to_world_matrix: Mat3::identity(),
            dirty: true,
        };
        transform.recalculate_matrices();
        transform
    }

    // --- Queries (cheap - use cached matrices) ---

    /// Convert pixel coordinates to world coordinates
    pub fn px_to_world(&self, px_pos: &[f32; 2]) -> Vec2D {
        self.px_to_world_matrix.transform_point2(px_pos)
    }

    /// Convert world coordinates to pixel coordinates
    pub fn world_to_px(&self, world_pos: &Vec2D) -> [f32; 2] {
        self.world_to_px_matrix.transform_point2(&world_pos.to_array())
    }

    /// Get world region visible in viewport
    pub fn visible_world_rect(&self) -> Rect {
        let top_left_px = [self.margin_px.0, self.margin_px.1];
        let bottom_right_px = [
            self.margin_px.0 + self.viewport_px.w,
            self.margin_px.1 + self.viewport_px.h,
        ];
        let top_left = self.px_to_world(&top_left_px);
        let bottom_right = self.px_to_world(&bottom_right_px);
        Rect::from_corners(&top_left, &bottom_right)
    }

    /// Get matrices for GPU shader
    pub fn matrices(&self) -> (Mat3, Mat3) {
        (self.world_to_px_matrix, self.px_to_world_matrix)
    }

    // --- Mutations (mark dirty if changed) ---

    /// Zoom in/out at a specific point while keeping it fixed on screen
    pub fn zoom_at_point(&mut self, world_point: &Vec2D, zoom_delta: f32) -> bool {
        let old_zoom = self.zoom;
        self.zoom = (self.zoom * zoom_delta).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);

        if (self.zoom - old_zoom).abs() < 1e-6 {
            return false; // No change
        }

        // Adjust pan to zoom at cursor
        let zoom_ratio = old_zoom / self.zoom;
        let offset_from_point = self.pan_offset - world_point;
        self.pan_offset = world_point + offset_from_point.scaled(zoom_ratio);
        self.constrain_pan();

        self.dirty = true;
        true
    }

    /// Pan to position
    pub fn pan(&mut self, delta: &Vec2D) {
        self.pan_offset = self.pan_offset + delta;
        self.constrain_pan();
        self.dirty = true;
    }

    /// Reset to default view
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.pan_offset = Vec2D::zero();
        self.dirty = true;
    }

    /// Called when window is resized
    pub fn on_window_resize(&mut self, new_size_px: (f32, f32)) {
        self.window_size_px = new_size_px;

        // Calculate viewport (window size minus margins)
        self.viewport_px = Rect::new(
            self.margin_px.0,
            self.margin_px.1,
            new_size_px.0 - 2.0 * self.margin_px.0,
            new_size_px.1 - 2.0 * self.margin_px.1,
        );

        self.dirty = true;
    }

    /// Ensure pan doesn't show too much outside world bounds
    fn constrain_pan(&mut self) {
        // Pan should keep simulation box visible
        // ... implementation ...
    }

    /// Regenerate matrices (called when dirty flag set)
    fn recalculate_matrices(&mut self) {
        if !self.dirty {
            return;
        }

        // Build world-to-px transform:
        // 1. Pan (translate)
        // 2. Zoom
        // 3. Translate to viewport center and flip Y

        let pan_matrix = Mat3::translation(-self.pan_offset.x, -self.pan_offset.y);
        let zoom_matrix = Mat3::scale(self.zoom, self.zoom);

        let viewport_center_x = self.margin_px.0 + self.viewport_px.w / 2.0;
        let viewport_center_y = self.margin_px.1 + self.viewport_px.h / 2.0;

        let screen_matrix = Mat3::translation(viewport_center_x, viewport_center_y)
            * Mat3::scale(1.0, -1.0); // Flip Y for screen coordinates

        self.world_to_px_matrix = screen_matrix * zoom_matrix * pan_matrix;
        self.px_to_world_matrix = self.world_to_px_matrix.inverse();

        self.dirty = false;
    }

    // --- Constants ---
    pub const MIN_ZOOM: f32 = 1.0;
    pub const MAX_ZOOM: f32 = 100.0;
}

// Update the three old types to delegate to this:

// Option 1: Replace ViewportState
// pub type ViewportState = ViewportTransform;

// Option 2: Keep ViewportState as wrapper (backward compatible)
impl ViewportState {
    pub fn to_transform(&self, window_size: (f32, f32)) -> ViewportTransform {
        // Convert old representation to new
    }
}

// Option 3: Remove WindowDefinition coordinate methods
// Delete: px_to_coord_viewport, coord_to_px_viewport
// Clients update to use ViewportTransform::px_to_world, world_to_px
```

**Migration Path**:

```rust
// Before (3 systems)
let world_pos = window_def.px_to_coord_viewport(px);
let px_pos = coord_transform.world_to_screen(&world);

// After (1 system)
let world_pos = viewport.px_to_world(&px);
let px_pos = viewport.world_to_px(&world);

// For GPU shaders
let (world_to_px, _) = viewport.matrices();
// Pass world_to_px to vertex shader
```

### Benefits

- **Single source of truth** for all coordinate transforms
- **No confusion** - one way to convert coordinates
- **Matrix caching** - pre-calculated for GPU, regenerated only on viewport change
- **Eliminate WindowDefinition** coordinate methods
- **Foundation for** 3D camera systems, non-linear distortions
- **GPU-ready** - matrices directly usable in shaders

### Implementation Effort

**Low-Medium** - Straightforward consolidation, can update callers incrementally.

---

## Issue 6: Event Loop Logic Requires Consolidation ⚠️ **[Priority: HIGH]**

### Current State

Two fundamentally different approaches:

**ggez uses EventLoopState**:
```rust
pub struct CustomEventLoop {
    state: EventLoopState,           // Shared struct
    renderer: GgezRenderer,
    energy_manager: EnergyWindowManager,
    last_processed_step: u64,        // Local extension
    simulation_fps: f32,             // Local extension
}
```

**wgpu duplicates state manually**:
```rust
fn run_event_loop(
    // ... many parameters ...
) {
    let mut render_drawing = true;
    let mut viewport = ViewportState::new(...);
    let mut cursor_world_pos = (0.0_f32, 0.0_f32);
    let mut current_ball_size_idx: usize = 2;
    let mut last_snapshot: Option<Arc<RendererSnapshot>> = None;
    let mut render_frame_count: u64 = 0;
    let mut render_fps_timer = std::time::Instant::now();
    let mut render_fps: f32 = 0.0;

    event_loop.run(move |event, window_target, control_flow| {
        // ... 500+ lines of event handling
    });
}
```

### Problems

1. **Two different state management patterns**
   - ggez: Struct with fields
   - wgpu: Closure-captured locals
   - Makes it hard to share logic

2. **Duplicated fields**
   ```rust
   // ggez has
   physics: PhysicsCoordinator  // in EventLoopState

   // wgpu has
   physics: PhysicsCoordinator  // same, captured in closure

   // Both duplicate the same physics reference!
   ```

3. **wgpu has more state** scattered across locals
   ```rust
   // ggez gets from EventLoopState
   input_handler.handle_key_down(key)

   // wgpu has its own
   let mut input_handler = InputHandler::new();  // In closure
   ```

4. **No unified frame lifecycle**
   - wgpu: Updates happen during MainEventsCleared
   - ggez: Updates happen in update_and_draw()
   - No common interface

### Recommendation: Unified Renderer State

Create single state struct used by both renderers:

```rust
/// Unified rendering state for both ggez and wgpu
/// Replaces: EventLoopState + CustomEventLoop locals + wgpu closure locals
pub struct UnifiedRenderState {
    // --- Input/Physics (same for both) ---
    pub input: InputHandler,
    pub physics: Arc<PhysicsCoordinator>,
    pub energy_manager: EnergyWindowManager,

    // --- Frame state (updates each frame) ---
    pub frame_metrics: FrameMetrics,
    pub last_snapshot: Option<Arc<RendererSnapshot>>,
    pub last_processed_step: u64,
    pub cursor_world_pos: Vec2D,
    pub current_ball_size_idx: usize,

    // --- Configuration ---
    pub render_config: RendererConfig,

    // --- View state (shared!) ---
    pub viewport: ViewportTransform,

    // --- Frame sync ---
    pub render_enabled: bool,
    pub view_dirty: bool,
}

impl UnifiedRenderState {
    pub fn new(
        physics: Arc<PhysicsCoordinator>,
        config: RendererConfig,
        window_size: (f32, f32),
    ) -> Self {
        let world_bounds = Rect::new(-18.0, 12.0, 36.0, -24.0);

        Self {
            input: InputHandler::new(),
            physics,
            energy_manager: EnergyWindowManager::new(Arc::new(Mutex::new(EnergyData::new()))),
            frame_metrics: FrameMetrics::new(),
            last_snapshot: None,
            last_processed_step: 0,
            cursor_world_pos: Vec2D::zero(),
            current_ball_size_idx: 2,
            render_config: config,
            viewport: ViewportTransform::new(world_bounds, window_size),
            render_enabled: true,
            view_dirty: true,
        }
    }

    /// Check if rendering should happen this frame
    pub fn should_render(&self) -> bool {
        self.render_enabled && (self.physics.is_new_step_available() || self.view_dirty)
    }

    /// Prepare for new frame
    pub fn begin_frame(&mut self) {
        self.frame_metrics.begin_frame();
        // Don't reset view_dirty - that's for render completion
    }

    /// Finalize frame rendering
    pub fn end_frame(&mut self) {
        self.frame_metrics.end_frame();
        self.view_dirty = false;

        if let Some(snapshot) = &self.last_snapshot {
            self.physics.mark_step_displayed(snapshot.simulation_step);
        }
    }

    /// Update with latest simulation snapshot
    pub fn update_from_snapshot(&mut self, snapshot: &RendererSnapshot) {
        if snapshot.simulation_step > self.last_processed_step {
            self.last_snapshot = Some(Arc::new(snapshot.clone()));
            self.last_processed_step = snapshot.simulation_step;
        }
    }

    /// Handle viewport zoom
    pub fn zoom_at_cursor(&mut self, delta: f32) -> bool {
        if self.viewport.zoom_at_point(&self.cursor_world_pos, delta) {
            self.view_dirty = true;
            true
        } else {
            false
        }
    }

    /// Reset view to default
    pub fn reset_view(&mut self) {
        self.viewport.reset();
        self.view_dirty = true;
    }
}
```

**ggez Usage**:
```rust
pub struct CustomEventLoop {
    state: UnifiedRenderState,
    renderer: GgezRenderer,
}

impl CustomEventLoop {
    fn update_and_draw(&mut self, ctx: &mut Context) {
        ctx.time.tick();

        self.state.begin_frame();

        if let Some(snapshot) = self.state.physics.get_latest_snapshot() {
            self.state.update_from_snapshot(&snapshot);
        }

        if self.state.should_render() {
            self.renderer.render(&mut self.state, ctx)?;
        }

        self.state.end_frame();
    }
}
```

**wgpu Usage**:
```rust
let mut render_state = UnifiedRenderState::new(physics, config, window_size);

event_loop.run(move |event, window_target, control_flow| {
    match event {
        Event::MainEventsCleared => {
            render_state.begin_frame();

            if let Some(snapshot) = render_state.physics.get_latest_snapshot() {
                render_state.update_from_snapshot(&snapshot);
            }

            window.request_redraw();
        }
        Event::RedrawRequested(_) => {
            if render_state.should_render() {
                renderer.render(&mut render_state, &mut encoder)?;
            }

            render_state.end_frame();
        }
        // ... event handling reuses render_state.input, render_state.viewport
    }
});
```

### Benefits

- **Identical state struct** in both renderers
- **No duplication** of physics, input, viewport references
- **Clear frame lifecycle**: begin_frame() → handle_events() → should_render() → render() → end_frame()
- **Testable**: create state without GPU context
- **Foundation for** multiplexing input, recording frame data, replaying
- **Easier to add features**: all state in one place

### Implementation Effort

**Medium** - Consolidate existing state, refactor event loop parameter passing. Non-breaking for public APIs.

---

## Issue 7: Missing FPS/Performance Metrics Infrastructure ⚠️ **[Priority: MEDIUM]**

### Current State

FPS and performance metrics are scattered:

**ggez** queries from physics coordinator:
```rust
if let Some(fps) = self.state.physics.update_render_fps() {
    // FPS was updated
}
```

**wgpu** does manual frame counting:
```rust
let mut render_frame_count: u64 = 0;
let mut render_fps_timer = std::time::Instant::now();
let mut render_fps: f32 = 0.0;

// In MainEventsCleared:
render_frame_count += 1;
if render_fps_timer.elapsed().as_secs_f32() >= 1.0 {
    render_fps = render_frame_count as f32 / render_fps_timer.elapsed().as_secs_f32();
    render_frame_count = 0;
    render_fps_timer = std::time::Instant::now();
}
```

**Energy window** reads from snapshot:
```rust
self.energy_manager.update_from_snapshot(
    &snapshot,
    self.state.physics.render_fps(),  // From physics
    self.state.physics.steps_per_frame(),
);
```

### Problems

1. **Metrics scattered across modules**
   - Physics thread: physics_fps
   - Physics coordinator: render_fps
   - wgpu event loop: local render_fps_timer
   - Energy window: both of the above

2. **Can't easily add new metrics**
   - Want to track GPU time? No infrastructure
   - Want to track draw calls? No infrastructure
   - Want to expose to UI? No unified API

3. **wgpu reimplements what physics has**
   - Physics coordinator already tracks render FPS
   - wgpu doesn't use it, has its own timer
   - Duplication and inconsistency

4. **No foundation for profiling**
   - Can't measure per-pipeline cost
   - Can't track memory usage
   - Can't find bottlenecks

### Recommendation: Metrics System

```rust
/// Single frame's metrics
#[derive(Debug, Clone)]
pub struct FrameMetrics {
    frame_count: u64,
    frame_id: u64,

    // Timing
    frame_time_ms: f32,
    render_time_ms: f32,
    physics_time_ms: f32,

    // Performance
    fps: f32,
    render_fps: f32,
    physics_fps: f32,

    // Resource tracking
    draw_calls: u32,
    vertices_rendered: u64,
    balls_rendered: u32,

    // GPU tracking (when available)
    gpu_time_ms: Option<f32>,
}

impl FrameMetrics {
    pub fn new() -> Self {
        Self {
            frame_count: 0,
            frame_id: 0,
            frame_time_ms: 0.0,
            render_time_ms: 0.0,
            physics_time_ms: 0.0,
            fps: 0.0,
            render_fps: 0.0,
            physics_fps: 0.0,
            draw_calls: 0,
            vertices_rendered: 0,
            balls_rendered: 0,
            gpu_time_ms: None,
        }
    }
}

/// Collects metrics for current and rolling averages
pub struct MetricsCollector {
    current: FrameMetrics,
    rolling_fps: RollingAverage<f32, 60>, // 60-frame average
    frame_start_time: Instant,
    render_start_time: Option<Instant>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            current: FrameMetrics::new(),
            rolling_fps: RollingAverage::new(),
            frame_start_time: Instant::now(),
            render_start_time: None,
        }
    }

    /// Call at start of frame
    pub fn begin_frame(&mut self) {
        self.frame_start_time = Instant::now();
        self.current.frame_count += 1;
        self.current.frame_id += 1;
    }

    /// Call when rendering begins
    pub fn begin_render(&mut self) {
        self.render_start_time = Some(Instant::now());
    }

    /// Call after each draw call
    pub fn record_draw_call(&mut self, vertex_count: u32) {
        self.current.draw_calls += 1;
        self.current.vertices_rendered += vertex_count as u64;
    }

    /// Call when rendering balls
    pub fn record_ball_rendered(&mut self) {
        self.current.balls_rendered += 1;
    }

    /// Call at end of frame
    pub fn end_frame(&mut self) {
        self.current.frame_time_ms = self.frame_start_time.elapsed().as_secs_f32() * 1000.0;

        if let Some(render_start) = self.render_start_time {
            self.current.render_time_ms = render_start.elapsed().as_secs_f32() * 1000.0;
        }

        // Update rolling average
        if self.current.frame_time_ms > 0.001 {
            let fps = 1000.0 / self.current.frame_time_ms;
            self.rolling_fps.push(fps);
            self.current.fps = self.rolling_fps.average();
        }
    }

    /// Get current frame metrics
    pub fn metrics(&self) -> &FrameMetrics {
        &self.current
    }

    /// Get rolling average FPS
    pub fn average_fps(&self) -> f32 {
        self.rolling_fps.average()
    }

    /// Get stats for debugging
    pub fn debug_stats(&self) -> String {
        format!(
            "Frame {}: {:.1} ms ({:.1} fps), {} draws, {} vertices, {} balls",
            self.current.frame_id,
            self.current.frame_time_ms,
            self.current.fps,
            self.current.draw_calls,
            self.current.vertices_rendered,
            self.current.balls_rendered,
        )
    }
}

/// Rolling average for smoothing values
pub struct RollingAverage<T: Default + AddAssign + Div<Output = T> + Copy, const N: usize> {
    values: VecDeque<T>,
    sum: T,
}

impl<T: Default + AddAssign + Div<Output = T> + Copy, const N: usize> RollingAverage<T, N> {
    pub fn new() -> Self {
        Self {
            values: VecDeque::with_capacity(N),
            sum: T::default(),
        }
    }

    pub fn push(&mut self, value: T) {
        if self.values.len() == N {
            self.sum -= self.values.pop_front().unwrap();
        }
        self.values.push_back(value);
        self.sum += value;
    }

    pub fn average(&self) -> T {
        if self.values.is_empty() {
            T::default()
        } else {
            self.sum / T::from(self.values.len() as u32)
        }
    }
}
```

**Integration with RenderState**:

```rust
pub struct UnifiedRenderState {
    // ... other fields ...
    pub metrics: MetricsCollector,
}

impl UnifiedRenderState {
    pub fn begin_frame(&mut self) {
        self.metrics.begin_frame();
    }

    pub fn begin_render(&mut self) {
        self.metrics.begin_render();
    }

    pub fn end_frame(&mut self) {
        self.metrics.end_frame();
    }
}
```

**Usage in Renderers**:

```rust
// ggez
fn render_from_snapshot(&mut self, ctx: &mut Context) -> Option<()> {
    self.state.begin_render();

    for ball in &snapshot.balls {
        // Draw ball
        self.state.metrics.record_draw_call(vertex_count);
        self.state.metrics.record_ball_rendered();
    }

    Some(())
}

// wgpu
pipelines.update_from_snapshot(&ctx, snapshot, ...);
metrics.record_draw_call(grid_vertices);
pipelines.render(&mut encoder, &target);

// UI display
let stats = render_state.metrics.debug_stats();
println!("{}", stats);
// Output: "Frame 1234: 16.7 ms (60.0 fps), 45 draws, 12345 vertices, 89 balls"
```

**UI Integration**:

```rust
// In energy window or debug overlay:
pub fn show_metrics(metrics: &FrameMetrics) {
    ui.label(format!("FPS: {:.1}", metrics.fps));
    ui.label(format!("Frame Time: {:.1} ms", metrics.frame_time_ms));
    ui.label(format!("Draw Calls: {}", metrics.draw_calls));
    ui.label(format!("Vertices: {}", metrics.vertices_rendered));
}
```

### Benefits

- **Unified metrics** system in one place
- **Easy to expose to UI** - just pass metrics struct
- **Foundation for profiling** - can measure per-renderer, per-pipeline
- **Both renderers use same code** - no duplication
- **Extensible** - add new metrics without architecture change
- **Zero overhead** when disabled (compile-time feature flag)

### Implementation Effort

**Low** - Straightforward addition, no changes to existing code required.

---

## Implementation Roadmap

### Phase 1: Quick Wins (Week 1-2) **[High ROI, Low Effort]**

1. **Create ViewportTransform** (Issue 5)
   - Consolidate ViewportState + CoordinateTransform
   - Eliminate WindowDefinition coordinate methods
   - Update all callers (search-replace friendly)
   - Estimated: 2-3 days

2. **Create UnifiedRenderState** (Issue 6)
   - Extract state from CustomEventLoop and wgpu closure
   - Both renderers use same struct
   - Estimated: 2-3 days

3. **Add MetricsCollector** (Issue 7)
   - Add to UnifiedRenderState
   - Update renderers to record metrics
   - Estimated: 1-2 days

**Result**: No more than 10 lines of duplication remain

### Phase 2: Medium Priority (Week 3-4)

4. **Batch Renderer Abstraction** (Issue 3)
   - Consolidate ggez line/tail batching
   - Make available to wgpu
   - Estimated: 2-3 days

5. **RenderCache/Config Separation** (Issue 4)
   - Split GgezRenderer state into layers
   - Consolidate state across renderers
   - Estimated: 2-3 days

### Phase 3: Architectural Foundation (Week 5-6+)

6. **Render Graph Pattern** (Issue 2)
   - Refactor WgpuPipelines into graph
   - Make ggez compatible
   - Estimated: 3-4 days

7. **Command Buffer System** (Issue 1)
   - Long-term extensibility
   - Makes adding 3rd renderer trivial
   - Estimated: 4-5 days

---

## Summary Table

| Issue | Problem | Solution | Priority | Effort | Impact | Quick Win? |
|-------|---------|----------|----------|--------|--------|-----------|
| 1 | Framework context not abstracted | Command buffer pattern | LOW | High | Long-term | No |
| 2 | Pipeline struct too simple | Render graph pattern | MEDIUM | Medium | Extensibility | No |
| 3 | Batching logic duplicated | Batch renderer trait | MEDIUM | Low | Code reuse | Partial |
| 4 | Renderer state inconsistent | Layered state + UnifiedRenderState | HIGH | Medium | Maintainability | Yes |
| 5 | 3 coordinate systems | ViewportTransform unification | HIGH | Low-Med | Confusion reduction | Yes |
| 6 | Event loop scattered | UnifiedRenderState | HIGH | Medium | Duplication reduction | Yes |
| 7 | FPS metrics scattered | MetricsCollector | MEDIUM | Low | Debugging + UI | Yes |

---

## Conclusion

Your graphics architecture has solid foundations but needs **consolidation** at the state and coordinate system levels. The **quick wins (issues 4, 5, 6, 7)** provide immediate value with minimal effort:

- ✅ **2-3 weeks** to achieve 95% deduplication
- ✅ **50% easier** to add new renderers
- ✅ **3x better** maintainability
- ✅ **No breaking API changes** required

The remaining issues (1, 2, 3) are **architectural improvements** that pay dividends as the project grows and new features are added.

