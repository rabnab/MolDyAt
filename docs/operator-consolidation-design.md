# Operator Handling Consolidation — Design Proposal

## Problem Statement

Adding the valve operator required touching ~17 files across 5 architecture layers.
Much of this was boilerplate that will repeat identically for thermostat and source
sphere. The current design forces **shotgun surgery**: a change to one concept
(e.g. "add a new operator type") fans out across many files that all need
synchronized edits.

### Files touched per new operator (current)

| Layer | Files | Purpose |
|---|---|---|
| Data model | `objects2d/*.rs`, `mod.rs` | Struct + export |
| MainContent | `core/mod.rs`, `ctor.rs` | Fields + init |
| Business logic | `core/<operator>.rs` | CRUD methods |
| Lifecycle | `core/operators.rs` | Manual clear/rescale wiring |
| Physics commands | `physics_thread.rs` | Enum variants + dispatch |
| Input actions | `input_handler.rs` | Enum variants |
| Action processor | `action_processor.rs` | Action→command mapping |
| Snapshot | `renderer_snapshot.rs`, `snapshot.rs` | Field + population |
| Render context | `render_graph.rs`, `wgpu_pipelines.rs`, `wgpu_window.rs` | Preview data threading |
| Rendering | `line_pipeline.rs` or new pipeline | Draw code |
| Tool interaction | `unified_render_state.rs`, tool module | State machine + mouse handling |

Of these, roughly **8 files are pure plumbing** — they just forward data without
adding logic. The remaining files contain genuine new behaviour.

---

## Design Goals

1. **Adding a new operator should not require changes to the command pipeline.**
   New operator logic should be self-contained; the pipeline discovers it, not
   the other way around.

2. **Operator state on MainContent should be registry-based, not field-based.**
   No more `Vec<T>` + `next_id: usize` pairs and manual ctor/clear/rescale wiring
   per type.

3. **Snapshot population and rendering data should flow automatically**
   from registered operators without touching `RendererSnapshot` constructors or
   `PrepareContext` for each new type.

4. **Preserve thread safety.** The physics thread owns `MainContent`; the render
   thread reads immutable snapshots. This boundary must remain clean.

5. **Keep it simple.** Avoid trait-object indirection where static dispatch
   suffices. Prefer enums when the set of operator types is known at compile time.

---

## Proposed Architecture

### Phase 1 — Operator Command Trait (eliminates pipeline boilerplate)

**Current:**
```
InputAction::CommitValveSegment → action_processor → PhysicsCommand::AddValveSegment → execute_command → main_content.add_valve_segment()
```
Four files touched for one action. Each new action requires adding an enum variant
in `InputAction`, a variant in `PhysicsCommand`, a match arm in `action_processor`,
and a match arm in `execute_command`.

**Proposed:**
```rust
// core/src/application/operator_command.rs

/// A self-contained operator command that knows how to apply itself.
pub trait OperatorCommand: Send + std::fmt::Debug {
    fn execute(self: Box<Self>, content: &mut MainContent);
}
```

`PhysicsCommand` gains a single variant:
```rust
enum PhysicsCommand {
    // ... existing non-operator commands unchanged ...
    Operator(Box<dyn OperatorCommand>),
}
```

`InputAction` gains a single variant:
```rust
enum InputAction {
    // ... existing non-operator actions unchanged ...
    OperatorAction(Box<dyn OperatorCommand>),
}
```

`action_processor` gets one match arm:
```rust
InputAction::OperatorAction(cmd) => {
    physics.send_command(PhysicsCommand::Operator(cmd));
}
```

`execute_command` gets one match arm:
```rust
PhysicsCommand::Operator(cmd) => {
    cmd.execute(main_content);
}
```

New operator actions just define a struct + `impl OperatorCommand`. No pipeline
files change. The existing `CommitLineSegment`, `MoveLineSegmentEndpoint`,
`CommitValveSegment`, `ToggleValve`, and `MoveValveSegment` actions all migrate to
concrete structs implementing this trait.

**Migration note:** Existing non-operator commands (`TogglePause`, `AddBall`, etc.)
stay as enum variants. Only operator-specific actions use the trait pattern. This
avoids a large migration and keeps the non-operator path zero-cost.

### Phase 2 — Operator Registry on MainContent (eliminates field sprawl)

**Current:** Each operator adds 2+ fields to `MainContent`:
```rust
pub(crate) line_segments: Vec<SimLineSegment>,
pub(crate) line_segment_next_id: usize,
pub(crate) valve_segments: Vec<ValveSegment>,
pub(crate) valve_segment_next_id: usize,
// future: thermostats, thermostat_next_id, sources, source_next_id ...
```

Plus manual wiring in `ctor.rs`, `operators.rs` (clear/rescale), and `snapshot.rs`.

**Proposed:**
```rust
// core/src/objects2d/operator.rs (extend existing module)

/// Trait for a collection of operators of one type.
pub trait OperatorCollection: Send {
    /// Remove all operators and reset ID counters.
    fn clear(&mut self);

    /// Adjust operator positions for a world resize.
    fn rescale(&mut self, old_rect: &Rect, new_rect: &Rect);

    /// Append rendering data to the operator snapshot.
    fn snapshot_into(&self, snapshot: &mut OperatorSnapshot);

    /// Downcast support for type-specific access.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
```

`MainContent` holds:
```rust
pub(crate) operators: Vec<Box<dyn OperatorCollection>>,
```

With a typed accessor:
```rust
impl MainContent {
    pub(crate) fn get_operators<T: OperatorCollection + 'static>(&self) -> Option<&T> {
        self.operators.iter()
            .find_map(|op| op.as_any().downcast_ref::<T>())
    }

    pub(crate) fn get_operators_mut<T: OperatorCollection + 'static>(&mut self) -> Option<&mut T> {
        self.operators.iter_mut()
            .find_map(|op| op.as_any_mut().downcast_mut::<T>())
    }
}
```

Concrete collections:
```rust
pub struct LineSegmentCollection {
    pub segments: Vec<SimLineSegment>,
    next_id: usize,
}

pub struct ValveCollection {
    pub valves: Vec<ValveSegment>,
    next_id: usize,
}
```

Both implement `OperatorCollection`. `clear_all_operators()` and
`rescale_all_operators()` become:
```rust
fn clear_all_operators(&mut self) {
    for op in &mut self.operators {
        op.clear();
    }
}
```

No per-type wiring in `operators.rs`. Adding a thermostat means adding a
`ThermostatCollection` struct — not editing `MainContent`, `ctor.rs`, or
`operators.rs`.

### Phase 3 — Consolidated Operator Snapshot (eliminates snapshot sprawl)

**Current:** Each operator adds a field to `RendererSnapshot`:
```rust
pub line_segments: Vec<SimLineSegment>,
pub valve_segments: Vec<ValveSegment>,
// future: pub thermostats: Vec<Thermostat>, ...
```

Plus initialization in two constructors and population in `create_snapshot()`.

**Proposed:** A single aggregate:
```rust
// core/src/application/renderer_snapshot.rs

/// All operator data needed for rendering, in one struct.
#[derive(Clone, Default)]
pub struct OperatorSnapshot {
    pub line_segments: Vec<SimLineSegment>,
    pub valve_segments: Vec<ValveSegment>,
    // future types added here only
}
```

`RendererSnapshot` holds one field:
```rust
pub operators: OperatorSnapshot,
```

`create_snapshot()` populates it via the registry:
```rust
let mut op_snap = OperatorSnapshot::default();
for op in &self.operators {
    op.snapshot_into(&mut op_snap);
}
// ...
RendererSnapshot { operators: op_snap, .. }
```

Rendering code accesses `ctx.snapshot.operators.line_segments` instead of
`ctx.snapshot.line_segments`. One struct to extend, one initialisation site, one
population loop.

### Phase 4 — Preview Data via Render State (eliminates PrepareContext growth)

**Current:** Each tool's preview data is threaded through:
`UnifiedRenderState` → `wgpu_window.rs` → `prepare_from_snapshot()` → `PrepareContext` → pipeline

**Proposed:** Previews are rendered by querying `UnifiedRenderState` directly during
`prepare()`, rather than threading each preview as a separate parameter.

Option A — **Preview collector on render state:**
```rust
impl UnifiedRenderState {
    /// All operator preview geometry for the current frame.
    pub fn operator_previews(&self) -> OperatorPreviews {
        OperatorPreviews {
            line_segment: self.line_segment_preview(),
            valve_segment: self.valve_segment_preview(),
            snap_indicators: self.snap_indicators(),
        }
    }
}
```

`PrepareContext` holds one field:
```rust
pub operator_previews: OperatorPreviews,
```

Adding a new tool's preview just extends `OperatorPreviews` — one struct, one
call site, no signature changes.

Option B — **Move preview geometry into `OperatorSnapshot`** by having the render
state inject preview segments as "uncommitted" entries with a `preview: bool` flag.
This is simpler but mixes committed and uncommitted state.

**Recommendation:** Option A — it keeps committed and preview data separate.

---

## Migration Strategy

The phases are ordered by independence and impact:

| Phase | Dependencies | Files eliminated per new operator | Risk |
|---|---|---|---|
| 1. Command trait | None | 3 (InputAction, PhysicsCommand, action_processor, execute_command) | Low — additive change |
| 3. Operator snapshot | None | 3 (RendererSnapshot struct + 2 ctors, snapshot.rs) | Low — mechanical rename |
| 2. Operator registry | Phase 3 recommended first | 3 (MainContent fields, ctor.rs, operators.rs) | Medium — changes MainContent access patterns |
| 4. Preview consolidation | Phase 3 | 3 (PrepareContext, wgpu_pipelines, wgpu_window) | Medium — touches render pipeline |

Each phase can be merged independently and validated with the existing test suite.
After all four, adding a new operator type requires:

1. **Data struct** in `objects2d/` (unavoidable)
2. **Collection + OperatorCommand impls** (self-contained)
3. **Tool interaction module** (self-contained)
4. **Rendering code** in pipeline (unavoidable)

Down from ~17 files to ~4, all containing genuine new logic rather than plumbing.

---

## What NOT to consolidate

- **Tool state machines** (`LineSegmentDrawState`, `ValveDrawState`): These are
  genuinely distinct interaction models. A generic "operator tool" trait would add
  complexity without reducing code — each tool has unique mouse semantics.

- **Collision resolution**: Line segment collisions involve physics-specific math
  (nearest-point projection, velocity reflection). This is domain logic, not
  framework plumbing, and shouldn't be abstracted behind a generic interface.

- **Sidebar icon mapping**: The 4-location synchronisation (sidebar.rs, input_handler,
  sidebar_tool, sidebar_pipeline) is annoying but small. A registry-driven sidebar
  is possible but the effort doesn't pay off for 4 icons.

---

## Open Questions

1. **Enum vs. trait for OperatorCommand?** An enum (`OperatorCmd::AddValve { .. }`)
   avoids `Box<dyn>` allocation but re-introduces exhaustive matching. For the
   expected ~10 operator commands, the enum is likely fine and avoids heap allocation
   in the command channel hot path. The trait approach is shown above for generality;
   the final choice should be profiled.

2. **Should `OperatorCollection::snapshot_into` use a visitor pattern instead?**
   Having each collection push into a shared `OperatorSnapshot` means the snapshot
   struct still needs to know all types. An alternative is returning
   `Box<dyn Any>` and downcasting in the renderer — but this loses type safety.
   The current proposal favours type safety at the cost of one struct edit per type.

3. **Cross-operator interactions** (valve depends on line segment parent): The
   registry approach still needs typed access for cross-references. The
   `get_operators::<T>()` accessor handles this, but ordering constraints
   (line segments must be registered before valves) need to be documented.
