---
name: operator-framework
description: Contract for implementing or changing operators (line segment, valve, thermostat, source sphere) and the sidebar tool icons. Use when adding, renaming, reordering or editing operator tools, sidebar hit-testing or geometry, or barrier collision physics.
---

# Operator Framework (AI Implementation Contract)

Use this when implementing or changing operators (line segment, valve, thermostat, source sphere).
`docs/operators.md` describes how operators work today; this file lists the rules for changing them.

## Sidebar And Tool Order
- `TOOLS` in `core/src/graphics/sidebar.rs` is the one tool table (#307): a `ToolDescriptor { tool, icon, name }` per sidebar tool, in icon order (ball, wall, valve, thermostat, source sphere). `ICON_COUNT`, `tool_for_icon()` / `icon_for_tool()` (used by `operator_tool_for_icon()` and `selected_sidebar_icon()` in `sidebar_tool.rs`) and the wgpu atlas (`sidebar_atlas_icons()`, `atlas_row_for_icon()` in `wgpu_renderer/src/wgpu/pipelines/sidebar_pipeline.rs`) derive from it; do not hard-code indices anywhere else.
- Reordering or renaming a tool: edit `TOOLS` only. Adding a tool: its `OperatorTool` variant (`core/src/graphics/renderer/event_loop/input_handler.rs`), its `TOOLS` entry, its handler arm in `UnifiedRenderState::handler_for_tool()`, and for a new picture a `ToolIcon` variant with its image in `icon_image()` (`sidebar_pipeline.rs`; PNG bytes in `wgpu_renderer/src/wgpu/gpu/textures.rs`, files in `wgpu_renderer/src/wgpu/icons/`). Both matches are exhaustive, so the compiler names what is missing. The ball icon is procedural (`icon_image` returns `None`) and has no atlas row.
- Keep icon size and spacing constants in `sidebar.rs`, shared by hit-testing and rendering. Visible icons are clamped by both vertical space and `ICON_COUNT` (`visible_icon_count_for_margin`); selected-icon semantics stay `Option<usize>`.

## Tools And Commands
- Each tool is an `OperatorToolHandler` (`core/src/graphics/renderer/event_loop/operator_tool_handler.rs`), created in `UnifiedRenderState::handler_for_tool()`. All input methods return `ToolResponse`; `reset()` runs when the tool is switched.
- Tools never mutate simulation state. They return an `OperatorCommand` (`crates/thermo_runtime/src/application/operator_command/<type>.rs`) inside the `ToolResponse`; it is sent as `DomainCommand::Operator` and executed on the physics thread. A new mutation is a new command struct; `DomainCommand`, `InputAction` and the action processor stay unchanged.
- Tools read operators only from the last snapshot (`ToolContext`), and supply `preview()`, `snap_indicators()` and `hovered()`.
- Hover and delete: `hovered()` returns an `ObjectRef` (`crates/thermo_physics/src/objects2d/wall_chain.rs`). A new operator type adds an `ObjectRef` variant, a `delete_object` arm (`crates/thermo_physics/src/application/core/object_deletion.rs`) and a `highlight_capsule` arm (`wgpu_renderer/src/wgpu/pipelines/highlight_pipeline.rs`).

## World State And Lifecycle
- Operator data lives in `crates/thermo_physics/src/objects2d/` (`line_segment.rs`, `wall_chain.rs`, `operator/`) in world coordinates, owned by a collection in `SimulationWorld`.
- Shared primitives in `crates/thermo_physics/src/objects2d/operator/mod.rs`: `OperatorState` for ON/OFF, `OperatorIdCounter` (reset to zero on clear; `after()` for loaded collections), `remap_point()` with the `is_degenerate()` guard for resize.
- Every collection implements `OperatorCollection` and is listed in `operator_collections_mut()` (`crates/thermo_physics/src/application/core/operators.rs`); that list is the only wiring for clear and rescale. Radii and other lengths use an isotropic scale so circles stay circles.
- Walls and valves: user walls are `WallChain`s of `SimLineSegment` pieces and valve joints. Collision code sees only capsules from `collect_solid_obstacles` (pieces plus closed valves); never look up valve state per contact.

## Collision And Physics Requirements
- Wall contacts are resolved during the position update (`SimulationWorld::sweep_wall_contact`), like the box walls: contact time on the step's path, reflection along the surface normal via `ball_impl::wall_contact` (exact for constant acceleration, including several bounces or resting within a step). Forces are then computed at the resolved position, and the velocity update applies the contact (`v_n -> -v_n` at the contact time) and books the momentum as wall momentum.
- Do not project a ball onto the surface or mirror its whole overshoot during a step: both move it against the force and gain energy on every contact.
- The overlap fallback (`resolve_wall_overlaps_with` / `resolve_line_segment_collisions`) handles balls that start a step inside a wall: position correction, velocity reflection (`v' = v - 2(v·n)n`) only when moving toward the wall, speed preserved.
- The ghost wall (`SetGhostWall`) collides like a wall but must not count toward wall momentum.
- Keep pressure/momentum accounting wired into collision resolution. `SimStats::wall_contacts` / `wall_overlap_corrections` count exact contacts and fallback corrections; the drift wall scenarios require 0 corrections.
- Operators that change velocities after the force pass (thermostats) must refresh the velocity-derived stats in the same step (`refresh_velocity_stats`). Operators that add balls (source spheres) run after the velocity update, in the `cells` phase.
- Physics changes must keep the drift suite green (`cargo ci-test-drift`); threshold changes follow `docs/drift-test.md`.

## Snapshot, Persistence And Headless Output
A new operator type, or a new persistent field, must be added in all of these:
- `OperatorSnapshot` (`crates/thermo_runtime/src/application/renderer_snapshot.rs`) and its creation in `crates/thermo_runtime/src/application/snapshot.rs`. Live readouts belong behind `SceneLayers::READOUTS`.
- A render pipeline in `wgpu_renderer/src/wgpu/pipelines/`, registered in draw order in `WgpuPipelines::new` (see `docs/architecture.md`, Render pipelines).
- The save format (see the `save-format` skill): a new operator kind is a new variant of `OperatorData` (`crates/thermo_physics/src/application/persistence/format.rs`) with its own `*Data` file type, a key in `OperatorVisitor::visit_map`, an arm in `capture_operators` and the restore in `convert.rs`, and its checks in `validate()`. If it changes results (it almost always does), bump `WORLD_FILE_VERSION` with a migration that only updates the number, so older builds refuse the file instead of skipping the operator with a warning (#273, #274). Parts an operator owns (a chain's pieces and valves) are saved inside its entry, not as operators of their own. Old saves must keep loading.
- Headless operator counts in `crates/thermo_runtime/src/headless_output.rs`.

## Test Requirements For Operator Changes
- Place tests in topic files under `tests/` directories (no large inline test modules).
- Minimum coverage:
  - Selection/mapping: icon index ↔ tool, atlas row count (`atlas_holds_one_row_per_textured_tool_in_registry_order`, `tool_list_maps_icons_and_tools_both_ways`)
  - Tool interaction: place, toggle, drag, scroll, reset on tool switch
  - Collision behaviour: reflect / moving away / no collision; energy of bouncing and resting contacts (`test_ball_*_keeps_energy` in `barrier_energy.rs`); box-wall/line-wall handover (`wall_handover.rs`)
  - Rescale invariants (relative position preserved, degenerate-rect guard), clear resets ids
  - Delete and hover for the new `ObjectRef` variant
  - Save/load round trip
  - Sidebar visibility/geometry consistency
- Test locations:
  - Shared primitives: `crates/thermo_physics/src/objects2d/operator/`
  - Simulation effects: `crates/thermo_physics/src/application/core/tests/`
  - Tools, sidebar mapping, hover: `core/src/graphics/renderer/event_loop/tests/`
  - WGPU sidebar and pipelines: `wgpu_renderer/src/wgpu/tests/`
- The seeded operator fixture (`crates/thermo_runtime/src/headless_runner/fixtures/seeded_headless_operators.jsonl`) must match. Refresh it only for intended changes: `UPDATE_HEADLESS_FIXTURE=1 cargo ci-test-headless-regression`, rerun without the variable, and include the fixture diff in the PR.
