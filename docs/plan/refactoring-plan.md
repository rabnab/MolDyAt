# Refactoring Plan: Maintainability, Extensibility, Performance

Status: accepted direction, decisions recorded 2026-10-04 (see §6). Phases
1–3 have landed, and with M9 the crate split; the remaining work was
regrouped into Phases 4–8 on 2026-10-23, with every work item and open
issue assigned to a phase (see §4). Successor to the runtime refactor in
[refactor_architecture.md](../done/refactor_architecture.md) (#137, landed). The current
structure is described in [architecture.md](../architecture.md).

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

This is the analysis from 2026-10-04, before Phase 1. Since then the legacy
traits (M1), core `println!`s (M4), duplicated timing branches (M3), per-step
allocations (P1), per-step T/P binning (P3) and the crate-wide
`#![allow(dead_code)]` (G3) are gone, `DomainCommand` is
grouped (C1), and `scirs2-interpolate` was dropped (#247). See §4 for the
current status.

~38.5k lines of Rust in two crates.

| Area | Where | Observation |
|---|---|---|
| Crate layout | `core` (`moldyat_core`, split into `thermo_physics`, `thermo_runtime` and `thermo_ui` by M7–M9) | Held physics **and** renderer-agnostic UI (event loop, viewport, tools, sidebar) **and** a winit/softbuffer energy window, plus `scirs2-interpolate`/`ndarray`. Headless runs and physics tests pulled in winit and heavy deps. |
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
| Open bugs touching the plan | #213 (f32 precision in large worlds), #216 (operators/mouse must always render), #217 (resize lag) | Addressed by P8, V4 and V1 respectively, or at least made easier (#216 and #217 are closed; P8 is now P8a/P8b). |

Strengths to keep: drift harness and energy tests, seeded headless regression
fixture, `OperatorCommand` / `OperatorToolHandler` traits, `GpuBatch` pattern,
lock-free snapshot slot, documented index contracts.

---

## 2. Target architecture

`thermo_physics` exists since M7 (#308), `thermo_runtime` since M8 (#309)
and `thermo_ui` since M9 (#310); `thermo_wgpu` is still `moldyat_wgpu` in
`wgpu_renderer/`.

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
| G2 | Golden-output guard for refactors | Make the seeded headless regression fixture and drift suite the required gate for every refactor PR (document in CONTRIBUTING). Add a fixture for a scene with operators (walls, valve, thermostat, source). Add a reproducibility test: same seed with 1 and N Rayon threads gives bit-identical state. | S | – |
| G3 | Remove crate-wide `#![allow(dead_code)]` | Delete dead code or add local, justified allows. Clippy clean. | M | – |

### Epic M – Maintainability

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| M1 | Replace legacy single-impl traits | Turn `GameLogic`, `LogStuff`, `VerletIntegration` into inherent methods/free functions; `GameLogicData` → `StepConfig`. No behaviour change (G2 green). | S | G2 |
| M2 | Move UI/debug flags out of `SimulationWorld` | `show_timings`, `show_ball_stats`, `real_data`, `step_by_step` → `RuntimeOptions` in `MainContent`. World holds physical state only. Persistence format unaffected or migrated. Done (#305): `RuntimeOptions` (`application/core/runtime_options.rs`) in `MainContent`, kept across loads, never saved; the engine takes it per step and passes only the timing-log flag to the phases; `was_paused_for_resize` moved to `PhysicsRuntimeState`. | S | M1 |
| M3 | Unify timing instrumentation | `PhaseClock` is the only step timer: each phase boundary is one lap that feeds G1's phase report (`PhaseTimings`); `TimingMonad` and the duplicated `#[cfg]` branches in `simulation_step_with_dt` are removed. `measure-time` only gates the `F4` timing log. | S | G1 |
| M4 | Replace `println!` in core with `log` / verbose sink | Verbose-ball output goes through a `log` target (`moldyat::verbose`) or the snapshot. | S | – |
| M5 | Split oversized modules | `core/line_segments.rs` (obstacles, sweep, overlap fallback), `unified_render_state.rs`, `line_segment_tool.rs`, `energy_renderer.rs`. Target < 400 lines per file; tests follow the topic-file rule. | M | – |
| M6 | Central `SimConfig` | Collect `DT_GOAL`, `FPS_GOAL`, LJ defaults, skin, speed limit factor, snapshot interval into one serde struct; CLI and save files read/override it. | M | M1 |
| M7 | Crate split: `thermo_physics` | Move `objects2d`, `application/core` (world, engine), persistence into a crate without winit/softbuffer/scirs2. Keep `moldyat_core` as a re-export shim for one release. | L | M1, M2 |
| M8 | Crate split: `thermo_runtime` + headless | Physics thread, `DomainCommand`, snapshot, headless runner/output move out of the GPU crate into `crates/thermo_runtime`, which builds without a windowing stack. Tighten the `thermo_physics` API that M7 widened (about 100 former `pub(crate)` items made `pub`, among them `SimulationWorld` fields): keep `pub` only what the runtime and UI use, behind accessors or narrow methods instead of public fields. Done (#309): the physics thread, commands, snapshots and headless runs are in `thermo_runtime`; `SimulationWorld` has accessors instead of public fields, and `thermo_physics` exposes only what the other crates use (831 → 505 public items, see `docs/architecture.md`). | L | M7 |
| M9 | Crate split: `thermo_ui` | After M8 `moldyat_core` holds only the UI layer (event loop, tools, viewport, sidebar model, energy window) and the logger setup. Move it to `crates/thermo_ui`, rename the package, and remove the re-exports kept for one release by #308 and #309 (callers use `thermo_physics` / `thermo_runtime` directly). The energy window moves as it is; it becomes a wgpu window and `softbuffer` is dropped in V4a. Done (#310): `crates/thermo_ui`, no re-exports between the crates. | S | M8 |
| M10 | Docs refresh | Update `architecture.md`, move finished plans to `docs/done/` (done for `refactor_architecture.md`, `headless-renderer-plan.md`, `nose-hoover.md`, `operator-handling-review.md`), update the `operator-framework` skill. Recurring, part of every PR (decision 12): each PR updates the docs, skills and project rules its change touches, and this plan's status and issue table (`plan-item-workflow` skill, step 5). No issue or phase slot of its own. | – | every PR |

### Epic I – Interaction types and mixed species

Priority: mixed-species simulation is needed soon, so I1–I7 come right after the
guardrails. Water (I9–I10) follows. See §5 for the model choice behind it.

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| I1 | Extract `PairPotential` from `BallPhysics` | Trait/enum with `force(r)`, `potential(r)`, `force_simd(f32x4)`, `potential_simd`, `cutoff()`; capping (`r_cap`, sloped core with quadratic potential) and cutoff shift implemented **once**, generic over the potential. LJ becomes `LennardJones { sigma, epsilon }`. Bit-identical drift baselines (same target/profile). | M | G2 |
| I2 | Static dispatch for the SIMD kernel | `force_simd` generic over the potential (enum match outside the neighbour loop, monomorphised inner loop). Perf within ±2 % of G1. | M | I1 |
| I3 | Species replace size classes; LJ on centre distance | `Species { name, mass, sigma, epsilon, display_radius, colour }` replaces `size_idx` everywhere (balls, factory, snapshot, renderer colours, persistence). Interaction is standard Lennard-Jones on the **centre distance** with `σ_ij`; `display_radius` is adjustable and used **only for rendering**. Every physics use of `Ball2D::radius` (~20 files: force kernel, wall sweep/contact, line-wall overlap, cell size, cutoff, thermostat/source overlap checks, placement collision) switches to species data: interaction range from `σ_ij`, hard wall contact at centre distance `σ_i / 2` from the wall (decision 8), behind a small `WallLaw` interface (hard contact only for now) so I12 can add a soft wall without touching the call sites. `KernelPoint` carries the species index instead of the radius. **Intentional behaviour change:** recapture drift baselines per `docs/drift-test.md`; update the "Radius awareness" rule in `.github/copilot-instructions.md`. Old saves migrate size classes to species with `σ` chosen so the old equilibrium centre distance is kept (`σ_i = 2·R_i·2^(-1/6) + σ_N2` keeps it exactly under arithmetic mixing). Includes the I4 table core (decision 9); `N2` is the default species (decision 11). | L | I1, I2 |
| I4 | Interaction table and mixing rules | The table core landed with I3 (decision 9): `PairTable` (N×N `CappedPotential`) built with Lorentz–Berthelot (`σ_ij = (σ_i+σ_j)/2`, `ε_ij = √(ε_i ε_j)`), per-pair cutoff, `r_cap` and `U(cutoff)` cached, per-pair list radius, SIMD gather per chunk, scalar ↔ SIMD parity with mixed species in one chunk. The rest landed with #235: per-pair overrides of `σ_ij` / `ε_ij` from the config (`[[potential.pair_override]]`), binary-mixture drift scenario (energy). | S | I3, M6 |
| I5 | Unit system with N₂ as reference | Reduced units stay internal with N₂ as the unit: `σ_N₂ = 1`, `ε_N₂ = 1`, `m_N₂ = 1` (28.01 amu). With σ ≈ 3.7 Å and ε/k_B ≈ 95 K the time unit is τ = σ·√(m/ε) ≈ 2.2 ps and the temperature unit 95 K (final values from one cited source, recorded in `docs/units.md`). `UnitSystem` converts literature parameters of other species and display values (T in K, time in ps, length in Å). Current default balls (σ = 1, ε = 1) become N₂ directly. Done (#236): `UnitSystem::N2` in `application/units.rs`, Hirschfelder, Curtiss & Bird (1954); the real-units display uses it. | M | I3, M6 |
| I6 | Species library | Species defined in the config file (M6): N₂, Ar, Ne, CH₄, O₂ (single-site LJ) plus the `legacy` preset. Done (#237): the built-in table is the library (`N2`, the size classes `XS`–`XL` as the legacy species, then `Ar`, `Ne`, `CH4`, `O2` from Hirschfelder, Curtiss & Bird 1954); a config's `[[species]]` list (Å, K, u) replaces the whole table and travels with saves; headless `--seed-species` seeds mixtures. | S | I4, I5 |
| I7 | Mixed-species UI and metrics | Species picker (sidebar or keys via C2), colour per species, source spheres emit a chosen species, per-species count / temperature as metric series (V3). Done (#238): `N` / `Alt + N` (`balls.next_species` / `balls.previous_species`) and right click pick the species of new balls and new source spheres; it is shown on the sidebar ball icon, in the cursor readout and in the status lines with the ball count per species; each source sphere is labelled with its species. Per-species time series wait for the metrics store (V3, Phase 5). | M | I3, C2 |
| I8a | Gravitational potential energy in the stats | The reported potential energy holds only the pair energy (`ForceResult::potential_energy`), so with gravity on the shown total energy is not conserved; only the tests in `barrier_energy.rs` add `m·g·y`. Add the gravity term to the potential energy (or as its own series for V3) and an energy-conservation test with gravity. Gravity defaults to 0, so default runs and fixtures are unchanged; outputs of runs with gravity change (document in the PR). Done (#334): with gravity on, a pass at the end of the step adds `m·g·h` (height above the bottom wall) to the potential energy. | S | – |
| I8b | `ExternalField` abstraction | Gravity becomes `UniformField`; enables traps or other fields; applied in the kick with the potential-energy accounting from I8a. | M | I1, I8a |
| I9 | Orientational degrees of freedom | Optional per-species orientation θ, angular velocity ω, moment of inertia; rotational velocity-Verlet step; rotational kinetic energy in stats, temperature and the Nosé–Hoover thermostat. Prerequisite for water. Deterministic (no new reductions). One track with I10: start only once the water model is decided (§7). | L | I3 |
| I10 | 2D water: Mercedes-Benz model (pending decision) | LJ core + three hydrogen-bond arms (Gaussian in distance and arm alignment), parameters from Silverstein, Haymet & Dill (1998). Torques from the same potential. Tests: energy conservation, force/torque vs finite differences, smoke test for the density maximum. Water–N₂ via the LJ part only (mixing rules). | L | I9, I4 |
| I11 | Multi-site molecules (only if needed later) | Rigid two-site N₂ or atomistic water (SPC/E, TIP4P) need molecules, constraints (SHAKE/RATTLE) and electrostatics (damped shifted force). Not needed for mixed species or MB water. | L | I9 |
| I12 | Soft wall potential with hard-contact backstop | `WallLaw` gets a soft variant `U_w(z)` (z = distance from the wall): WCA-type by default, optional attraction `ε_w` (adsorption, wetting). The wall is a pseudo-species in the I4 table (`σ_iw`, `ε_iw` by mixing or override). Signed distance for box walls and closed chains, so the force points inward after an overshoot. The existing hard contact (`wall_contact`) stays as a **backstop** at a deeper distance (e.g. `z ≈ 0.3·σ_i`, where `U_w ≫ k_B T`): a soft wall alone does not stop crossing (finite barrier with the force cap, discrete steps, two-sided segments flip the force), so thin segments and valves rely on it. Backstop hits are counted like cap hits; the drift suite expects none in normal scenarios. Wall pressure from the force sum; no energy bumps at chain joints; walls in the cell grid. Restitution < 1 stays hard-wall only or gets a documented damping term. Tests: energy conservation (new drift scenario), no ball behind a wall in a hot-gas scenario or with injected fast balls, continuity across joints, thread-count reproducibility. | L | I3, I4 |

### Epic C – Input commands

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| C1 | Group `DomainCommand` | Split into `Sim(SimControl)`, `Balls(BallCommand)`, `World(WorldCommand)`, `Operator(Box<dyn OperatorCommand>)`. Handler delegates per group. | S | – |
| C2 | Command registry | One table of `CommandDescriptor { id, label, category, default_binding, kind: Ui(UiAction) | Domain(fn(&Ctx)->DomainCommand) }`. Generated: key dispatch, help text (`PrintHelp`), `process_action_common`. Adding a command = one entry + handler. Unit test: no duplicate bindings, every command has help text. Landed as `command_registry.rs` with `Domain(fn() -> DomainCommand)`: no key command needs a context yet (C5 adds parameters). | M | C1 |
| C3 | Configurable key bindings | Load overrides from a TOML file; defaults from C2. Bindings are a UI setting: a UI settings file read by the UI crate, not a `SimConfig` key (`SimConfig` lives in `thermo_physics` and travels with saves). Done (#335): `[key_bindings]` in `ui.toml` (working directory) or `--ui-config`, applied as a `KeyMap` (`key_map.rs`) that the key dispatch and the help text read; key names as the help text shows them (`key_names.rs`). | S | C2 |
| C4 | Tool registry | `ToolDescriptor { tool, icon, name }` list drives `OperatorTool`, `operator_tool_for_icon`, `selected_sidebar_icon` and the atlas order — removes the 3-file sync rule from the skill. Test: icon order == registry order. | M | – |
| C5 | Scriptable commands for headless | Headless runner accepts a command script (`step 1000; add_random 500; toggle_gravity; …`) mapped through the same registry ids. Enables reproducible scenario tests. The registry (`command_registry.rs`) is in the UI crate, while headless runs are in `thermo_runtime`: the domain half (command id → `DomainCommand`) moves into `thermo_runtime`, and the UI registry refers to it by id. Landed as `--script` (#340): `DOMAIN_COMMANDS` in `thermo_runtime::application::command_table`, `step N` lines, `operator.*` commands for the sidebar tools, `command` records; both headless fixtures are now checked-in args/script/config files. | M | C2, M8 |

### Epic V – Visuals and graphs

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| V1 | ~~Layer registry~~ | Folded into V2. Mostly done another way: `PipelineHandles` is gone, `WgpuPipelines::new` is one list in draw order, and `SceneLayers` selects which layers are built and drawn; #216 and #217 are closed. | – | – |
| V2 | Split `PrepareContext` / snapshot into sections, snapshot reuse | `RendererSnapshot` → `{ frame: FrameInfo, balls: BallBuffer, operators: OperatorSnapshot, fields: FieldSnapshot, debug: Option<DebugSnapshot> }` (`OperatorSnapshot` exists already); pipelines read only their section. New visual = new section or derived data, no new top-level fields. Includes the rest of V1 (a descriptor per layer, layer toggles as commands via C2) and P7 (reused snapshot allocations, debug data only for verbose balls, status text built in the UI from numbers). | M | C2 |
| V3 | Metrics/time-series store | `MetricsRegistry` in runtime: named series (E_kin, E_pot, E_tot, T, P, wall momentum, neighbour count, step time, per-species values, …) in ring buffers, sampled at snapshot cadence. `EnergyData` becomes one consumer. Headless JSONL output reads the same store. Unblocks the per-species series of I7. | M | M8 |
| V4a | Plot window (wgpu) | Separate OS window rendered with wgpu (shared device, own surface) replacing the softbuffer character plot: a line-plot pipeline with axes and ticks for the E_kin / E_pot / E_tot series of V3. `softbuffer` is dropped. Window placement keeps the current split-screen layout. Done (#338): `PlotScene` layout in `thermo_ui`, `PlotSurface` / `PlotPipeline` in the binary; see `docs/architecture.md#energy-plot-window`. | M | V3, M9 |
| V4b | Plot selection and layout | Any selected series from V3, several plots per window, legend, autoscale. Done (#339): `PlotSelection`, up to 4 plots stacked on a shared time axis, series picked with `,` / `.`, `Ctrl/Alt + P`, plots with `Ctrl/Alt + L`, focus with `F` or a click, a click on a legend entry hides a series; see `docs/architecture.md#energy-plot-window`. | M | V4a |
| V5 | Field visualisations | Generalise thermo heatmap to any scalar per-cell field (T, P, density, velocity magnitude) and add a vector field (velocity arrows). Field selection cycles through registry. Builds on the thermo grid of its own from #290, so the field resolution does not change with the species present. | M | V2, #290 |
| V6 | Distribution graphs | Speed histogram vs. Maxwell–Boltzmann, radial distribution function g(r) — computed off the hot path at snapshot cadence. Headless output and export (V7) first; the plot needs V4b. | M | V3, V7 (V4b for the plot) |
| V7 | Data export | CSV/JSONL export of selected series and a snapshot dump from the UI (command) and headless. Done (#337): `thermo_runtime::export` (`Ctrl + E` / `Ctrl + J` series, `Alt + E` / `Alt + J` per-ball snapshot, `--export-dir`, `--export-format`, `--export-series`); see `docs/headless.md#data-export`. | S | V3 |

### Epic P – Performance

Each item reports before/after numbers from G1 and must keep drift + energy tests green.

| ID | Title | Scope / acceptance | Size | Deps |
|---|---|---|---|---|
| P1 | Remove per-step allocations | Reuse scratch buffers for `old_positions`, `force_results`, `bounce_infos`, `obstacles`, rebuild candidate lists. | S | G1 |
| P2 | Parallel force application + stats | `apply_forces_and_collect_stats` via `par_iter_mut`; stats reduced over **fixed-size chunks** combined in chunk order, so sums do not depend on thread count or scheduling (reproducibility test from G2 must pass). | M | G1, G2 |
| P3 | Thermo field at snapshot cadence | Bin T/P every `THERMO_SAMPLE_STRIDE` steps instead of every step; the rolling window counts samples and still spans 1000 steps. A fixed step stride (not wall-clock snapshot time) keeps headless runs deterministic. | S | G1 |
| P4 | Flat cell grid (CSR) and spatial reordering | `CellGrid` as counting-sort arrays (`cell_start`, `ball_idx`) rebuilt per step in parallel instead of `Vec<Vec<Cell>>` with per-cell `Vec`s. Includes P5: the counting sort gives the cell order, so periodically sort `balls` by it for cache locality, following the index contract (clear lists, rebuild grid). | M | P1 |
| P5 | ~~Spatial reordering of balls~~ | Folded into P4. | – | – |
| P6 | Hot/cold split of `Ball2D` | Hot SoA arrays (pos, v, accel, mass, species) vs cold per-ball data (verbose, tail, counters). Large mechanical change, after the crate split to limit churn. | L | M7, P4 |
| P7 | ~~Snapshot reuse~~ | Folded into V2. The snapshot phase is about 1 % of a step even at 200k balls (`docs/perf-baseline.md`), too little for its own item. | – | – |
| P8a | Measure precision loss in large worlds (#213) | A test or headless scenario that shows how far f32 positions and drift degrade with world size and distance from the origin, so P8b is sized by numbers. | S | – |
| P8b | Precision in large worlds (#213) | Cell-relative coordinates or f64 positions with f32 kernel offsets, as P8a shows is needed. | L | P6, P8a |
| P9 | ~~Half neighbour lists~~ | Dropped: the force reduction it needs makes results depend on thread scheduling, and reproducibility is required. | – | – |
| P10 | ~~GPU-side ball buffer~~ | Done: `GpuBatch::flush` writes into a persistent buffer and reallocates only when the element count outgrows it (to the next power of two). | – | – |

---

## 4. Suggested order

```text
Phase 1 (guardrails + quick wins):  G1, G2, G3, M1, M3, M4, C1, P1, P3
Phase 2 (mixed species):            I1, I2, I3, I4, M6, I5, I6, C2, I7
Phase 3 (crate split):              M2, #295, #270, #273, #274, M5, C4, M7, M8, P2
Phase 4 (finish the split):         M9, I8a, #327, C3
Phase 5 (metrics + plots):          V3, V7, V4a, V4b, C5
Phase 6 (visuals + tools):          V2, #290, V5, V6, #303, #203
Phase 7 (performance):              P8a, P4, #250, P6, P8b
Phase 8 (walls + water):            I12, I8b, I9 + I10 (once §7 is decided), I11 (if needed)
```

Every ID of §3 is in this order, except these, which need no work of their own:

| ID | Status |
|---|---|
| V1 | Folded into V2 (Phase 6). |
| P5 | Folded into P4 (Phase 7). |
| P7 | Folded into V2 (Phase 6). |
| P9 | Dropped (reproducibility, decision 4). |
| P10 | Done. |
| M10 | Part of every PR (see §3). |

GitHub issues (every open issue is in a phase, as of 2026-10-23):

| Phase | Parent | Landed on `master` | Open |
|---|---|---|---|
| 1 | #219 | G1 #221, G2 #222, G3 #223, M1 #224, M3 #225, M4 #226, C1 #227, P1 #228, P3 #229 | – |
| 2 | #220 | I1 #230, I2 #233, I3 #234, M6 #231, I4 #235, I5 #236, C2 #232, I6 #237, I7 #238 | – |
| 3 | #304 | M2 #305, #295, #270, #273, #274, M5 #306, C4 #307, P2 #311, M7 #308, M8 #309 | – |
| 4 | #329 | M9 #310 | I8a #334, #327, C3 #335, M10 |
| 5 | #330 | V3 #336, V7 #337, V4a #338, #367, #366, V4b #339, #371, #372, C5 #340 | M10 |
| 6 | #331 | – | V2 #341, #290, V5 #342, V6 #343, #303, #203, M10 |
| 7 | #332 | – | P8a #344, P4 #345, #250, P6 #346, P8b #213, M10 |
| 8 | #333 | – | I12 #280, I8b #347, I9 #348, I10 #349, I11 #350, M10 |

"Landed" is taken from the commits on `master` (as of 2026-10-23), not from
the issue states. Every item has an issue except M10, which is part of
every PR. A new issue goes into
the phase whose items it depends on or touches; add it to both lists above
and as a sub-issue of that phase's parent.

Phases 4–8 replace the former Phases 4 and 5 (2026-10-23), which mixed
crate work, graphs, visuals, performance and water in two long lists:

- **Phase 4** finished the crate split. After M8 `moldyat_core` held only
  the UI, so M9 moved and renamed it to `thermo_ui`. I8a fixes the total
  energy shown with gravity on; C3 needs nothing else. #327 (`+` / `-` do
  nothing on non-US layouts) comes right before C3: it decides whether
  bindings match physical or logical keys, which is the type C3 loads
  from the bindings file.
- **Phase 5** builds the metrics store first; export (V7), the wgpu plot
  window (V4a/V4b, which drops `softbuffer`) and headless command scripts
  (C5) build on it and on the runtime crate.
- **Phase 6** reshapes the snapshot (V2 with the rest of V1 and P7) before
  new visuals; #290 gives the heatmap its own grid before V5 generalises it.
- **Phase 7** measures the f32 precision loss (P8a) before the large
  changes; the flat cell grid (P4, with P5) comes before the hot/cold split
  (P6), and P8b only if P8a shows it is needed.
- **Phase 8** holds the physics extensions. I12 has no open prerequisites
  and can be pulled forward; I9 and I10 wait for the water-model decision
  (§7).
- **M10** is not a step of its own: every PR keeps the docs, skills and
  this plan in line with its change, so they never lag behind a phase.

Mixed species (Phase 2) lands before the crate split because it is needed soon and
touches the hot kernel; it is easiest to verify against unchanged baselines. The
crate split then moves already-clean code. I7 needs the command registry (C2),
so C2 moves into Phase 2.

---

## 5. Interaction models for N₂ and water

Assumption: the simulation stays 2D, so literature parameters (fitted in 3D)
give qualitatively right, not quantitatively exact, behaviour.

| Substance | Recommended model | Why | Needs |
|---|---|---|---|
| N₂ | Single-site LJ, σ ≈ 3.7 Å, ε/k_B ≈ 95 K (published fits range ≈ 70–100 K depending on the fitted property) | Small, nearly spherical, weak quadrupole; single-site LJ reproduces gas behaviour well. Same machinery as the current balls. | I3–I6 |
| N₂ (detailed) | Two-site LJ + quadrupole partial charges (e.g. TraPPE) | Only for liquid-structure or orientation effects. | I11 |
| Water (2D) | **Mercedes-Benz (MB) model**: LJ disk + three H-bond arms | Designed for 2D; reproduces density anomaly, H-bond network and hydrophobic effect. Single particle, short-range, no charges, cutoff-friendly → fits cell grid, Verlet lists and SIMD design. | I9, I10 |
| Water (alternative) | mW (Stillinger–Weber three-body) | Single-site, no orientation, but its 109.47° angle term is tuned for 3D tetrahedra; would need re-tuning in 2D. | three-body kernel |
| Water (atomistic) | SPC/E, TIP4P/2005 | Charges, rigid geometry and long-range electrostatics; expensive and mostly 3D-specific. | I11 |
| Water–N₂ | LJ part via Lorentz–Berthelot; H-bond arms act only water–water | N₂ is a hydrophobic solute — the classic MB-model use case. | I4, I10 |

Pure LJ for water is **not** recommended: it gives a simple liquid without
hydrogen bonding, so none of water's characteristic behaviour appears.

---

## 6. Assumptions and decisions

- The project stays 2D and CPU-physics (no GPU compute for forces).
- Determinism of seeded runs on the same target/profile must be preserved; any
  change that alters results needs a drift baseline update per `docs/drift-test.md`.
- Persistence files may get a format version bump with migration; old saves keep loading.
- wgpu stays the only renderer backend.

Decisions (2026-10-04):

1. Split into four crates (M7–M9).
2. Particle types (species) replace size classes; old saves migrate (I3).
3. Plots move to a separate wgpu window (V4, now V4a/V4b).
4. Seeded runs must stay reproducible, including across thread counts (G2, P2; P9 dropped).
5. Mixed-species simulation has priority (Phase 2); N₂ and water models per §5.
6. N₂ defines the reduced units (I5).
7. Interaction is Lennard-Jones on the centre distance; the radius is adjustable but visual only (I3).

Decisions (2026-10-17):

8. Hard wall contact at centre distance `σ_i / 2` (I3): it matches the closest approach of two LJ balls (≈ 0.97 σ at T* = 1). `2^(1/6)·σ_i / 2` would only fit purely repulsive (WCA) balls. A soft wall potential is a separate item (I12) and keeps the hard contact as a backstop.
9. The per-pair interaction table moves from I4 into I3: the migrated size classes need `σ_ij` per pair (arithmetic mixing keeps every old equilibrium distance), and every pair has its own `r_cap`.
10. Cutoff `4·σ_ij` per pair; the cell grid is sized for the species present in the world, not for the whole table, so worlds without large species keep small cells.
11. A built-in `N2` species (`σ = ε = m = 1`) is the default for new balls and source spheres; the size classes `XS`–`XL` stay as built-in species for migrated saves. With `M` as default, every default world would have had 2.3x more neighbour pairs.

Decisions (2026-10-23):

12. Docs refresh (M10) is part of every PR, not an item at the end of a phase: each PR updates the docs, skills, project rules and this plan its change touches.

## 7. Open questions

1. Water model: Mercedes-Benz (recommended) or atomistic water (I11)? Pending: owner is reading up on the MB model.
