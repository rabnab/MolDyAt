# Operator Handling — Review Findings

Review of the `OperatorToolHandler` trait-dispatch architecture after the
consolidation refactoring.  Items are ranked by impact.

---

## High Priority

### 1. Missing tool reset on switch

**File:** `core/src/graphics/renderer/event_loop/unified_render_state.rs`
(`SelectOperatorTool` arm)

When the user switches tools the old handler is replaced without calling
`reset()`.  If a user is mid-draw on a line segment and clicks a
different tool, the drawing state is silently dropped.  The `reset()`
method exists on the trait but is never invoked on tool switch.

**Fix:** Call `reset()` on the outgoing handler before replacing it.

### 2. Thermostat tool is half-wired

**Files:** `input_handler.rs` (`OperatorTool` enum), `sidebar_tool.rs`,
`unified_render_state.rs` (`handler_for_tool`)

`OperatorTool::Thermostat` exists in the enum, the sidebar mapping, and
the icon atlas, but `handler_for_tool()` returns `None` for it.
Selecting it in the UI silently does nothing.

**Fix:** no fix needed. this is just prepared for a future implementation.

### 3. `handle_cursor_move` returns `bool` while peers return `ToolResponse`

**File:** `core/src/graphics/renderer/event_loop/operator_tool_handler.rs`

`handle_mouse_button` and `handle_scroll` return `ToolResponse`, but
`handle_cursor_move` returns a bare `bool`.  This means cursor-move
events cannot emit commands and the inconsistency forces the dispatcher
to special-case the call.

**Fix:** Unify to `ToolResponse` for all three input methods.

---

## Medium Priority

### 4. `OperatorPreview` enum vs `OperatorPreviews` struct — confusing names

**Files:** `operator_tool_handler.rs`, `mod.rs`

Two nearly-identical names for different concepts.  The enum is *"one
tool's preview"* (returned by each handler), while the struct is *"all
previews bundled for the frame"* (consumed by the renderer).

**Suggestion:** Rename to e.g. `ToolPreview` / `FramePreviews` to
disambiguate.

### 5. `make_tool_context_from` with `LazyLock` static fallback

**File:** `unified_render_state.rs`

The split-borrow workaround uses a `std::sync::LazyLock` static to
provide a default `OperatorSnapshot` when no snapshot exists.  This is
functional but unusual.

**Alternatives:**
- Store a default `OperatorSnapshot` as a struct field.
- Make `ToolContext.operators` an `Option<&OperatorSnapshot>` and let
  handlers handle the absent case.

### 6. Sidebar icon-to-tool mapping is duplicated

**Files:** `sidebar_tool.rs` (`operator_tool_for_icon`,
`selected_sidebar_icon`), `wgpu_renderer/.../sidebar_pipeline.rs` (atlas
order constants)

The index→tool mapping and the icon atlas order must stay synchronised
manually.  Adding or reordering a tool requires editing both locations.

**Fix:** Define a single const array (e.g. in `core/src/graphics/sidebar.rs`)
and derive both the hit-test mapping and the atlas order from it.

### 7. Silent no-op on stale operator IDs

**Files:** `core/src/application/core/source_spheres.rs` (and similar
toggle/adjust methods)

Commands like `toggle_source_sphere` silently succeed when the ID does
not exist.  No log, no error, no user feedback.

**Fix:** At minimum emit a `log::warn!` when an operator ID is not
found.

---

## Low Priority

### 8. `needs_continuous_redraw` naming is unclear

**File:** `operator_tool_handler.rs`

The name suggests frame-rate concerns, but the actual meaning is *"does
cursor movement alone require a redraw?"* (e.g. for snap indicators).

**Suggestion:** Rename to `redraw_on_cursor_move`.

### 9. `SourceSphereToolHandler` is stateless but heap-allocated

**File:** `source_sphere_tool.rs`

The handler is a zero-size struct that gets `Box`-allocated on every tool
selection.  Not a real performance concern, but a `&'static` singleton
would avoid the allocation entirely.

### 10. `ToolContext` lacks cursor delta

**File:** `operator_tool_handler.rs`

`ToolContext` carries only `cursor_world_pos`, not previous position or
movement delta.  Future tools that react to drag velocity would need
this.

**Suggestion:** Add `cursor_delta: Option<Vec2D>` when needed.

### 11. No mechanism to disable/grey-out unavailable tools

**Files:** `operator_tool_handler.rs`, sidebar rendering

There is no way to mark a tool as unavailable in the current context
(e.g. "valve tool disabled because no line segments exist").

**Suggestion:** Add an optional `is_available` query method if/when
needed.
