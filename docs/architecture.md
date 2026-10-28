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
| `crates/thermo_physics` | `thermo_physics` | The physics: particles and species, pair potentials and the SIMD force kernel, cell grid and Verlet lists, walls and operators (`objects2d`); the world, `MainContent` and its stepping (`application/core`); run settings (`sim_config`), units, stats, thermo fields and saves (`persistence`). Dependencies: rayon, serde, rand, toml, log |
| `crates/thermo_runtime` | `thermo_runtime` | Running a simulation without a window: the physics thread and its `SnapshotPublisher`, `DomainCommand` and its handler, `OperatorCommand`s, `PhysicsEvent`, `RendererSnapshot` and its builder (`CreateSnapshot`), scene layers, the metrics store (`metrics`), the data export (`export`), and headless runs (`headless_cli`, `headless_runner`, `headless_output`, `headless_timings`, `headless_script`). Dependencies: `thermo_physics`, arc-swap, serde_json, rayon, log |
| `crates/thermo_ui` | `thermo_ui` | The UI without a GPU backend (`graphics`): event loop and input handling, the keyboard command registry and key map, operator tools, viewport, sidebar model, and the energy plot window's state with its plot layout (`PlotScene`, drawn by the backend, which owns the OS window, #371); plus the logger setup and the UI settings file (`ui_settings`: `ui.toml` with the key bindings, #335). Dependencies: `thermo_runtime`, `thermo_physics`, winit (input types only: keys, mouse and window events), arc-swap, toml, log, env_logger |
| `wgpu_renderer` | `moldyat_wgpu` | The `moldyat` binary: command line, the app shell (`wgpu/shell`: winit event loop, the main and plot windows, the plot window created and closed as `thermo_ui` asks, #371) and the renderer (`wgpu/render`: GPU context, surfaces, render pipelines; no winit, #372). For `--headless` it computes the default world from its window size and calls `thermo_runtime`, handing it the git commit it was built from (`HeadlessRunner::from_config`) |

Dependencies point one way: `moldyat_wgpu` → `thermo_ui` →
`thermo_runtime` → `thermo_physics`. `thermo_physics` and `thermo_runtime`
build and test without a windowing or GPU stack: `cargo test -p
thermo_physics`, `cargo test -p thermo_runtime`.

Each crate is used under its own name: there are no re-exports of one
crate's modules in another. The former `moldyat_core` (`core/`), which
re-exported the physics and the runtime at their old paths for one release
(#308, #309), became `thermo_ui` (#310).

### The `thermo_physics` API

Only what `thermo_runtime` and the UI use is public (#309); everything
else is crate-private, so the physics can change its internals without
touching the other crates.

- `SimulationWorld` has no public fields. Its state is read through
  accessors (`balls()`, `sim()`, `physics()`, `cell_grid()`,
  `wall_chains()`, `valve_segments()`, `thermostats()`, …); it changes only
  through `MainContent` methods, so the runtime cannot break invariants
  such as the index contract.
- The physics thread steps with `MainContent::step_recording_thermo_fields`
  (thermo fields only while the heatmap is shown); headless runs use
  `MainContent::simulation_step`. `SimulationEngine` and
  `MainContent::step_parts` are internal.
- The feature `test-support` adds what the tests of dependent crates need:
  the world builders in `thermo_physics::test_support`, a few test helpers
  (`Ball2D::new`, `SPECIES_M`, `resolve_line_segment_collisions`, …) and
  mutable access (`MainContent::world_mut`, `SimulationWorld::balls_mut`,
  `sim_mut`, `physics_mut`, `thermostats_mut`, `set_step_count`). Only
  dev-dependencies enable it; normal builds never contain these.

## Threads

```text
main thread (winit event loop)            physics thread
──────────────────────────────            ──────────────
WgpuWindow / EventRouter                  PhysicsRuntime loop
   │                                         │
UnifiedRenderState + orchestrators           │
   │  DomainCommand  ── mpsc channel ──►  DomainCommandHandler → MainContent
   │                                         │   MainContent::step_recording_thermo_fields
   │  RendererSnapshot ◄── ArcSwap slot ──  SnapshotPublisher (CreateSnapshot)
   │  PhysicsEvent     ◄── mpsc channel ──  (save / load results)
FrameRenderer (shell) → draw_main_frame → RenderGraph → GPU (render)
```

- **Main thread.** Owns the window, input handling, the viewport, the active
  operator tool and all GPU resources. It never touches simulation state.
- **Physics thread.** Owns `MainContent` and its `SnapshotPublisher`
  (the snapshot slot and the layers the renderer asked for) exclusively. Each loop iteration
  drains all queued commands, runs one simulation step (unless paused),
  and publishes a snapshot when the world changed.
- **Rayon pool.** The physics step parallelises neighbour-list rebuilds,
  positions and forces with `par_iter`. The pool has `RAYON_NUM_THREADS`
  workers, or the number of cores minus 2.
- **Energy plot window.** A separate OS window (`graphics/energy_plot`)
  with the history of the metrics series in stacked plots, drawn on the main
  thread with wgpu (see [Energy plot window](#energy-plot-window)).

## Layers

| Layer | Type | Lives in | Owns |
|---|---|---|---|
| Command boundary | `DomainCommand` | `crates/thermo_runtime/src/application/domain_command.rs` | Every change the UI asks of the simulation |
| Physics runtime | `PhysicsRuntime`, `DomainCommandHandler` | `crates/thermo_runtime/src/application/physics_thread/` | The thread loop: draining commands, sleep cadence, snapshot cadence, wakeups, shutdown, steps/s reporting |
| Physics-thread owner | `MainContent` | `crates/thermo_physics/src/application/core/mod.rs` | The world plus `StepConfig` (time step, ball factory), `RuntimeOptions` (step-by-step, timing log, ball-stats display, real-units display; kept across loads, never saved) and `PhysicsRuntimeState` (wait time, snapshot interval, physics FPS, save store, world generation, pause during a window resize) |
| Snapshots | `SnapshotPublisher`, `CreateSnapshot` | `crates/thermo_runtime/src/application/physics_thread.rs`, `crates/thermo_runtime/src/application/snapshot.rs` | The slot shared with the renderer and the requested layers; building a `RendererSnapshot` from a `MainContent` |
| Metrics | `MetricsRegistry` | `crates/thermo_runtime/src/metrics/` | Time series (E_kin, E_pot, E_tot, wall momentum, ball count per species; reduced units, labelled `eps*` and `m*sigma*/tau*`) in ring buffers, with the step and simulation time of every sample, sampled from the snapshot once per new step just before it is published, and cleared for a new world generation. `EnergyData` reads the energy series (appending only new values); headless frames print the latest values; `export` writes its series and a per-ball snapshot dump as CSV or JSONL (`Export` world command, `--export-dir`), and `SampleStream` appends every sample to a file as it is taken (`ToggleSampleStream` world command, fed by the physics loop right after sampling, so its writes block that thread; `--export-stream`). Both carry the simulation time of each sample |
| Run settings | `SimConfig` | `crates/thermo_physics/src/application/sim_config/` | Time step, speed limit, force cap and cutoff, Verlet skin, snapshot interval; built from defaults, a `--config` TOML file and CLI flags, stored in saves (see below) |
| World state | `SimulationWorld` | `crates/thermo_physics/src/application/core/mod.rs` | Balls, `BallPhysics` constants, cell grid, stats, operator collections (line segments, valves, wall chains, source spheres, thermostats), the ghost wall and thermo field history; physical state only, no UI flags (#305) |
| Stepping | `SimulationEngine` | `crates/thermo_physics/src/application/core/simulation_engine.rs` | One simulation step over a `SimulationWorld` (see below); holds no state |
| Operator mutations | `OperatorCommand` structs | `crates/thermo_runtime/src/application/operator_command/` | Placing, changing and deleting operators |
| Physics primitives | `Ball2D`, `SpeciesTable`, `BallPhysics`, `CellGrid`, operators | `crates/thermo_physics/src/objects2d/` | Ball data, particle species, pair potential and SIMD force kernel, Verlet lists, wall contact, operator data models |
| Renderer state | `UnifiedRenderState` | `crates/thermo_ui/src/graphics/renderer/event_loop/` | Input handler, physics bridge, and `RenderStateData` (viewport, energy window, renderer config, cursor and tool state, last snapshot, frame metrics) |
| Backend | `WgpuWindow` and friends | `wgpu_renderer/src/wgpu/` | Window, GPU context, event routing, frame submission, render pipelines |

### Simulation step

`SimulationEngine::simulation_step_with_dt` runs these phases in order (the
phase names match `crates/thermo_physics/src/application/phase_timings.rs`).
Outside `thermo_physics` a step goes through `MainContent`:
`step_recording_thermo_fields` on the physics thread, `simulation_step` in
headless runs; both step with the configured time step.

1. **neighbours**: balls whose displacement since their last rebuild exceeds
   skin/3 rebuild their Verlet lists from the 3x3 surrounding cells.
2. **positions**: velocity-Verlet position update. Contacts with the box
   walls and with line-segment walls are resolved along the wall normal
   (`ball_impl::wall_contact`).
3. **forces**: pair forces from the SIMD kernel (`simd_forces`, `f32x4`).
4. **apply**: velocity update, kinetic and pair potential energy, wall
   momentum bookkeeping, the overlap fallback for balls still inside a
   wall, and the Nosé–Hoover thermostats. The velocity update runs in
   parallel over fixed chunks of 1024 balls; each chunk sums its energies
   and momenta in ball order and the chunk sums are combined in chunk
   order, so the sums are the same for every thread count. With gravity
   on, a last pass adds `m·g·h` to the potential energy (#334), from the
   positions after the overlap fallback, with `h` the height above the
   bottom wall (so f32 keeps the other energies in worlds far from the
   origin) and balls with a NaN position left out. Gravity's acceleration
   and potential are defined once (`ball_core::gravity_acceleration` /
   `gravity_potential`). Changing gravity or adding balls shifts the total
   energy: it measures drift only while gravity and the ball count stay
   fixed.
5. **cells**: source spheres emit balls, balls move between grid cells,
   invalid (NaN) balls are removed.
6. **thermo**: global temperature.

While a field overlay is shown, the physics thread also samples the per-cell
fields (temperature, pressure, density, flow velocity) every `THERMO_SAMPLE_STRIDE` steps (see
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

`wgpu_renderer/src/wgpu/` is two modules with a one-way dependency (#372),
shell → render:

- **`shell/`** talks to the OS: the winit event loop, the main and plot
  windows, input into `UnifiedRenderState`, redraw requests, sizes.
- **`render/`** draws: the GPU context and surfaces, the pipelines, the
  render graph, one frame without a window. It uses no winit (a test,
  `tests/render_guard.rs`, fails on a `winit` outside comments under
  `render/`). What it needs of a window is `RenderWindow`
  (`render/surface_window.rs`: raw window and display handles, size,
  request a redraw, about to present), which the shell implements for
  winit's `Window` (`shell/winit_window.rs`).

The data the renderer draws comes from `thermo_ui` (`RendererSnapshot`,
`PlotScene`), so the split keeps the renderer free of the OS, not of the
UI. A later crate split (library for `render`, binary for `shell`) is then
a file move.

| Module | Type | Responsibility |
|---|---|---|
| `shell` | `WgpuWindow` | winit `ApplicationHandler`; carries out the plot window requests after every event |
| `shell` | `AppLifecycle` | Creates the main window, GPU context, pipelines and shared state |
| `shell` | `EventRouter` | Classifies window and device events and dispatches them |
| `shell` | `FrameRenderer` | The window side of a frame: redraw requests, the requested window size, acting on a `FrameOutcome` (configure the surface, redraw, exit) |
| `shell` | `PlotTarget` (`plot_window.rs`) | The plot window's OS window (#371): created, retitled and closed as `thermo_ui` asks |
| `render` | `WgpuContext` | Instance, device, queue, main surface and adapter, created from a `RenderWindow` and the display handle |
| `render` | `draw_main_frame` / `encode_frame` (`frame.rs`) | One frame: prepare the pipelines from the snapshot, record into a target view (the surface texture, or an offscreen texture in the GPU smoke tests), present; returns a `FrameOutcome` |
| `render` | `PlotSurface` / `PlotSurfaces` | The plot window's surface and `PlotPipeline` on the main window's device, kept per window by an opaque id (`u64`, from the shell) |
| `render` | `RenderGraph` / `WgpuPipelines` | Ordered set of render pipelines with named handles |

## Command flow

1. The backend turns window events into `InputAction`s (`InputHandler`), or
   passes mouse events to the active operator tool.
2. Action processing and the tools produce `DomainCommand`s.
3. `PhysicsCoordinator::send_command` puts them on the channel to the
   physics thread.
4. `DomainCommandHandler` applies each command to `MainContent`.

### Keyboard commands

Every keyboard command is one `CommandDescriptor` in the table `COMMANDS`
(`crates/thermo_ui/src/graphics/renderer/event_loop/command_registry.rs`):

| Field | Meaning |
|---|---|
| `id` | Stable id `<category>.<name>`, e.g. `sim.toggle_pause`, for key-binding files and headless scripts (plan items C3, C5). Never renamed |
| `label` | One line for the help text and the README key table. It may name a value such as `{normal_gravity}` (`LABEL_VALUES`), so it never repeats a constant |
| `category` | Help-text group; its prefix starts the id (`sim`, `balls`, `tools`, `world`, `view`, `debug`, `help`) |
| `default_binding` | `KeyBinding`s: key (`BindKey`), modifier (`None`, `Ctrl`, `Alt`) and trigger (`Press`, `PressOnce` ignores auto-repeat, `Release`). A key is a `Symbol(char)`, the character the key types, wherever the layout puts it: every key that types one is bound so (letters, digits, `+`, `-`, the numpad), so shortcuts sit where they are printed (#327; decision on #354). Only keys that type no character (SPACE, ESC, TAB, DEL, BACKSPACE, Shift, F1–F12) are a physical `Code(KeyCode)` |
| `kind` | `Ui(UiAction)`, handled in the renderer, or `Domain(id)`, sent to the physics thread: the `DomainCommand` comes from the shared table `DOMAIN_COMMANDS` in `thermo_runtime` (`application::command_table`, #340), which headless scripts use too. A test checks that the key table and the domain table name the same commands |

```text
KeyEvent → key_input (physical KeyCode + character typed without modifiers)
        → InputHandler::handle_key_down / handle_key_up (KeyInput)
        → KeyMap::command_for_key(key, phase, held modifiers) → InputAction::Command(&CommandDescriptor)
        → InputOrchestrator::process_action → action_processor::run_command
              ├─ Domain(id)   → PhysicsCoordinator::send_command(domain_command_spec(id).build(None))
              └─ Ui(action)   → run_ui_action (viewport, renderer config, help, energy window)
```

The table holds the default bindings and generates the UI-vs-domain
routing. The bindings in effect are a `KeyMap` (`key_map.rs`, owned by
`InputHandler`): the defaults, with the entries of a UI settings file
(`ui.toml` in the working directory, or `--ui-config`; `thermo_ui::ui_settings`,
#335) applied by `KeyMap::with_overrides`. An entry replaces all keys of one
command; a key it binds is taken away from the command that had it by
default; unknown ids and keys are logged and skipped. Key names
(`key_names.rs`) are those of the help text, so `parse_key` reads what the
help shows. The key dispatch (`KeyMap::command_for_key`) and the help text
(`H`, `KeyMap::help_text`) read the key map. Key lookup: while a modifier is held, its
binding for the key wins (Ctrl before Alt); otherwise the plain binding fires.
Within one modifier, a key's physical binding comes first (only keys that
type no character have one), then the binding of the character it types
without modifiers (`key_without_modifiers`: Shift, Alt / Option and dead
keys do not change it), then of the character it types with the modifiers
held, for layouts where a character needs Shift (Swiss `+` is Shift + 1,
AZERTY digits need Shift). Letters are matched in lower case, as a key
types them without Shift, and shown in upper case. The first of these
with a binding for the held modifier owns the press, also in the phase
where that binding does not fire, so a key bound to a press command never
runs the command of its character on release. Layouts without Latin
letters (e.g. Russian) are not supported: no key types `s`, so the letter
shortcuts have no key there (decided on #354). On platforms where
winit has no unmodified character (web, iOS, Android) both are the typed
character, so there a modifier can change it. A release and every
auto-repeat use the characters of the first press. `=` is a hidden alias
of `+` (the same key on US keyboards): it works but the help shows `+`.
The energy window looks its keys up in the same key map
(`energy_window_key`): the keys of `view.cycle_plot` and
`view.toggle_energy_window` (`P`, `TAB`) cycle what it plots, the key of
`view.reset_plot_view` (`HOME`) resets its time range, and `ESC`
closes it, as that window's own close shortcut.
A chord is decided when its key goes down with the modifier held
(`KeyMap::chord_for_key`) and owns that key until the release: Ctrl+S (the key that
types `s`) fires on press,
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
3. Add its key name to `key_names.rs` if the key has none, and paste the README key
   table that the failing `test_readme_key_table_matches_the_registry` prints.

The tests in `event_loop/tests/command_registry.rs` check unique ids and
bindings, labels, that every binding dispatches to its command, the help text
and the README table; `event_loop/tests/key_map.rs` checks the key names and
the overrides, and `ui_settings/tests.rs` the settings file and the README
example. A command must not depend on wall-clock time or thread
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
   `RendererSnapshot` (`CreateSnapshot::fill_snapshot` for the
   requested layers) and its `SnapshotPublisher` stores it in an
   `ArcSwapOption` slot. The publisher keeps the snapshot it replaced and
   refills it next time if the renderer has let go of it, so a steady run
   allocates no new ball, operator or debug vectors (#341). It publishes at most once per snapshot interval
   (16.7 ms), and immediately after a resize. A paused, untouched world
   publishes nothing.
2. A wakeup callback asks the winit event loop for a redraw.
3. `PhysicsCoordinator` hands the latest snapshot to the orchestrators;
   On the redraw, `FrameRenderer` (shell) has the renderer draw the frame
   (`draw_main_frame`: prepare the pipelines from the snapshot, record,
   submit, present) and acts on its `FrameOutcome`.

A snapshot is in sections (#341); each pipeline reads only its own, and a
new visual adds a section or data derived from one, not a top-level field:

| Section | Type | Holds |
|---|---|---|
| `frame` | `FrameInfo` | Step, world generation, layers, time, energies, wall momentum, ball count per species, gravity preset, physics rate, world rectangle, and the numbers of the status lines (`StatusNumbers`, which the UI formats: `energy_plot/status_text.rs`) |
| `balls` | `BallBuffer` | `BallRenderData` per ball (no heap data), the species table, the species of new balls and its ring radii; derefs to the ball slice |
| `operators` | `OperatorSnapshot` | Wall pieces, walls, valve spans, source spheres, thermostats |
| `fields` | `FieldSnapshot` | The cell grid spec and the averaged per-cell fields (`CellFields`: temperature, pressure, density, flow velocity) with their grid |
| `debug` | `Option<DebugSnapshot>` | Neighbour lines of verbose balls; `None` without any |

`PrepareContext` passes the sections, not the snapshot, to the pipelines.

The renderer chooses which `SceneLayers` it needs (`BASE`, `BALLS`, `THERMO`,
`READOUTS`) with a `View` command; `LAYERS` (`scene_layers.rs`) describes
each layer and names the registry commands that switch it. The physics thread builds only those
parts of the snapshot, and computes the thermo fields only while `THERMO` is
requested. With rendering switched off (F2), only `BASE` is requested.

## Render pipelines

Every visual layer is one `RenderPipeline` in
`wgpu_renderer/src/wgpu/render/pipelines/`, registered in draw order in
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
  populated frame (skipped without a GPU adapter), the energy plot's
  `PlotPipeline` included.
- The energy plot window has its own surface and pipeline outside the render
  graph (see [Energy plot window](#energy-plot-window)).

### Energy plot window

`EnergyWindowManager` (`thermo_ui`) decides when the window opens and
closes, owns its keys and the plot selection, and keeps `EnergyData`
in step with the metrics store: every registry series with its unit and
the simulation time of each sample (`MetricsRegistry::times`), appending
only new values. What the window shows is a `PlotSelection`
(`energy_plot/plot_selection.rs`, #339): one to `MAX_PLOTS` (4) plots
stacked on a shared time axis, each with one or more series by registry
name, some of them hidden, and the focused plot that the plot commands
change. It lives in the manager and survives closing the window; it is not
saved between runs. `PlotScene::build` (`energy_plot/plot_layout.rs`) lays
the plots out in window pixels without a GPU: the title, key hints built
from the key map (`energy_window_key_hints`; trailing hints are dropped
when the title line is full, and a `ui.toml` rebinding shows), then per
plot a legend line (a focus mark with several plots, each series in its
colour, a hidden one grey, the unit when all share one) above its own
axes, grid and zero line and curves (a sample without a finite value or
time breaks a curve), with time labels under the last plot only, then the
info line (time, zoom state) and the status text. Each plot's y axis fits
the series it draws (hidden ones excluded); all plots share their left
edge, so their times line up. The energies keep their colours, other
series take one from a palette by their place in the series list
(`series_color`). The scene records each plot's area (`PlotScene::panels`)
and each legend entry (`PlotScene::legend`) for clicks. The x axis
is simulation time over the range of a `TimeView` (`energy_plot/time_view.rs`).
By default it shows a fixed span (`TIME_SPAN`, 10 time units): the time
step changes only how fast the curves scroll, never the scale. Until the
history spans that much the curves start at the left edge, then the newest
sample stays at the right edge (follow mode); a history shorter than the
span (the registry keeps 1000 samples) leaves the left part empty (the
sample stream keeps the whole history on disk, #366). Curves are cut at
both edges of the range, and
the y range (round ticks, 1/2/5 × 10ⁿ) fits what is visible, the cut values
included. x ticks come every 0.5 time units when there is room (else every
1, 2, 5, …), with labels only where they fit. Labels and thin lines sit on
whole pixels so the bitmap glyphs and 1 px lines stay crisp. A curve with
more points inside the range than twice the plot's pixel columns is drawn
with the lowest and highest point of each column only (`downsample`, #366),
so the draw cost stays constant and one-sample spikes stay visible; fewer
points are drawn as they are. The layout is unit-tested there.

The mouse changes the range (#367), through the energy window's own event
path (`EnergyWindowManager::handle_event`; mouse input is not in the
command registry). The scroll wheel zooms by `ZOOM_FACTOR` (1.25) per notch,
line and pixel deltas alike: around the pointer, or with the newest sample
kept at the right edge in follow mode. The span stays between four sample
intervals and the whole history (at least `TIME_SPAN`). A middle-button
drag pans with the pointer (dragging right shows earlier times) once it
has moved more than a few pixels (`PAN_THRESHOLD_PX`), so a click with a
little jitter does not pause the plot; panning away from the newest sample
leaves follow mode, panning back returns to it (a zoom never does, even
when the range reaches the newest sample). While not following, the
plot is paused: `update_from_snapshot` does not sync `EnergyData`, so the
history stays as it was when the pause began and can be examined (the
status text and the current time still update; the legend shows the
paused range instead of the time). Back in follow mode it syncs at once
and jumps to the current series. A cleared registry (a loaded world) is
noticed while paused by `EnergyData::registry_history_changed`, which
checks the epoch of the registry's times without copying, and ends the
pause. A middle double click or the key of `view.reset_plot_view` (`HOME`)
resets the view, and so does a new world history (`EnergyData::history_resets`,
counted when the registry is cleared). The pixel-to-time conversion uses
the plot area and history of the last frame (`PlotScene::plot_area`,
`PlotScene::history`). A view change redraws at once instead of waiting for
the redraw throttle; the legend shows a zoomed or paused range. The view
lives in the manager and survives closing and reopening the window.

The plots are changed by registry commands (`UiAction::Plot(PlotChange)`),
which work from both windows: `view.cycle_plot` / `view.toggle_energy_window`
in the plot window give the focused plot the next energy preset
(`PRESETS`), `view.plot_next_series` / `view.plot_previous_series`
(`.` / `,`) replace its last series with the next one not shown,
`view.plot_add_series` / `view.plot_remove_series` (`Ctrl + P` / `Alt + P`)
add or remove one, `view.plot_add_plot` / `view.plot_remove_plot`
(`Ctrl + L` / `Alt + L`) add a plot below the focused one or remove it, and
`view.plot_focus_next` (`F`) moves the focus. The energy window tracks Ctrl
and Alt itself (`ModifiersChanged`) for the chords. A left click on a
legend entry hides or shows that series, a left click in a plot focuses
it (the targets of the last frame).

The OS window belongs to the binary (#371), so `thermo_ui` uses winit for
input types only. The manager asks for it with `PlotWindowRequest`s (`Open`
with the title, `Close`, `SetTitle`, `Redraw`), queued while it handles
an event and taken by the app after every event
(`UnifiedRenderState::take_plot_window_requests`). `PlotTarget`
(`shell/plot_window.rs`) carries them out (`handle_plot_requests`, tested with a
fake window): it creates the window on the energy half of the main
window's monitor (`split_screen_layout`, pure, in `thermo_ui`; the
simulation window keeps its size, #167), with the minimum size
`MIN_WINDOW_WIDTH` × `MIN_WINDOW_HEIGHT`, and reports back with
`plot_window_opened` (size and scale factor) or `plot_window_failed`.
Later sizes come from the window's own `Resized` / `ScaleFactorChanged`
events, which the router hands to the manager by window id
(`PlotTarget::window_id`).

The binary draws it: on a `RedrawRequested` for the plot window,
`UnifiedRenderState::energy_window_redraw` hands out a `PlotFrame` (the
scene, laid out for the reported size), and `PlotTarget` hands it to the
renderer's `PlotSurfaces` (`render/plot_surface.rs`), which draws it into a
surface of its own, created from the shared `Instance` and drawn with the
shared device and queue; it knows the window as a `RenderWindow` and an
opaque id. `PlotPipeline` (`pipelines/plot_pipeline.rs`) is not part
of the render graph: it draws a scene, not a snapshot, with two
`GpuBatch`es, anti-aliased round-capped segments (`plot_segment.wgsl`) and
coloured 8×8 bitmap text (`plot_text.wgsl`), both in window pixels. The
surface prefers `Bgra8Unorm` / `Rgba8Unorm` (no sRGB encoding, so the
colours appear as given). The surface goes with its window
(`PlotSurfaces::sync` by id); a window whose surface cannot be created is
logged once and not retried until it is replaced. Redraws are throttled to about 12 Hz.

## Persistence

`crates/thermo_physics/src/application/persistence/` saves and loads the whole world
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
The size asked for shows the world 1:1 (`LEN_TO_PX`), but at most the
monitor and the GPU's largest surface (`max_texture_dimension_2d`); a larger
world is asked for at its shape and shown scaled down, letterboxed. Every
surface size is clamped to that GPU limit as well
(`WgpuContext::resize`), so no window size can make wgpu panic (#363).

### Simulation config

`SimConfig` gathers the run settings that are not part of the world: the
size of a new world (`[world]`, `WorldConfig::rect()`: `DEFAULT_WORLD_RECT`
scaled by `scale`, #351; a loaded save keeps its `world_rect`), the time
step and speed limit factor (`[integration]`), the force cap and cutoff of the
pair potential (`[potential]`, turned into `CapSettings`), the Verlet skin
factor (`[neighbours]`) and the snapshot interval of the physics thread
(`[display]`). Its defaults are the values the code used before it existed.
Both modes build a new world in `world.rect()`. The window's default size
shows that box exactly, and a window resize keeps the world units per pixel
the world is shown at (`ViewportTransform::world_per_px`), so a scaled or
loaded world keeps its scale. `MainContent::with_config` builds the world from it: `BallPhysics` takes the
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
(`crates/thermo_runtime/src/headless_runner.rs`). It steps the simulation directly on
the main thread and writes snapshots as versioned JSON Lines
(`headless.v2`: `run_start`, `frame`, `command`, `run_end` records) to stdout or a file.
A `--script` file runs domain commands (by their registry id, through the
same `DomainCommandHandler` as the physics thread) and operator commands at
chosen steps (`headless_script.rs`, `headless_runner/script_run.rs`, #340).
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
