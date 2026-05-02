# Runtime Architecture Refactor Analysis

This note analyzes issue #137, `Refactor runtime architecture for clearer ownership boundaries`, and recommends an implementation order for its sub-issues.

## Summary

The refactor should start by introducing a single application-level command contract, then split the simulation domain from its runtime, then decompose renderer orchestration, and only after that clean up remaining coordinator state, bootstrap policy, and migration shims.

That order fits both the explicit sub-issue dependencies and the current code structure:

- Command flow is currently split across `InputAction`, `PhysicsCommand`, and `OperatorCommand`.
- Simulation ownership and stepping behavior are still coupled in the core application layer.
- Runtime lifecycle and domain mutation are both handled in the physics-thread path.
- Renderer state and renderer orchestration are concentrated in `UnifiedRenderState`.
- WGPU startup code still owns world bootstrap policy that should live in core.

## Current Architecture Pressure Points

### 1. Command flow is fragmented

The same user intent currently crosses multiple command layers:

- renderer-facing `InputAction`
- runtime-facing `PhysicsCommand`
- domain-facing `OperatorCommand`

This creates translation logic at the renderer and runtime boundaries and makes it harder to reason about where new actions should be added.

### 2. Simulation state and stepping are not clearly separated

The application core still mixes persistent world state, stepping behavior, stats updates, operator effects, and snapshot-oriented concerns. That makes it difficult to test the physics/domain slice in isolation.

### 3. Physics runtime and domain mutation are coupled

The physics-thread path currently combines thread lifecycle, wakeups, cadence, shutdown, command transport, and command execution against the simulation. Those are different responsibilities and should not have to evolve together.

### 4. Renderer state is also acting as an orchestrator

`UnifiedRenderState` currently owns renderer state, input handling, tool lifecycle, viewport updates, energy-window behavior, and frame bookkeeping. That makes it a coupling hotspot for unrelated renderer concerns.

### 5. Backend entrypoints still encode domain policy

The WGPU entrypoint is still responsible for initial world/bootstrap sizing logic. That policy belongs in core so it can be shared across render backends and kept independent from backend wiring.

## Dependency Constraints

The explicit dependency graph in the issue set is:

- `#144` has no prerequisite issues.
- `#141`, `#139`, and `#143` depend on `#144`.
- `#145` depends on `#144` and `#141`.
- `#140` depends on `#139`.
- `#138` depends on `#145` and `#139`.
- `#146` depends on `#141` and `#140`.
- `#142` depends on substantial progress in `#144`, `#141`, `#145`, `#139`, `#140`, `#143`, `#138`, and `#146`.
- `#147` depends on all of the refactor issues above, including `#142`.

Those constraints already rule out most alternative sequences.

## Recommended Implementation Order

### 1. `#144` Introduce `DomainCommand` as the single app-level command contract

This should land first because it creates the command boundary the rest of the refactor can target.

Why first:

- It gives UI, renderer, and runtime code one canonical application-level command model.
- It removes ambiguity about whether new behavior belongs in renderer actions, runtime commands, or operator commands.
- Both the domain split and renderer split depend on having this boundary available, even if they use temporary adapters during migration.

Expected outcome:

- user actions are translated into `DomainCommand`
- temporary adapters bridge `DomainCommand` into the current runtime path
- focused tests cover representative action-to-command mappings

### 2. `#141` Split simulation state from simulation stepping logic

Once `DomainCommand` exists, separate the domain world from the engine that mutates it.

Why second:

- `#145` explicitly depends on this split
- the runtime cannot cleanly drive the simulation until there is a distinct state container and stepping service
- this is the boundary that makes later bootstrap and runtime cleanup make sense

Expected outcome:

- persistent state moves toward a `SimulationWorld`-style container
- stepping and update logic moves toward a `SimulationEngine`-style service
- a temporary facade can preserve current call sites while the rest of the refactor lands

### 3. `#145` Extract physics runtime from domain command execution

After the domain split, separate thread/runtime lifecycle from command handling.

Why third:

- the runtime should drive a world/engine boundary instead of mutating a monolithic application object directly
- this is the earliest point where thread lifecycle, cadence, wakeups, and shutdown can be isolated without inventing new temporary abstractions
- it reduces coupling before the renderer-side coordination work starts to shrink APIs

Expected outcome:

- runtime lifecycle code moves into a focused `PhysicsRuntime`
- domain mutation moves into a focused command handler
- pause/resume, snapshot cadence, and shutdown behavior stay unchanged

### 4. `#139` Decompose `UnifiedRenderState` into state plus orchestrators

With command flow and physics runtime boundaries in place, reduce renderer-side coupling.

Why fourth:

- renderer orchestration can now target `DomainCommand` instead of bespoke runtime paths
- input flow and frame bookkeeping can be moved out of the monolithic state holder without simultaneously redesigning the backend runtime
- `#140` depends on this boundary cleanup

Expected outcome:

- owned renderer state stays in a smaller state container
- input/tool flow moves into an input orchestrator
- frame and snapshot bookkeeping move into a frame orchestrator

### 5. `#140` Split WGPU app runtime into lifecycle, event router, and frame renderer

Only after renderer orchestration is decomposed should the backend runtime be split.

Why fifth:

- otherwise the WGPU refactor would still have to route through a monolithic `UnifiedRenderState`
- this sequence lets the WGPU work focus on backend lifecycle, event dispatch, and frame submission instead of also solving renderer/domain coupling
- it creates the clean backend boundary that `#146` depends on

Expected outcome:

- lifecycle/setup code becomes isolated
- event matching and dispatch become isolated
- frame submission and redraw handling become isolated

### 6. `#143` Modularize operator commands by feature area

This issue only depends on `#144`, but it is better scheduled after the structural domain/runtime/renderer boundaries are in place.

Why sixth:

- it is not on the critical path for the major architecture splits
- delaying it reduces merge churn while command flow and runtime boundaries are still moving
- after `#144` and the major boundary work, the feature-module split is mostly a packaging and discoverability improvement

Expected outcome:

- operator commands move into feature-oriented modules
- root-level re-exports preserve the public surface during migration
- future operator additions have a clearer home

Note:

If parallel capacity is available, `#143` could start earlier after `#144`. For a single sequential track, placing it here is lower risk.

### 7. `#138` Remove stale coordinator state and tighten coordinator APIs

This cleanup should happen after both the runtime split and the renderer split.

Why seventh:

- the stale or duplicated coordinator state is easiest to identify after `#145` and `#139` remove the largest sources of coupling
- earlier cleanup would risk preserving transition-only state that later becomes unnecessary
- this issue is best treated as a convergence pass on the new boundaries

Expected outcome:

- coordinators keep only active responsibilities
- duplicate tracking and weak ownership are removed
- remaining APIs become narrower and better documented

### 8. `#146` Centralize world bootstrap policy outside renderer entrypoints

Bootstrap policy should move only after both the core domain boundary and the backend runtime boundary are clearer.

Why eighth:

- it depends on `#141` and `#140`
- the right home for bootstrap policy only becomes obvious after simulation ownership and renderer startup wiring are separated
- doing it later avoids embedding bootstrap utilities into temporary abstractions

Expected outcome:

- initial world sizing/bootstrap policy lives in core
- renderer startup becomes thinner and more backend-agnostic
- default startup behavior remains unchanged

### 9. `#142` Document target architecture and migration strategy

The architecture document should be written once the intended steady-state structure is visible.

Why ninth:

- the issue explicitly depends on substantial progress across the refactor
- writing it too early would document the migration scaffolding instead of the target architecture
- by this point the document can describe both the implemented boundaries and any remaining cleanup work accurately

Expected outcome:

- contributors can see the final dependency direction
- temporary shims and their planned removal are documented clearly
- future work has a stable reference point

### 10. `#147` Remove migration shims and enforce final architectural boundaries

This is the final convergence step.

Why last:

- every earlier issue may introduce temporary adapters or compatibility facades
- removing shims before the new seams are stable would force unnecessary rework or break incremental delivery
- this issue turns the migration structure into the permanent architecture

Expected outcome:

- transitional adapters are removed
- final module boundaries are enforced
- CI verifies the simplified architecture without fallback paths

## Recommended Phases

The work naturally groups into four phases:

1. Command and domain foundation: `#144`, `#141`, `#145`
2. Renderer decomposition: `#139`, `#140`
3. Cleanup and policy extraction: `#143`, `#138`, `#146`
4. Documentation and hardening: `#142`, `#147`

## Final Recommendation

Use this sequential order for the mainline refactor:

1. `#144`
2. `#141`
3. `#145`
4. `#139`
5. `#140`
6. `#143`
7. `#138`
8. `#146`
9. `#142`
10. `#147`

This order matches the dependency graph, fits the current coupling points in the codebase, minimizes churn in the largest refactor slices, and leaves shim removal until the new ownership boundaries are actually stable.