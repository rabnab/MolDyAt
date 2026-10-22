# Operators

Operators are the objects a user places in the simulation box from the
sidebar: walls, valves, thermostats and source spheres. Each one is
simulation state in world coordinates, changed only through operator
commands, and drawn from the renderer snapshot.

The rules for changing operator code (files that must stay in sync, required
tests) are in the `operator-framework` skill (`.claude/skills/`).

## The operators

| Operator | Tool | Data model | Toggle | Effect on the simulation |
|---|---|---|---|---|
| Wall (line segment) | `OperatorTool::LineSegment` | `SimLineSegment { id, start, end, thickness }`, grouped into `WallChain`s | always on | Impenetrable capsule; balls reflect elastically |
| Valve | `OperatorTool::ValveSegment` | `ValveSegment { id, is_closed, … }` on a wall, range `t_start..t_end` along it | open / closed | A closed valve blocks like the wall; an open one is a gap |
| Thermostat | `OperatorTool::Thermostat` | `SimThermostat { id, position, state, target_temperature, radius, chain, tau, current_local_temp }` | on / off | Nosé–Hoover chain thermostat on the balls inside its radius |
| Source sphere | `OperatorTool::SourceSphere` | `SimSourceSphere { id, center, state, emit_period, ball_species, emission_speed }` | on / off | Emits a ball at its perimeter every `emit_period` steps |

The data models live in `core/src/objects2d/` (`line_segment.rs`,
`wall_chain.rs`, `operator/`). `SimulationWorld` holds one collection per
type: `line_segments`, `valve_segments`, `wall_chains`, `source_spheres`,
`thermostats`.

### Walls and valves

A wall the user draws is one `WallChain`: two draggable endpoints and zero or
more valves cut into it. Internally the chain alternates solid pieces (each
an independent `SimLineSegment` with its own id and thickness) and joints
between them; a valve sits in a joint. A wall without valves is a chain with
one piece and no joints, so adding a valve always splits one piece.

Collision code does not know about chains or valve state. Once per step,
`collect_solid_obstacles` flattens every piece and every closed valve into
plain capsules; an open valve produces no obstacle. A ball touches a wall
when its centre is its wall contact distance from the surface
(`BallPhysics::wall_contact_distance`, from the `WallLaw`: hard contact at
`σ_i / 2` of its species). Contacts are then resolved in two places:

- **Position update.** `SimulationWorld::sweep_wall_contact` finds the
  contact time on the ball's path and reflects it along the surface normal
  (`ball_impl::wall_contact`), exactly for constant acceleration, including
  several bounces or resting within one step. The box walls use the same
  routine. The velocity update applies the returned contacts and books the
  momentum change as wall momentum, which feeds the pressure readout.
- **Overlap fallback.** A ball that starts a step inside a wall (for example
  after a wall was drawn on top of it) is pushed out and reflected if it moves
  toward the wall. `SimStats::wall_overlap_corrections` counts these cases.

While the user draws or drags a wall, a `GhostWall` collides like a real wall
but its contacts do not count toward wall momentum.

### Thermostats

Each step, after forces, contacts and the overlap fallback, every active
thermostat gets the balls whose centre is inside its radius; a ball inside
several regions belongs to the nearest one. For each thermostat with at least
two balls:

1. The local temperature is the centre-of-mass-subtracted kinetic
   temperature of its balls, the same formula the thermo field uses:

   $$
   T_{local} = \frac{\sum_i m_i v_i^2 - |\sum_i m_i v_i|^2 / \sum_i m_i}{2(N - 1)}
   $$

2. A target temperature of 0 freezes the balls.
3. Otherwise a 5-link Nosé–Hoover chain is integrated in several small
   reversible sub-steps, with $g = 2(N-1)$ degrees of freedom,
   $Q_0 = g T_{target} \tau^2$ and $Q_{1..4} = T_{target} \tau^2$. Chain
   variables are clamped to `[-10, 10]`.
4. Only the peculiar velocities $u_i = v_i - v_{com}$ are scaled by
   $e^{-\xi_0 dt}$, so the region's drift is preserved.

Kinetic energy, centre-of-mass velocity and maximum speed are recomputed
after the thermostats, so the stats and the energy plot reflect the change in
the same step. The 2D virial pressure of a cell uses the matching kinetic
term $(N-1)T$:

$$
P = \frac{(N - 1) T + \tfrac{1}{2} \sum_{i \lt j} \mathbf{r}_{ij} \cdot \mathbf{F}_{ij}}{A}
$$

The design history is in [nose-hoover.md](done/nose-hoover.md).

### Source spheres

An active source sphere emits a ball of its species at its perimeter every
`emit_period` steps, with speed `emission_speed`. Emission runs after the
velocity update, so new balls do not enter the thermostats in the step they
appear.

A new sphere emits the species of new balls at the time it is placed;
right-clicking it with the source tool cycles its species through the world's
table. The emitted ball is placed only if it keeps the contact radius of its
own species clear of other balls and walls.

## Lifecycle

`core/src/objects2d/operator/mod.rs` provides the parts every operator
shares:

- `OperatorState` (`On` / `Off`) for toggleable operators, with the green
  circle / red cross indicator colour.
- `OperatorIdCounter`: ids increase monotonically and restart at zero when
  the simulation is cleared. A loaded collection continues after its highest
  id.
- `remap_point` and `is_degenerate`: on a world resize every position keeps
  its fractional place in the world rectangle; a zero-area rectangle is
  skipped. Thermostat radii scale with a single isotropic factor so regions
  stay circular.
- `OperatorCollection`: the trait through which `MainContent` clears and
  rescales all collections (`application/core/operators.rs`).

Clearing the balls (Escape) also removes every operator. Saved worlds include
all operators and wall chains.

## Input and tools

The sidebar tools are one list, `TOOLS` in `core/src/graphics/sidebar.rs`
(#307): ball, wall, valve, thermostat, source sphere, each a
`ToolDescriptor { tool, icon, name }`. Hit-testing (`sidebar_tool.rs`), the
selected icon and the wgpu icon atlas (`sidebar_pipeline.rs`) all derive from
it. The ball icon is drawn procedurally; the renderer maps every other
`ToolIcon` to its image (`icon_image`) and builds the atlas rows from the list
in icon order.

Selecting a tool installs its `OperatorToolHandler`
(`core/src/graphics/renderer/event_loop/`). The previous handler is reset
first, so a half-drawn wall is dropped. Each handler receives mouse button,
cursor and scroll events with a `ToolContext` (cursor position and the
operators of the last snapshot) and returns a `ToolResponse`, which may carry
an `OperatorCommand` for the physics thread. Handlers also supply the frame's
`ToolPreview`, snap indicators and the hovered object.

| Tool | Left button | Right button | Scroll | Ctrl+scroll |
|---|---|---|---|---|
| Wall | press–drag–release draws a wall; drags an endpoint of an existing wall; snaps to walls and endpoints | – | – | thickness of the wall under the cursor |
| Valve | press on a wall and drag to cut a valve; click on a valve toggles open/closed | – | – | – |
| Thermostat | click on empty space places one; click on one toggles it; drag moves it | – | target temperature | radius |
| Source sphere | click on empty space places one, emitting the species of new balls (`N` / `Alt + N`); click on one toggles it | on a sphere: cycles the species it emits | emission speed | emission period |

Hover and delete: every tool reports the object of its own kind under the
cursor (or being dragged) as an `ObjectRef`. The renderer highlights it with
`HighlightPipeline`, and draws wall endpoint markers only for the hovered
wall. Delete or Backspace sends `DeleteObject { target }` for that object.

## Commands

Every operator change is a struct that implements `OperatorCommand`
(`core/src/application/operator_command/`), sent as
`DomainCommand::Operator(Box<dyn OperatorCommand>)` and executed on the
physics thread against `MainContent`:

| Module | Commands |
|---|---|
| `line` | `AddLineSegment`, `MoveLineSegmentEndpoint`, `ChangeLineThickness`, `SetGhostWall` |
| `valve` | `AddValveSegment`, `ToggleValve` |
| `thermostat` | `PlaceThermostat`, `ToggleThermostat`, `MoveThermostat`, `AdjustThermostatTargetTemperature`, `AdjustThermostatRadius` |
| `source` | `PlaceSourceSphere`, `ToggleSourceSphere`, `CycleSourceSphereSpecies`, `AdjustSourceSphereSpeed`, `AdjustSourceSphereEmitPeriod` |
| `delete` | `DeleteObject` (any single operator object) |

Commands that refer to an id that no longer exists do nothing.

## Rendering

`RendererSnapshot` carries an `OperatorSnapshot`: line segments, walls
(chains with endpoints), valve spans resolved to world coordinates, source
spheres and thermostats. Live thermostat readings are included only when the
`READOUTS` layer is requested. The tools read the same snapshot through
`ToolContext`.

The wgpu backend draws operators with these pipelines in
`wgpu_renderer/src/wgpu/pipelines/`:

| Pipeline | Draws |
|---|---|
| `wall_pipeline` | wall capsules and the ghost wall |
| `valve_pipeline` | valves: closed with red outline and stripes, open with green outline |
| `endpoint_pipeline` | endpoint markers of the hovered wall |
| `highlight_pipeline` | the hovered object |
| `line_pipeline` | snap indicators and the source-sphere speed/rate arcs |
| `source_sphere_pipeline`, `source_label_pipeline` | source spheres; species name, speed (`S:`) and rate (`R:`) labels |
| `thermostat_pipeline`, `thermostat_label_pipeline` | thermostat zone (pulsing while on), icon, target and local temperature |
| `sidebar_pipeline` | the tool icons and the selected tool |

## Tests

| Layer | Location |
|---|---|
| Shared primitives | `core/src/objects2d/operator/` |
| Simulation effects (collisions, energy, thermostats, sources, rescale) | `core/src/application/core/tests/` (for example `line_segments.rs`, `barrier_energy.rs`, `wall_handover.rs`) |
| Tools and sidebar mapping | `core/src/graphics/renderer/event_loop/tests/` and the tool modules |
| wgpu sidebar and pipelines | `wgpu_renderer/src/wgpu/tests/` |
| End to end | the seeded headless operator fixture (`seeded_headless_operators.jsonl`) |
