# Refactoring Plan: Maintainability, Extensibility, Performance

Status: proposal (2026-10). Successor to the runtime refactor in
[refactor_architecture.md](refactor_architecture.md) (#137, landed). The current
structure is described in [architecture.md](architecture.md).

Goals:

1. **Maintainability** – smaller modules, clear crate boundaries, no UI types in physics.
2. **Extensibility** – add interaction types, input commands, visuals and graphs
   by adding code in one place instead of editing 3–6 files.
3. **Performance** – faster steps for large worlds without breaking the drift
   and energy guarantees.

Every item below is sized to be one issue / one PR. IDs (`M1`, `I2`, …) are
plan-local; replace them with GitHub issue numbers when the issues are created.

---

## 1. Current state (analysis summary)

~38.5k lines of Rust in two crates.

| Area | Where | Observation |
|---|---|---|
| Crate layout | `core` (`rust_thermo_core`) | Holds physics **and** renderer-agnostic UI (event loop, viewport, tools, sidebar) **and** a winit/softbuffer energy window, plus `scirs2-interpolate`/`ndarray`. Headless runs and physics tests pull in winit and heavy deps. |
| Crate layout | `wgpu_renderer` | Backend + CLI + headless runner. Headless runner lives in the GPU crate. |
| Physics model | `ball_impl/ball_physics.rs`, `simd_forces.rs` | Lennard-Jones is hard-wired: `PhysicsConstants { lj_sig, lj_eps, lj_cutoff, … }`, scalar + `f32x4` kernels, Newton solve for `r_cap`. One potential for all pairs; no notion of species. Gravity is a constant inside the velocity kick. |
| Step | `core/simulation_engine.rs` | Clear step order, but per-step allocations (`old_positions`, `force_results`, `obstacles`, `checked`), serial force application loop, T/P field binned every step, duplicated `#[cfg(feature = "measure-time")]` branches. |
| Data layout | `Ball2D` | AoS with hot (pos, v, accel) and cold (Verlet `Vec`s, `VerboseInfo`, tail deque, counters) data mixed. `KernelPoint` already packs the force-kernel read set. |
| Spatial | `CellGrid { cells: Vec<Vec<Cell>> }`, `Cell { balls: Vec<usize> }` | One heap `Vec` per cell; full neighbour lists (each pair computed twice, potential `* 0.5`). |
| World state | `SimulationWorld` | Mixes physical state with UI/debug flags (`show_timings`, `show_ball_stats`, `real_data`, `step_by_step`). |
| Legacy traits | `application/mod.rs` | `GameLogic`, `LogStuff`, `VerletIntegration` are single-implementor traits on `MainContent`; mostly forwarding. |
| Commands | `DomainCommand` | Flat enum of ~28 variants + `Operator(Box<dyn OperatorCommand>)`. Operators are already open for extension; the rest is not. |
| Input | `input_handler.rs`, `action_processor.rs`, `domain_command_mapping.rs`, help text | A new key command touches: `InputAction` enum, key `match`, `process_action_common` (exhaustive list), DomainCommand mapping, `DomainCommand`, `DomainCommandHandler`, help text. Keybindings are hard-coded. |
| Operator tools | `OperatorTool`, `sidebar_tool.rs`, `sidebar_pipeline.rs` | Tool ↔ icon mapping must be kept in sync in 3 files (documented in the `operator-framework` skill). |
| Rendering | `wgpu_renderer/src/wgpu/` | Good base: `RenderPipeline` trait + `RenderGraph` + `GpuBatch`. But `WgpuPipelines::new` hard-codes registration and `PipelineHandles`; `PrepareContext` and `RendererSnapshot` grow a field for every feature. |
| Graphs | `graphics/energy_plot/` | Single-purpose energy plot drawn as coloured characters into a separate softbuffer window. Data comes from `EnergyData` only; no generic time-series store; headless output has its own formatting. |
| Snapshot | `core/snapshot.rs` | Fresh `Vec<BallRenderData>` (≈120 B/ball incl. two `Vec` headers) every publish; status text formatted every publish. |
| Hygiene | both crates | `#![allow(dead_code)]` at both crate roots; 13 `println!` in non-test core code; several 600–800 line files (`core/line_segments.rs`, `unified_render_state.rs`, `line_segment_tool.rs`, `energy_renderer.rs`, `energy_window.rs`). |
| Open bugs touching the plan | #213 (f32 precision in large worlds), #216 (operators/mouse must always render), #217 (resize lag) | Addressed by P8, V4 and V1 respectively, or at least made easier. |

Strengths to keep: drift harness and energy tests, seeded headless regression
fixture, `OperatorCommand` / `OperatorToolHandler` traits, `GpuBatch` pattern,
lock-free snapshot slot, documented index contracts.

---

## 2. Target architecture

```text
crates/
  thermo_physics   – Vec2D, balls/particles, species, potentials, cell grid,
                     Verlet lists, walls/operators (domain), SimulationWorld,
                     SimulationEngine, persistence. Deps: rayon, serde, rand, log.
  thermo_runtime   – physics thread, DomainCommand + handler, PhysicsEvent,
                     snapshot builder, metrics/time-series store, headless runner.
  thermo_ui        – renderer-agnostic UI: input map, actions, tools, viewport,
                     sidebar model, layer/plot descriptors. Deps: winit (input types only).
  thermo_wgpu      – backend: window, pipelines, plot rendering, CLI binary.
```

Dependency direction stays one-way: `wgpu → ui → runtime → physics`.

Extension points after the refactor:

| To add… | You implement… | Registered in… |
|---|---|---|
| Interaction type | `PairPotential` (scalar + SIMD eval, cutoff, cap) | `Potential` enum + `InteractionTable` |
| External field | `ExternalField` | `SimulationWorld::fields` |
| Input command | `Command` descriptor (id, label, default binding, handler) | `CommandRegistry` (help + key map generated) |
| Operator tool | `OperatorToolHandler` + `ToolDescriptor` (icon, name) | `ToolRegistry` (sidebar + atlas order generated) |
| Visual layer | `RenderPipeline` + `LayerDescriptor` (toggle, draw order) | `LayerRegistry` |
| Graph / series | `MetricSource` (produces samples) + `PlotDescriptor` | `MetricsRegistry` / plot panel |

---

## 3. Work breakdown

Size: **S** ≤ 1 day, **M** 2–4 days, **L** ≥ 1 week. "Deps" are hard prerequisites.

### Epic 0 – Guardrails (do first)

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| G1 | Performance baseline and benchmark scenarios | Extend `scripts/headless_perf.sh` to fixed seeded scenarios (dilute gas 10k, dense gas 50k, walls + valves, large world 200k). Record ms/step split by phase (neighbours, positions, forces, apply, thermo, snapshot) as JSON. Commit baseline numbers in `docs/perf-baseline.md` (with CPU, profile). | M | – |
| G2 | Golden-output guard for refactors | Make the seeded headless regression fixture and drift suite the required gate for every refactor PR (document in CONTRIBUTING). Add a fixture for a scene with operators (walls, valve, thermostat, source). | S | – |
| G3 | Remove crate-wide `#![allow(dead_code)]` | Delete dead code or add local, justified allows. Clippy clean. | M | – |

### Epic M – Maintainability

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| M1 | Replace legacy single-impl traits | Turn `GameLogic`, `LogStuff`, `VerletIntegration` into inherent methods/free functions; `GameLogicData` → `StepConfig`. No behaviour change (G2 green). | S | G2 |
| M2 | Move UI/debug flags out of `SimulationWorld` | `show_timings`, `show_ball_stats`, `real_data`, `step_by_step` → `RuntimeOptions` in `MainContent`. World holds physical state only. Persistence format unaffected or migrated. | S | M1 |
| M3 | Unify timing instrumentation | `TimingMonad` becomes a no-op type when `measure-time` is off; remove duplicated `#[cfg]` branches in `simulation_step_with_dt`. Timings feed G1's phase report. | S | G1 |
| M4 | Replace `println!` in core with `log` / verbose sink | Verbose-ball output goes through a `log` target (`thermo::verbose`) or the snapshot. | S | – |
| M5 | Split oversized modules | `core/line_segments.rs` (obstacles, sweep, overlap fallback), `unified_render_state.rs`, `line_segment_tool.rs`, `energy_renderer.rs`. Target < 400 lines per file; tests follow the topic-file rule. | M | – |
| M6 | Central `SimConfig` | Collect `DT_GOAL`, `FPS_GOAL`, LJ defaults, skin, speed limit factor, snapshot interval into one serde struct; CLI and save files read/override it. | M | M1 |
| M7 | Crate split: `thermo_physics` | Move `objects2d`, `application/core` (world, engine), persistence into a crate without winit/softbuffer/scirs2. Keep `rust_thermo_core` as a re-export shim for one release. | L | M1, M2 |
| M8 | Crate split: `thermo_runtime` + headless | Physics thread, `DomainCommand`, snapshot, headless runner/output move out of the GPU crate. `cargo test -p thermo_physics` builds without a windowing stack. | L | M7 |
| M9 | Crate split: `thermo_ui` | `graphics/renderer/event_loop`, viewport, sidebar model, interpolation. Energy window moves to the backend (see V3). | M | M8 |
| M10 | Docs refresh | Update `architecture.md`, move `refactor_architecture.md` / `headless-renderer-plan.md` / `operator-handling-review.md` to `docs/history/`, update the `operator-framework` skill. | S | after each epic |

### Epic I – Interaction types

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| I1 | Extract `PairPotential` from `BallPhysics` | Trait/enum with `force(r)`, `potential(r)`, `force_simd(f32x4)`, `potential_simd`, `cutoff()`; capping (`r_cap`, linear extrapolation) and cutoff shift implemented **once**, generic over the potential. LJ becomes `LennardJones { sigma, epsilon }`. Bit-identical drift baselines (same target/profile). | M | G2 |
| I2 | Static dispatch for the SIMD kernel | `force_simd` generic over the potential (enum match outside the neighbour loop, monomorphised inner loop). Perf within ±2 % of G1. | M | I1 |
| I3 | Species and interaction table | `Species { id, radius, mass, colour }` replaces `size_idx`; `InteractionTable[species_a][species_b] -> Potential` with Lorentz–Berthelot mixing as default. `KernelPoint` gains a species byte (padding slot is free). Persistence: format version bump + migration from size classes. | L | I1, I2 |
| I4 | Additional potentials | Pick from: WCA (purely repulsive), Morse, soft-sphere, Yukawa/screened Coulomb. Each with energy-conservation test (dense gas drift scenario) and scalar ↔ SIMD parity test. | M each | I3 |
| I5 | `ExternalField` abstraction | Gravity becomes `UniformField`; enables e.g. centrifugal or harmonic trap fields; applied in the kick with energy accounting (`potential` reported to stats). | M | I1 |
| I6 | Bonded interactions (optional) | Harmonic/FENE bonds between ball pairs (molecules). Bonds store **ball ids**, not indices, resolved per step (index contract). Separate force pass. | L | I3 |
| I7 | UI to choose species/potential | Command + sidebar/keys to select active species for placement; interaction table editable via config file first (M6), UI later. | M | I3, C2 |

### Epic C – Input commands

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| C1 | Group `DomainCommand` | Split into `Sim(SimControl)`, `Balls(BallCommand)`, `World(WorldCommand)`, `Operator(Box<dyn OperatorCommand>)`. Handler delegates per group. | S | – |
| C2 | Command registry | One table of `CommandDescriptor { id, label, category, default_binding, kind: Ui(UiAction) | Domain(fn(&Ctx)->DomainCommand) }`. Generated: key dispatch, help text (`PrintHelp`), `process_action_common`. Adding a command = one entry + handler. Unit test: no duplicate bindings, every command has help text. | M | C1 |
| C3 | Configurable key bindings | Load overrides from a TOML/JSON file (via M6); defaults from C2. | S | C2, M6 |
| C4 | Tool registry | `ToolDescriptor { tool, icon, name }` list drives `OperatorTool`, `operator_tool_for_icon`, `selected_sidebar_icon` and the atlas order — removes the 3-file sync rule from the skill. Test: icon order == registry order. | M | – |
| C5 | Scriptable commands for headless | Headless runner accepts a command script (`step 1000; add_random 500; toggle_gravity; …`) mapped through the same registry ids. Enables reproducible scenario tests. | M | C2, M8 |

### Epic V – Visuals and graphs

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| V1 | Layer registry | Replace hard-coded registration + `PipelineHandles` in `WgpuPipelines::new` with `LayerDescriptor { id, draw_order, viewport, default_visible, toggle_command }`. Pipelines look up their own state; layer toggles become commands (C2). Addresses #216 (cursor/operator layers always prepared). | M | C2 |
| V2 | Split `PrepareContext` / snapshot into sections | `RendererSnapshot` → `{ frame: FrameInfo, balls: BallBuffer, operators: OperatorSnapshot, fields: FieldSnapshot, debug: Option<DebugSnapshot> }`; pipelines read only their section. New visual = new section or derived data, no new top-level fields. | M | V1 |
| V3 | Metrics/time-series store | `MetricsRegistry` in runtime: named series (E_kin, E_pot, E_tot, T, P, wall momentum, neighbour count, step time, …) in ring buffers, sampled at snapshot cadence. `EnergyData` becomes one consumer. Headless JSONL output reads the same store. | M | M8 |
| V4 | In-window plot panel (wgpu) | Generic line-plot pipeline (axes, ticks, legend, autoscale) rendering any selected series from V3, docked or as overlay; replaces the softbuffer character renderer. Keep the separate window as an option. | L | V2, V3 |
| V5 | Field visualisations | Generalise thermo heatmap to any scalar per-cell field (T, P, density, velocity magnitude) and add a vector field (velocity arrows). Field selection cycles through registry. | M | V2 |
| V6 | Distribution graphs | Speed histogram vs. Maxwell–Boltzmann, radial distribution function g(r) — computed off the hot path at snapshot cadence. | M | V3, V4 |
| V7 | Data export | CSV/JSONL export of selected series and a snapshot dump from the UI (command) and headless. | S | V3 |

### Epic P – Performance

Each item reports before/after numbers from G1 and must keep drift + energy tests green.

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| P1 | Remove per-step allocations | Reuse scratch buffers for `old_positions`, `force_results`, `bounce_infos`, `obstacles`, rebuild candidate lists. | S | G1 |
| P2 | Parallel force application + stats | `apply_forces_and_collect_stats` via `par_iter_mut` with a `StatsAccumulator` reduce (deterministic order-independent sums or fixed-chunk reduction to keep seeded runs reproducible). | M | G1 |
| P3 | Thermo field at snapshot cadence | Bin T/P only when a snapshot or metric is due (or accumulate cheap per-cell sums in the apply pass). | S | G1 |
| P4 | Flat cell grid (CSR) | `CellGrid` as counting-sort arrays (`cell_start`, `ball_idx`) rebuilt per step in parallel instead of `Vec<Vec<Cell>>` with per-cell `Vec`s. | M | P1 |
| P5 | Spatial reordering of balls | Periodically sort `balls` by cell (Morton/row order) for cache locality; follow the index contract (clear lists, rebuild grid). | M | P4 |
| P6 | Hot/cold split of `Ball2D` | Hot SoA arrays (pos, v, accel, radius, mass, species) vs cold per-ball data (verbose, tail, counters). Large mechanical change — do after M7 to limit churn. | L | M7, P4 |
| P7 | Snapshot reuse | Double-buffered snapshot with reused allocations; debug data (neighbour lines, tails) only for verbose balls in a separate small vec; status text built in UI from numbers. | M | V2 |
| P8 | Precision in large worlds (#213) | Cell-relative coordinates or f64 positions with f32 kernel offsets; evaluate after P6. | L | P6 |
| P9 | Half neighbour lists (experiment) | Newton's third law to halve pair work; needs per-thread force buffers + reduction. Keep only if G1 shows > 20 % gain at equal determinism. | M | P2, P6 |
| P10 | GPU-side ball buffer | Upload only positions/colours per frame to a persistent instance buffer; instanced draw already exists — verify no per-frame reallocation. | S | V2 |

---

## 4. Suggested order

```text
Phase 1 (guardrails + quick wins):  G1, G2, G3, M1, M3, M4, C1, P1, P3
Phase 2 (extension seams):          I1, I2, C2, C4, M2, M5, P2
Phase 3 (crate split):              M6, M7, M8, M9, V3
Phase 4 (features on new seams):    I3, I5, V1, V2, C3, C5, P4, P7, V5
Phase 5 (bigger features/perf):     I4, V4, V6, V7, P5, P6, P10, I6, I7, P8, P9
```

Interaction work (I1/I2) is deliberately before the crate split: it touches the
hot kernel and is easiest to verify against unchanged baselines. The crate split
then moves already-clean code.

---

## 5. Assumptions

- The project stays 2D and CPU-physics (no GPU compute for forces).
- Determinism of seeded runs on the same target/profile must be preserved; any
  change that alters results needs a drift baseline update per `docs/drift-test.md`.
- Persistence files may get a format version bump with migration; old saves keep loading.
- wgpu stays the only renderer backend.

## 6. Open decisions

1. **Crate split (M7–M9)**: worth the churn, or keep one `core` crate with stricter modules and feature flags?
2. **Species model (I3)**: replace size classes completely, or keep size classes as a species preset?
3. **Plots (V4)**: in the main window (docked panel) or keep a separate window rendered with wgpu?
4. **Half neighbour lists (P9)**: accept a reduction step and its determinism cost, or keep full lists?
5. **Which extra potentials (I4)** matter first?
