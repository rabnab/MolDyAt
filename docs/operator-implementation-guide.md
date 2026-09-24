# Operator Implementation Guide

This guide describes how to implement operators in the existing architecture without breaking sidebar behavior, rendering, or tests.

## Scope

Operators covered by current framework:
- Line segment barrier
- Valve segment (planned/partial)
- Thermostat (planned/partial)
- Source sphere (planned/partial)

Primary goals:
- Keep behavior consistent between UI selection, renderer state, and simulation core
- Preserve physics invariants and deterministic state transitions
- Keep operator changes testable per layer

## Architecture Layers

### Shared primitives and invariants

File: `core/src/objects2d/operator.rs`

Responsibilities:
- Shared ON/OFF state (`OperatorState`) for toggleable operators
- Shared ID counter (`OperatorIdCounter`) with reset-on-clear semantics
- Shared resize remap utilities (`remap_point`, `is_degenerate`)

Rules:
- Reuse these primitives for all operators instead of creating duplicate logic
- Keep rescale logic centralized through shared helper functions

### Event loop and input actions

Files:
- `core/src/graphics/renderer/event_loop/input_handler.rs`
- `core/src/graphics/renderer/event_loop/unified_render_state.rs`
- `core/src/graphics/renderer/event_loop/action_processor.rs`
- `core/src/graphics/renderer/event_loop/line_segment_tool.rs`

Responsibilities:
- Tool selection state and interaction modes
- Conversion of user input into `InputAction`
- Dispatch of actions into simulation commands

Rules:
- Renderer side should not directly mutate simulation internals
- Use actions/commands as the boundary
- Keep active tool switching deterministic

### Sidebar mapping and hit-testing

Files:
- `core/src/graphics/sidebar.rs`
- `core/src/graphics/renderer/event_loop/sidebar_tool.rs`
- `wgpu_renderer/src/wgpu/pipelines/sidebar_pipeline.rs` (atlas order in `SIDEBAR_ATLAS_ICONS`; icon bytes in `wgpu_renderer/src/wgpu/gpu/textures.rs`)

Responsibilities:
- Shared icon constants (`ICON_COUNT`, sizes, spacing)
- Pixel hit-testing to icon index
- Icon index to `OperatorTool` mapping
- Sidebar geometry and highlight rendering

Rules:
- Maintain strict index consistency across all layers
- If icon order changes, update mapping and atlas order in the same PR
- Ensure visible icon count is clamped by both screen space and actual icon count

### Simulation-side operator effects

Files:
- `core/src/application/core/line_segments.rs`
- `core/src/application/core/mod.rs`

Responsibilities:
- Persistent world-space operator state
- Collision/effect application during simulation update
- Clear/reset and ID lifecycle
- Rescale behavior on world changes

Rules:
- Store operator geometry in world coordinates
- On resize, preserve relative placement using shared remap helper
- Avoid coupling simulation logic to renderer-only concerns

## Current line segment collision contract

File: `core/src/application/core/line_segments.rs`

Current barrier behavior:
- Detect overlap by surface distance threshold
- Skip degenerate segments
- Reflect only when moving toward segment (velocity gate)
- Apply position correction before/with reflection
- Use elastic reflection preserving speed magnitude
- Return accumulated wall momentum components for pressure accounting

When modifying this path:
- Keep momentum accounting stable unless intentionally redesigning pressure model
- Keep reflection math and collision tolerances documented in code comments

## Operator extension checklist

When adding a new operator type:

1. Add tool enum variant and action path
- Update `OperatorTool`
- Add/extend input action(s)
- Wire actions through state handling and action processor

2. Add sidebar mapping and icon
- Add icon constant/asset in renderer pipeline
- Update `operator_tool_for_icon` and `selected_sidebar_icon`
- Keep icon index stable across renderer and event loop

3. Add simulation-side state and update logic
- Introduce world-space representation
- Define clear/reset behavior
- Integrate effect into simulation step path

4. Add rescale support
- Apply `remap_point` for position-like fields
- Guard with `is_degenerate` where needed

5. Add tests by layer
- Shared primitives tests in `core/src/objects2d/operator.rs`
- Simulation behavior tests in `core/src/application/core/tests/`
- Event loop/selection tests in `core/src/graphics/renderer/event_loop/tests/`
- WGPU sidebar tests in `wgpu_renderer/src/wgpu/tests/`

## Testing expectations

At minimum for operator changes:
- Mapping tests: icon index to tool and selected tool to icon index
- Interaction tests: select tool, place/commit, edit/move
- Physics tests: collision or effect semantics (including no-op edge cases)
- Resize tests: relative position preservation and degenerate-rect safety
- Sidebar tests: visible icon clipping and geometry consistency

Use existing patterns in:
- `core/src/application/core/tests/line_segments.rs`
- `core/src/graphics/renderer/event_loop/sidebar_tool.rs` tests
- `wgpu_renderer/src/wgpu/tests/sidebar_pipeline.rs`

## Common mistakes to avoid

- Updating icon assets/order without updating tool mapping
- Treating sidebar index as always equal to global icon count assumptions
- Mutating simulation state directly from renderer-only code paths
- Forgetting reset behavior for IDs and operator collections
- Ignoring resize remap requirements for world-space entities
- Adding large inline test modules instead of topic files in `tests/`
