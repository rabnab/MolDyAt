# Nosé-Hoover Thermostat Plan

This document describes how to integrate issue 129 into the current operator architecture. The goal is to add a Nosé-Hoover thermostat that controls the temperature of a finite spherical region around its origin without breaking the existing separation between renderer input, simulation state, and rendering snapshots.

## Scope

The implementation should provide:

- a placeable thermostat operator with world-space position and radius
- ON/OFF state, target temperature control, and draggable repositioning
- a finite spherical region of influence
- Nosé-Hoover-chain velocity control using 5 active chain variables per thermostat bucket
- renderer snapshot support for visualization and labels
- deterministic handling of overlapping thermostat regions

The implementation should not:

- fold thermostat behavior into the thermo-field visualization code
- mutate simulation state directly from renderer code
- apply multiple thermostats sequentially to the same ball in one step

## Key Design Decisions

### 1. Keep the thermostat as a first-class operator

The thermostat should follow the same architectural path as line segments, valves, and source spheres:

- world-state data in `core/src/objects2d/operator/`
- storage in `SimulationWorld`, owned by `MainContent`
- mutations through `OperatorCommand`
- rendering through `RendererSnapshot`

This keeps the feature aligned with the current operator framework instead of creating a one-off physics subsystem.

### 2. Reuse the local temperature formula already used by the thermo field

The thermostat should use the same center-of-mass-subtracted kinetic temperature used by `compute_cell_thermo`:

$$
T_{local} = \frac{\sum_i m_i v_i^2 - |\sum_i m_i v_i|^2 / \sum_i m_i}{2(N - 1)}
$$

In 2D that COM-subtracted definition implies a matching kinetic pressure term of $(N - 1) T$ for a cell, not $N T$.

The full cell pressure is the 2D virial pressure (`thermo_field::virial_pressure_2d`):

$$
P = \frac{(N - 1) T + \tfrac{1}{2} \sum_{i<j} \mathbf{r}_{ij} \cdot \mathbf{F}_{ij}}{A}
$$

The $\tfrac{1}{2}$ is $1/d$ for $d = 2$; each pair enters the sum once.

That formula should be extracted into a small shared helper so the thermo visualization and the thermostat cannot drift apart.

### 3. Assign each ball to at most one thermostat per step

If thermostat regions overlap, the issue requirement says the closer thermostat owns the ball. The implementation should therefore:

- collect all active thermostats
- test region membership using ball center position
- assign each ball to the nearest active thermostat whose spherical region contains it

This avoids sequential double-thermostatting and makes the result deterministic.

### 4. Apply the thermostat after collision resolution

The best insertion point in the simulation loop is after line-segment collision resolution and before source-sphere emission. That ordering gives the thermostat the final post-collision velocities for the current step and avoids immediately thermostating newly emitted balls.

### 5. Refresh velocity-derived stats after thermostat application

The thermostat changes velocity after the normal force integration path. If kinetic energy, center-of-mass velocity, and max speed are not refreshed afterward, the status text and energy plot will be stale for that frame. The implementation should therefore recompute the velocity-derived parts of `StatsAccumulator` before calling `self.sim.update(...)`.

## Integration Plan

## Phase 1: Operator Data Model

Add a new thermostat operator type under `core/src/objects2d/operator/thermostat.rs`.

The data model should include:

- `id: usize`
- `position: Vec2D`
- `state: OperatorState`
- `target_temperature: f32`
- `radius: f32`
- `chain: Vec<f32>`
- `tau: f32`
- `current_local_temp: f32`

The collection should mirror the source-sphere pattern:

- own a `Vec<Thermostat>`
- manage IDs through `OperatorIdCounter`
- implement `clear()` and `rescale()` through `OperatorCollection`

Radius rescaling should use a single isotropic scale factor so the control region stays circular after world resize.

## Phase 2: SimulationWorld Wiring

Extend `SimulationWorld` to store a thermostat collection alongside line segments, valves, and source spheres, with `MainContent` continuing to own the world plus runtime state.

Update the constructor and shared operator lifecycle wiring so thermostats:

- are initialized by default
- are removed when balls are cleared
- participate in world-resize remapping

This should happen through the same centralized operator hooks already used by the other operator collections.

## Phase 3: Shared Local Temperature Helper

Extract the local kinetic-temperature calculation from `core/src/application/thermo_field.rs` into a shared helper.

That helper should:

- accumulate `sum_m`, `sum_mv`, and `sum_mv2`
- return `None` or `NaN` when fewer than 2 balls are present
- stay 2D-specific to match the current simulation

The thermostat code should call this helper directly rather than using global temperature in `SimStats`.

## Phase 4: Thermostat Physics Step

Add simulation-side thermostat methods in a new module near `core/src/application/core/source_spheres.rs`.

The per-step algorithm should be:

1. Build thermostat ownership buckets for active thermostats.
2. For each bucket, compute `T_local` from the assigned balls.
3. If fewer than 2 balls are in the bucket, skip the update.
4. If `target_temperature == 0.0`, zero the assigned velocities.
5. Otherwise integrate a coupled 5-chain Nosé-Hoover system in a reversible
   split form (multiple small symmetric sub-steps):

$$
\dot{\xi}_0 = \frac{2K - gT_{target}}{Q_0},\qquad
\dot{\xi}_j = \frac{Q_{j-1}\xi_{j-1}^2 - T_{target}}{Q_j}, \; j=1..4
$$

with:

- $g = 2(N-1)$ thermal degrees of freedom in the local COM frame
- $Q_0 = g T_{target} \tau^2$
- $Q_1..Q_4 = T_{target}\tau^2$

6. Clamp all chain variables to `[-10, 10]` for numerical safety.
7. Scale peculiar velocities using only the first chain variable:

$$
u_i \leftarrow e^{-\xi_0 dt} u_i,\qquad v_i \leftarrow v_{com} + u_i
$$

## Phase 5: Stats and Energy Reporting

After thermostat application, recompute the velocity-derived pieces of the current step statistics:

- kinetic energy
- center-of-mass momentum and velocity
- maximum speed
- `max_id`

Potential energy should remain the value computed by the force pass. This keeps the plot and status text honest while preserving the current force calculation flow.

## Phase 6: Command and Tool Integration

Extend `core/src/application/operator_command.rs` with thermostat commands such as:

- place thermostat
- toggle thermostat
- move thermostat
- adjust target temperature
- adjust radius

Then add a thermostat tool handler in `core/src/graphics/renderer/event_loop/` following the source-sphere tool pattern.

The expected UI behavior is:

- left-click empty space: place thermostat
- left-click existing thermostat: toggle ON/OFF
- left-drag thermostat: move thermostat
- scroll: adjust target temperature
- Ctrl+scroll: adjust radius

The renderer should continue emitting commands rather than directly mutating simulation data.

## Phase 7: Snapshot and Rendering

Extend `OperatorSnapshot` and snapshot creation so thermostats are available to both the renderer and the event-loop tool context.

Rendering should show:

- a finite circular region of influence
- a visible ON/OFF indicator
- current local temperature label
- target temperature label

The thermostat icon is already present in the sidebar tool ordering, so the missing work is the scene rendering and tool handling rather than the sidebar icon itself.

## Phase 8: Safety and Edge Cases

The thermostat implementation should explicitly handle:

- fewer than 2 balls in region: no-op for that step
- target temperature equal to zero: zero velocities in region
- overlapping thermostats: nearest active thermostat owns each ball
- inactive thermostats: contribute no effect and do not claim balls
- resize events: preserve relative center placement and circular region semantics

The code should also avoid directly mixing thermostat logic into unrelated hot paths unless it reduces duplication or corrects stale stats.

## Suggested File Changes

- `core/src/objects2d/operator/thermostat.rs`
- `core/src/objects2d/operator/mod.rs`
- `core/src/application/core/mod.rs`
- `core/src/application/core/ctor.rs`
- `core/src/application/core/operators.rs`
- `core/src/application/core/thermostats.rs`
- `core/src/application/thermo_field.rs`
- `core/src/application/core/sim.rs`
- `core/src/application/operator_command.rs`
- `core/src/application/renderer_snapshot.rs`
- `core/src/application/core/snapshot.rs`
- `core/src/graphics/renderer/event_loop/thermostat_tool.rs`
- `core/src/graphics/renderer/event_loop/unified_render_state.rs`
- `wgpu_renderer/src/wgpu/` thermostat rendering files as needed

## Implementation Notes

- Keep all thermostat temperatures in reduced LJ units inside the simulation.
- Convert to display units only at the UI boundary if needed.
- Prefer a dedicated thermostat module rather than embedding the logic in `sim.rs` directly.
- Reuse the existing operator lifecycle utilities instead of duplicating ID or resize logic.
- Thermostat state keeps a 5-entry chain by default; if older snapshots provide a different length, the runtime path resizes to 5 before integration.
- The thermostat always operates in the bucket COM frame (`u_i = v_i - v_com`) so local drift is preserved.

## Todo List

- Add a thermostat operator type and collection under `core/src/objects2d/operator/`.
- Wire thermostat storage into `SimulationWorld`, constructor setup, clear, and rescale paths.
- Extract the local temperature formula from `thermo_field.rs` into a shared helper.
- Implement per-step thermostat ownership, local temperature measurement, chain update, and velocity scaling.
- Refresh velocity-derived stats after thermostat application and before `self.sim.update(...)`.
- Add thermostat operator commands for placement, toggle, move, target-temperature adjustment, and radius adjustment.
- Add a thermostat tool handler and connect `OperatorTool::Thermostat` in the event loop.
- Extend renderer snapshots to include thermostat data.
- Add scene rendering for thermostat region, indicator, and temperature labels.
- Document any final unit conventions and resize semantics in code comments where the behavior is non-obvious.

## Tests That Should Be Met

- Unit tests confirm thermostat IDs increment correctly and reset after clear.
- Unit tests confirm thermostat positions rescale correctly and degenerate resize inputs are safe.
- Unit tests confirm the shared local-temperature helper matches the current thermo-field behavior.
- Unit tests confirm a thermostat with fewer than 2 balls in its region performs no update.
- Unit tests confirm target temperature `0.0` freezes only the balls owned by that thermostat.
- Unit tests confirm hot regions cool and cold regions heat in the expected direction.
- Unit tests confirm overlapping thermostats assign each ball to the nearest active thermostat only.
- Unit tests confirm inactive thermostats do not affect velocities or claim balls.
- Snapshot tests confirm thermostat data is present in `OperatorSnapshot` and renderer snapshots.
- Event-loop tests confirm thermostat tool selection, placement, toggle, drag, scroll, and Ctrl+scroll behaviors.
- Rendering tests confirm sidebar mapping still matches icon ordering and thermostat visuals appear when snapshot data is present.
- Manual simulation checks confirm local temperature trends toward target without obvious runaway oscillation.
- Manual simulation checks confirm energy and status displays reflect thermostat-induced velocity changes in the same frame.
