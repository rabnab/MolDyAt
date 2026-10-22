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
| Physics-thread owner | `MainContent` | `core/src/application/core/mod.rs` | The world plus `StepConfig` (time step, ball factory), `RuntimeOptions` (step-by-step, timing log, ball-stats display, real-units display; kept across loads, never saved) and `PhysicsRuntimeState` (wait time, snapshot slot, snapshot interval, physics FPS, save store, world generation, requested snapshot layers, pause during a window resize) |
| Run settings | `SimConfig` | `core/src/application/sim_config/` | Time step, speed limit, force cap and cutoff, Verlet skin, snapshot interval; built from defaults, a `--config` TOML file and CLI flags, stored in saves (see below) |
| World state | `SimulationWorld` | `core/src/application/core/mod.rs` | Balls, `BallPhysics` constants, cell grid, stats, operator collections (line segments, valves, wall chains, source spheres, thermostats), the ghost wall and thermo field history; physical state only, no UI flags (#305) |
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
   Nosé–Hoover thermostats. The velocity update runs in parallel per ball;
   its per-ball energies and momenta are summed serially in ball order, so
   the sums are the same for every thread count.
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

### Keyboard commands

Every keyboard command is one `CommandDescriptor` in the table `COMMANDS`
(`core/src/graphics/renderer/event_loop/command_registry.rs`):

| Field | Meaning |
|---|---|
| `id` | Stable id `<category>.<name>`, e.g. `sim.toggle_pause`, for key-binding files and headless scripts (plan items C3, C5). Never renamed |
| `label` | One line for the help text and the README key table. It may name a value such as `{normal_gravity}` (`LABEL_VALUES`), so it never repeats a constant |
| `category` | Help-text group; its prefix starts the id (`sim`, `balls`, `tools`, `world`, `view`, `debug`, `help`) |
| `default_binding` | `KeyBinding`s: key, modifier (`None`, `Ctrl`, `Alt`) and trigger (`Press`, `PressOnce` ignores auto-repeat, `Release`) |
| `kind` | `Ui(UiAction)`, handled in the renderer, or `Domain(fn() -> DomainCommand)`, sent to the physics thread |

```text
KeyCode → InputHandler::handle_key_down / handle_key_up
        → command_for_key(key, phase, held modifiers) → InputAction::Command(&CommandDescriptor)
        → InputOrchestrator::process_action → action_processor::run_command
              ├─ Domain(make) → PhysicsCoordinator::send_command(make())
              └─ Ui(action)   → run_ui_action (viewport, renderer config, help, energy window)
```

The table generates the key dispatch, the help text (`H`, `help_text()`)
and the UI-vs-domain routing. Key lookup: while a modifier is held, its
binding for the key wins (Ctrl before Alt); otherwise the plain binding fires.
A chord is decided when its key goes down with the modifier held
(`chord_for_key`) and owns that key until the release: Ctrl+S fires on press,
Alt+`=` on the release even if Alt was let go first, and neither ever runs the
plain key's command (S, `=`). A modifier pressed after the key, or during its
auto-repeat, does not make a chord: the key stays plain. The backend handles
`UiAction::CycleCutoffRings` itself, since the rings live in its ball pipeline.

Adding a keyboard command:

1. Add a `CommandDescriptor` to `COMMANDS` with a new id and a free key.
2. Its handler: for a domain command, a variant in the right `DomainCommand`
   group and its arm in `DomainCommandHandler` (the logic goes into
   `MainContent`); for a UI command, a `UiAction` variant and its arm in
   `run_ui_action`.
3. Add its key name to `key_name` if the key is new, and paste the README key
   table that the failing `test_readme_key_table_matches_the_registry` prints.

The tests in `event_loop/tests/command_registry.rs` check unique ids and
bindings, labels, that every binding dispatches to its command, the help text
and the README table. A command must not depend on wall-clock time or thread
timing, so headless and seeded runs stay reproducible. Mouse input (placing
balls, zoom, pan, the operator tools) carries positions and is not in the
registry.

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
format version, the world rectangle, time step, gravity, the simulation
config, every ball, and one `operators` list, each through its own file type
in `format.rs` (`BallData`; `OperatorData` with `WallChainData`,
`SourceSphereData`, `ThermostatData`); the simulation types do not derive
serde (#270), so refactoring them cannot change the file. An operator entry
is `{"<kind>": {...}}`, and a wall chain holds its pieces and valves (#274),
so the file has no references between operators.
Loading replaces the world, increments the
world generation (the step count starts again, so the renderer can tell the
new world's snapshots apart), and
sends `WorldLoaded` so the window can resize to fit the loaded world.

### Simulation config

`SimConfig` gathers the run settings that are not part of the world: the time
step and speed limit factor (`[integration]`), the force cap and cutoff of the
pair potential (`[potential]`, turned into `CapSettings`), the Verlet skin
factor (`[neighbours]`) and the snapshot interval of the physics thread
(`[display]`). Its defaults are the values the code used before it existed.
`MainContent::with_config` builds the world from it: `BallPhysics` takes the
cap settings and skin factor, `SimulationWorld` the speed limit factor,
`StepConfig` the time step and `PhysicsRuntimeState` the snapshot interval.
`MainContent::config()` reads them back, with the current time step, so it is
always the config the simulation runs with.

`main` reads the config once: the defaults, or the TOML file given with
`--config` (missing keys keep their default, unknown keys are an error), and
headless `--dt` overrides its time step. A save stores the config in
`WorldFile::config`; loading builds the new world from it, so a loaded world
runs as it did when it was saved. `config` is a defaulted field (see below):
saves written before it existed load with the defaults. The top-level
`time_step` stays the time step of the file, and a saved config must agree
with it.

### Format versions and migrations

Every file carries `version`; saving always writes the current
`WORLD_FILE_VERSION`. Loading (`migrate::parse`) reads the version first. A
current-version file is deserialized directly. An older one is parsed into a
JSON value, the migrations from its version up to the current one run
(`v1 → v2 → …`), the upgrade is logged, and only then is it read into
`WorldFile`. Both are validated. Files without a version, with version 0, or
with a newer version are rejected with a clear error.

A file from a newer build of the same version can hold fields this build does
not know (#273). Unknown keys inside `config` are rejected, with a hint that
the save is probably newer: a dropped config key would change the physics.
Unknown fields anywhere else are skipped; `serde_ignored` collects their paths
(`balls[3].charge`; a field on every ball is one `balls[*].label` entry with a
count), and `load_world` logs them as one warning and counts them
in the load message. So an addition that changes results bumps the version,
even when it is additive for older files, and older builds refuse the file
instead of running it with different physics.

- An additive change needs no migration (and keeps the version unless it
  changes results, see above): a new field
  with `#[serde(default)]` or `#[serde(default = "…")]` whose default
  reproduces the behaviour of older builds, a new top-level list
  (`#[serde(default)] Vec<…>`), or a new `OperatorData` variant. Older files lack
  the field and load with the default, which `validate()` accepts. Example:

  ```rust
  #[serde(default = "elastic")] // returns 1.0, today's elastic walls
  pub wall_restitution: f32,
  #[serde(default)]             // older files have no sinks
  pub sinks: Vec<SinkData>,
  ```
- A breaking change (rename, removal, changed type, unit or meaning, or a
  changed default that older files rely on) bumps
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
