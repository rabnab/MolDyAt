---
name: operator-framework
description: Contract for implementing or changing operators (line segment, valve, thermostat, source sphere) and the sidebar tool icons. Use when adding, renaming, reordering or editing operator tools, sidebar hit-testing or geometry, or barrier collision physics.
---

# Operator Framework (AI Implementation Contract)

Use this when implementing or changing operators (line segment, valve, thermostat, source sphere).

## Single Source Of Truth Rules
- Keep icon count and geometry constants in `core/src/graphics/sidebar.rs` as the shared source for both hit-testing and rendering.
- Keep icon-to-tool mapping synchronized across:
  - `core/src/graphics/renderer/event_loop/input_handler.rs` (`OperatorTool` enum)
  - `core/src/graphics/renderer/event_loop/sidebar_tool.rs` (`operator_tool_for_icon()`, `selected_sidebar_icon()`)
  - `wgpu_renderer/src/wgpu/sidebar_pipeline.rs` icon atlas order (`ICON_WALL`, `ICON_VALVE`, `ICON_THERMO`, `ICON_SOURCE_OPEN`)
- If you add/reorder/remove tools, update all three locations in the same change.

## Event Loop And Command Flow
- UI selection/hit-testing stays in renderer state (`UnifiedRenderState`) and emits `InputAction` values.
- Physics-affecting changes must flow through commands (not direct mutation from renderer):
  - `SelectOperatorTool`
  - `CommitLineSegment`
  - `MoveLineSegmentEndpoint`
- Preserve separation of concerns:
  - Renderer/event loop: tool state and interaction
  - Application core: simulation state and collision/physics effects

## World-Space Behavior Requirements
- Operator geometry is world-space state. Store operator positions in world coordinates.
- On world resize, preserve fractional location using shared helpers from `core/src/objects2d/operator.rs`:
  - `remap_point()` for endpoint/anchor remapping
  - `is_degenerate()` guard to avoid invalid remaps
- Keep clear/reset behavior deterministic:
  - Clearing simulation also clears operator collections
  - ID counters reset to zero on clear

## Collision And Physics Requirements
- For impenetrable barriers (line segments), maintain this behavior:
  - Position correction to eliminate overlap
  - Velocity reflection using surface normal (`v' = v - 2(v·n)n`)
  - Reflect only when moving toward barrier (velocity gate)
  - Preserve speed magnitude for elastic reflection
- Keep pressure/momentum accounting wired into collision resolution when modifying barrier physics.

## Sidebar Renderer Coupling Rules
- Sidebar hit-testing and WGPU geometry must agree on which icons are visible for a given window height.
- In sidebar geometry builders, clamp visible icons by both:
  - Available vertical space
  - Actual icon count in atlas/tool set
- When selected icon highlighting changes, ensure selected index semantics remain stable (`Option<usize>`).

## Test Requirements For Operator Changes
- Place tests in topic files under `tests/` directories (no large inline test modules).
- Minimum coverage for operator changes:
  - Selection/mapping tests (icon index ↔ tool)
  - Placement/edit interaction tests (commit/move endpoint)
  - Collision behavior tests (reflect/moving-away/no-collision)
  - Rescale invariants (relative position preserved, degenerate-rect guard)
  - Sidebar visibility/geometry consistency tests
- Keep tests for each layer near code under test:
  - Shared operator primitives: `core/src/objects2d/operator.rs`
  - Core simulation/operator effects: `core/src/application/core/tests/`
  - Event loop + sidebar mapping: `core/src/graphics/renderer/event_loop/tests/`
  - WGPU sidebar geometry: `wgpu_renderer/src/wgpu/tests/`
