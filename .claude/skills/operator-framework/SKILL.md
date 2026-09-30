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
  - `wgpu_renderer/src/wgpu/pipelines/sidebar_pipeline.rs` icon atlas order (`SIDEBAR_ATLAS_ICONS` = `ICON_WALL`, `ICON_VALVE`, `ICON_THERMO`, `ICON_SOURCE_OPEN`) and `atlas_row_for_tool()`
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
  - Resolve contacts during the position update (`SimulationWorld::sweep_wall_contact`), like the box walls: contact time on the step's path, reflection along the surface normal via `ball_impl::wall_contact` (exact for constant acceleration, including several bounces or resting within a step). Forces are then computed at the resolved position, and the velocity update applies the contact (`v_n -> -v_n` at the contact time).
  - Do not project a ball onto the surface or mirror its whole overshoot during a step: both move it against the force and gain energy on every contact.
  - Keep `resolve_line_segment_collisions` as the overlap fallback (balls that start a step inside a wall): position correction, velocity reflection (`v' = v - 2(v·n)n`) only when moving toward the barrier (velocity gate), speed preserved
  - `test_ball_*_keeps_energy` in `core/src/application/core/tests/barrier_energy.rs` guards the energy of bouncing and resting contacts
  - `SimStats::wall_contacts` / `wall_overlap_corrections` count exact contacts and fallback corrections; the drift wall scenarios require 0 corrections and `wall_handover.rs` covers the box-wall/line-wall handover
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
