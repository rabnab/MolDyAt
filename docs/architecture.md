# Runtime Architecture

MolDyAt runs a 2D Lennard-Jones simulation on a dedicated physics thread and
draws it with wgpu on the main thread. The two threads share no mutable
state: user input reaches the simulation as commands, and the simulation
reaches the screen as immutable snapshots.

How the layout came about is recorded in
[refactor_architecture.md](done/refactor_architecture.md); planned changes
are in [refactoring-plan.md](plan/refactoring-plan.md).

## Crates

| Crate | Package | Contents |
|---|---|---|
| `core` | `moldyat_core` | Physics (`objects2d`, `application/core`), the physics thread, commands, snapshots, persistence, and the backend-independent UI layer (`graphics`: event loop, tools, viewport, sidebar model, energy plot window) |
| `wgpu_renderer` | `moldyat_wgpu` | The `moldyat` binary: CLI, the wgpu window and render pipelines, and the headless runner |

`moldyat_wgpu` depends on `moldyat_core`, never the reverse. Inside
`moldyat_core`, `application` and `objects2d` do not depend on `graphics`;
the one exception is `MainContent::bootstrap_world_rect`, which uses the
viewport margins to size the initial world.

## Threads

```text
main thread (winit event loop)            physics thread
──────────────────────────────            ──────────────
WgpuWindow / EventRouter                  PhysicsRuntime loop
   │                                         │
UnifiedRenderState + orchestrators           │
   │  DomainCommand  ── mpsc channel ──►  DomainCommandHandler → MainContent
   │                                         │   SimulationEngine::simulation_step
   │  RendererSnapshot ◄── ArcSwap slot ──  MainContent::create_snapshot
   │  PhysicsEvent     ◄── mpsc channel ──  (save / load results)
FrameRenderer → RenderGraph → GPU
```

- **Main thread.** Owns the window, input handling, the viewport, the active
  operator tool and all GPU resources. It never touches simulation state.
- **Physics thread.** Owns `MainContent` exclusively. Each loop iteration
  drains all queued commands, runs one simulation step (unless paused),
  and publishes a snapshot when the world changed.
- **Rayon pool.** The physics step parallelises neighbour-list rebuilds,
  positions and forces with `par_iter`. The pool has `RAYON_NUM_THREADS`
  workers, or the number of cores minus 2.
- **Energy plot window.** A separate winit/softbuffer window
  (`graphics/energy_plot`) that draws the energy history from snapshots.

## Layers

| Layer | Type | Lives in | Owns |
|---|---|---|---|
| Command boundary | `DomainCommand` | `core/src/application/domain_command.rs` | Every change the UI asks of the simulation |
| Physics runtime | `PhysicsRuntime`, `DomainCommandHandler` | `core/src/application/physics_thread/` | The thread loop: draining commands, sleep cadence, snapshot cadence, wakeups, shutdown, steps/s reporting |
| Physics-thread owner | `MainContent` | `core/src/application/core/mod.rs` | The world plus `StepConfig` (time step, ball factory) and `PhysicsRuntimeState` (wait time, snapshot slot, physics FPS, save store, world generation, requested snapshot layers) |
| World state | `SimulationWorld` | `core/src/application/core/mod.rs` | Balls, `BallPhysics` constants, cell grid, stats, operator collections (line segments, valves, wall chains, source spheres, thermostats), the ghost wall, thermo field history, and a few run flags (step-by-step, timing and stats display, real-units display) |
| Stepping | `SimulationEngine` | `core/src/application/core/simulation_engine.rs` | One simulation step over a `SimulationWorld` (see below); holds no state |
| Operator mutations | `OperatorCommand` structs | `core/src/application/operator_command/` | Placing, changing and deleting operators |
| Physics primitives | `Ball2D`, `SpeciesTable`, `BallPhysics`, `CellGrid`, operators | `core/src/objects2d/` | Ball data, particle species, pair potential and SIMD force kernel, Verlet lists, wall contact, operator data models |
| Renderer state | `UnifiedRenderState` | `core/src/graphics/renderer/event_loop/` | Input handler, physics bridge, and `RenderStateData` (viewport, energy window, renderer config, cursor and tool state, last snapshot, frame metrics) |
| Backend | `WgpuWindow` and friends | `wgpu_renderer/src/wgpu/` | Window, GPU context, event routing, frame submission, render pipelines |

### Simulation step

`SimulationEngine::simulation_step` runs these phases in order (the phase
names match `core/src/application/phase_timings.rs`):

1. **neighbours**: balls whose displacement since their last rebuild exceeds
   skin/3 rebuild their Verlet lists from the 3x3 surrounding cells.
2. **positions**: velocity-Verlet position update. Contacts with the box
   walls and with line-segment walls are resolved along the wall normal
   (`ball_impl::wall_contact`).
3. **forces**: pair forces from the SIMD kernel (`simd_forces`, `f32x4`).
4. **apply**: velocity update, kinetic and potential energy, wall momentum
   bookkeeping, the overlap fallback for balls still inside a wall, and the
   Nosé–Hoover thermostats.
5. **cells**: source spheres emit balls, balls move between grid cells,
   invalid (NaN) balls are removed.
6. **thermo**: global temperature.

While the heatmap is shown, the physics thread also samples the per-cell
temperature and pressure fields every `THERMO_SAMPLE_STRIDE` steps (see
[simulation.md](simulation.md#thermodynamic-grid-space)).

### Renderer state and orchestration

`UnifiedRenderState` is the API that the wgpu backend calls. Its work is
split into stateless orchestrators that operate on `RenderStateData`:

| Orchestrator | Responsibility |
|---|---|
| `FrameOrchestrator` | Frame lifecycle: picks up new snapshots, frame metrics |
| `InputOrchestrator` | Input actions and operator tool events |
| `ViewportOrchestrator` | Window resize, pan and zoom |
| `PhysicsCoordinator` | Spawns the physics thread, sends commands, reads snapshots and events |

### Backend

| Type | Responsibility |
|---|---|
| `WgpuWindow` | winit application shell |
| `AppLifecycle` | Creates the window, GPU context, pipelines and shared state |
| `EventRouter` | Classifies window and device events and dispatches them |
| `FrameRenderer` | Redraws: prepares pipelines from the snapshot and submits the frame |
| `WgpuContext` | Device, queue, surface and adapter |
| `RenderGraph` / `WgpuPipelines` | Ordered set of render pipelines with named handles |

## Command flow

1. The backend turns window events into `InputAction`s (`InputHandler`), or
   passes mouse events to the active operator tool.
2. Action processing and the tools produce `DomainCommand`s.
3. `PhysicsCoordinator::send_command` puts them on the channel to the
   physics thread.
4. `DomainCommandHandler` applies each command to `MainContent`.

`DomainCommand` is grouped by concern:

| Variant | Concern |
|---|---|
| `Sim(SimControl)` | Running the simulation: pause, step mode, time step, gravity |
| `Balls(BallCommand)` | Adding, removing and changing balls |
| `World(WorldCommand)` | The world as a whole: resize, save, load |
| `View(ViewCommand)` | What is reported or shown: timing display, ball stats, snapshot layers. No effect on physics |
| `Operator(Box<dyn OperatorCommand>)` | Operator tools: walls, valves, thermostats, source spheres |

Each group has a `From` impl, so senders pass the group value, for example
`send_command(WorldCommand::Save)`. The handler dispatches each group to its
own function (`handle_sim`, `handle_balls`, `handle_world`, `handle_view`);
operator commands run through `OperatorCommand::execute`. A queued
`World(Resize)` is held back, so a run of resizes from a window drag is
applied once, before the next command or at the end of the batch.

Results that the UI must report (a save or load succeeded or failed) come
back as `PhysicsEvent`s on a second channel.

## Snapshot flow

1. After a step or a handled command, the physics thread builds a
   `RendererSnapshot` (`MainContent::create_snapshot`) and stores it in an
   `ArcSwapOption` slot. It publishes at most once per snapshot interval
   (16.7 ms), and immediately after a resize. A paused, untouched world
   publishes nothing.
2. A wakeup callback asks the winit event loop for a redraw.
3. `PhysicsCoordinator` hands the latest snapshot to the orchestrators;
   `FrameRenderer` prepares the pipelines from it and submits the frame.

The renderer chooses which `SceneLayers` it needs (`BASE`, `BALLS`, `THERMO`,
`READOUTS`) with a `View` command. The physics thread builds only those
parts of the snapshot, and computes the thermo fields only while `THERMO` is
requested. With rendering switched off (F2), only `BASE` is requested.

## Render pipelines

Every visual layer is one `RenderPipeline` in
`wgpu_renderer/src/wgpu/pipelines/`, registered in draw order in
`WgpuPipelines::new`. All of them follow one pattern:

- `new(device, queue, format)`, then `prepare()` fills a `Vec` of
  `#[repr(C)]` instances or vertices and flushes it to a `GpuBatch`;
  `render()` only calls `draw()`.
- `gpu/batch.rs` (`GpuBatch`) is the only place that creates wgpu pipelines,
  bind groups and buffers. It has two draw modes: instanced quads (one
  element is one quad, corners from `quad_corner()` in the shader) or plain
  vertex lists.
- Every shader is registered in `gpu/shaders.rs`, which prepends
  `shaders/common.wgsl` (uniforms `u.view_proj` / `u.params`, `quad_corner`,
  `aa`, `ring_band`). `u.params` is pipeline-specific and documented at the
  top of each shader. An optional texture lives at `@group(1)`.
- Screen-space layers (sidebar, text, labels, frame) build NDC positions and
  keep the identity `view_proj`.
- Tests: `gpu/tests/shaders.rs` validates all WGSL with naga, and
  `tests/gpu_smoke.rs` builds every pipeline on a real device and renders a
  populated frame (skipped without a GPU adapter).

## Persistence

`core/src/application/persistence/` saves and loads the whole world
(Ctrl+S / Ctrl+O, or `--load` on the command line). A `WorldFile` holds a
format version, the world rectangle, time step, gravity, every ball, and all
operators including wall chains. Loading replaces the world, increments the
world generation (the step count starts again, so the renderer can tell the
new world's snapshots apart), and
sends `WorldLoaded` so the window can resize to fit the loaded world.

### Format versions and migrations

Every file carries `version`; saving always writes the current
`WORLD_FILE_VERSION`. Loading (`migrate::parse`) reads the version first. A
current-version file is deserialized directly. An older one is parsed into a
JSON value, the migrations from its version up to the current one run
(`v1 → v2 → …`), the upgrade is logged, and only then is it read into
`WorldFile`. Both are validated. Files without a version, with version 0, or
from a newer build are rejected with a clear error.

- An additive change (a new optional field whose default reproduces the old
  behaviour) keeps the version.
- A breaking change (rename, removal, changed type, unit or meaning) bumps
  `WORLD_FILE_VERSION` and adds one `migrate_vN_to_vN+1(&mut Value)` to
  `MIGRATIONS` in `persistence/migrate.rs`, with a test that loads the
  version-`N` reference save. A migration must not panic on unexpected input:
  it returns an error for missing or mistyped fields (`take_field`), so a bad
  file leaves the running world untouched.
- Every version has a reference save in `persistence/tests/fixtures/`
  (`v1.json`, …). Existing ones are never edited; all must keep loading.

## Headless mode

`moldyat --headless` runs the same `MainContent` without a window
(`wgpu_renderer/src/headless_runner.rs`). It steps the simulation directly on
the main thread and writes snapshots as versioned JSON Lines
(`headless.v2`: `run_start`, `frame`, `run_end` records) to stdout or a file.
Seeded runs are deterministic; they back the headless regression fixtures
and the performance scripts. The record format, CLI
flags and fixtures are described in [headless.md](headless.md).

## Boundaries

- The renderer never mutates simulation state; every change goes through a
  `DomainCommand`.
- The physics thread never touches renderer or GPU types; it only publishes
  snapshots and events.
- `SimulationWorld` is the only owner of physical state. `SimulationEngine`
  and the operator commands change it; nothing keeps indices into its ball
  vector across a step.
- Backend modules hold no simulation state or policy; startup policy (world
  size, loading) runs through `MainContent`.

`UnifiedRenderState` still exposes a wide API to the backend. It is a facade
over `RenderStateData` and the orchestrators, and the remaining split of
that facade is tracked in the refactoring plan.
