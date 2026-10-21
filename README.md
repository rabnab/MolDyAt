# MolDyAt

**Mol**ecular **Dy**namics: **At**oms. A molecular dynamics simulation of a Lennard-Jones gas in a finite box, written in Rust.

MolDyAt was formerly called ThermoSolver (releases up to v1.3.0).

[![Release](https://img.shields.io/github/v/release/rabnab/MolDyAt)](https://github.com/rabnab/MolDyAt/releases/latest)
![Rust](https://img.shields.io/badge/Rust-nightly-orange)

> **Note:** Earlier versions used [ggez](https://github.com/ggez/ggez) as the rendering backend.
> The renderer has since been replaced by a custom wgpu/WGSL pipeline.

## Technologies Used

* **Rust** (nightly) — edition 2024, Cargo workspace
* **wgpu** — GPU-accelerated renderer with custom WGSL shaders
* **winit** — cross-platform window and event loop
* **Rayon** — parallel physics calculations
* **SIMD** — portable `std::simd` force kernels (instruction set fixed at compile time)

## Features

### Physics Engine

* **Lennard-Jones potential** with configurable force capping for energy conservation
* **Verlet neighbor lists** with automatic rebuild based on movement threshold
* **Spatial partitioning** via cell grid for O(N) neighbor lookups
* **Species library**: N₂ (the unit), Ar, Ne, CH₄, O₂ with literature Lennard-Jones parameters, plus the former size classes XS to XL; a config file can define its own species list (see "Simulation config")
* **Physics thread** — simulation runs independently from the render loop

### Operators

Placeable tools from the sidebar:

* **Line segments** — interior walls with momentum bookkeeping
* **Valves** — openable barriers between regions
* **Thermostats** — control the temperature inside a circular region
* **Source spheres** — emit new balls, can be opened and closed

### Persistence

* **Save / load** complete worlds (balls and all operators) as versioned JSON — see [Saving and loading a world](#saving-and-loading-a-world)

### Performance Optimizations

* **Parallelization**: Multi-threaded physics via Rayon (always enabled)
* **SIMD Vectorization**: 2-4× speedup on force calculations via portable `std::simd` (`f32x4`)
  - The instruction set is chosen at **compile time**, not at runtime
  - Release binaries target a portable baseline (`x86-64-v2`, SSE4.2)
  - For maximum speed on your own CPU, build from source with `--release` (uses `target-cpu=native`)
* **Spatial Partitioning**: Cell grid with Verlet neighbor lists

### Renderer

* **RenderGraph architecture** — `RenderPipeline` trait with named pipeline handles
* **6 GPU pipelines** — frame border, checkerboard grid, lines, balls, crosshair, text overlay
* **Instanced ball rendering** — position/radius/color uploaded via GPU storage buffer
* **SDF circles** — anti-aliased ball fill, equilibrium ring (magenta), cutoff ring (gray)
* **Viewport** — zoom (scroll wheel), pan (middle-click drag), reset (`0`)
* **Energy plot window** — dockable overlay toggled at runtime (`TAB`)

## Getting Started

### Download (prebuilt binaries)

Prebuilt binaries for Windows and Linux (x86_64) are attached to each
[release](https://github.com/rabnab/MolDyAt/releases/latest).

**Windows (PowerShell):**

```powershell
$tag = "vX.Y.Z"  # a release from the releases page, newer than v1.3.1
$name = "moldyat-$tag-x86_64-pc-windows-msvc"
Invoke-WebRequest "https://github.com/rabnab/MolDyAt/releases/download/$tag/$name.zip" -OutFile "$name.zip"
Expand-Archive "$name.zip" -DestinationPath .
& ".\$name\moldyat.exe"
```

**Linux:**

```bash
tag=vX.Y.Z  # a release from the releases page, newer than v1.3.1
name=moldyat-$tag-x86_64-unknown-linux-gnu
curl -LO "https://github.com/rabnab/MolDyAt/releases/download/$tag/$name.tar.gz"
tar xzf "$name.tar.gz"
./"$name"/moldyat
```

The archives and the executable are called `moldyat` in releases after v1.3.1;
v1.3.1 and earlier use `rust_thermo_wgpu` instead (adjust `$name` and the
executable name).

Verify a download against its `.sha256` file (`sha256sum -c` on Linux,
`Get-FileHash` in PowerShell).

**System requirements**

* x86_64 CPU with SSE4.2 (x86-64-v2, roughly 2009 or newer)
* GPU with **DirectX 12** (Windows) or **Vulkan** (Linux) support, current drivers
* Override the graphics backend with `WGPU_BACKEND` (e.g. `WGPU_BACKEND=gl`) if
  window creation fails
* Headless mode (`--headless`) needs no GPU

### Building from source

#### Prerequisites

* [Rust nightly toolchain](https://rustup.rs/) — selected automatically via `rust-toolchain.toml`

#### Building & Running

```bash
cargo build -p moldyat_wgpu
cargo run   -p moldyat_wgpu
```

### Headless Mode (Examples)

Headless mode runs the simulation without opening a window and emits structured output for CI, regression checks, scripting, and offline analysis.

```bash
# Minimal headless run (defaults: --steps 1, summary output, stdout enabled)
cargo run -p moldyat_wgpu -- --headless
```

```bash
# Run 500 steps, emit snapshots every 50 steps to stdout
cargo run -p moldyat_wgpu -- --headless --steps 500 --snapshot-every 50
```

```bash
# Write JSONL records to a file while keeping console summaries
cargo run -p moldyat_wgpu -- --headless --steps 300 --snapshot-every 25 --output /tmp/headless.jsonl
```

```bash
# File-only mode (required when disabling stdout)
cargo run -p moldyat_wgpu -- --headless --steps 300 --output /tmp/headless.jsonl --no-stdout
```

```bash
# Deterministic seeded run for reproducible CI fixtures
cargo run -p moldyat_wgpu -- --headless --steps 1000 --snapshot-every 100 --rng-seed 12345 --seed-balls 64 --output /tmp/seeded.jsonl --no-stdout
```

```bash
# Custom time step and interior walls (world coordinates, x1,y1,x2,y2[,thickness]).
# Walls are placed before --seed-balls, so no ball starts inside a wall.
# run_start records the effective dt, the walls and the world_rect bounds.
cargo run -p moldyat_wgpu -- --headless --steps 20000 --snapshot-every 100 --rng-seed 12345 --seed-balls 800 --dt 0.005 --line-segment -30,-20,-30,20 --line-segment -20,15,20,15,3 --output /tmp/walls.jsonl --no-stdout
```

```bash
# Large, hot world for performance runs: --world-scale multiplies the default
# world's width and height, --seed-speed gives seeded balls speeds in 0..max.
# run_start records both.
cargo run --release -p moldyat_wgpu -- --headless --steps 300 --rng-seed 1 --seed-balls 12000 --world-scale 8 --seed-speed 6 --stdout
```

```bash
# Valves: --valve wall,t_start,t_end[,open|closed] cuts a valve into the
# --line-segment with that 0-based index (fractions along it, one per wall).
# --timings writes ms/step per phase as JSON, after --timing-warmup steps.
# --save-setup saves the prepared world to --save-dir before the first step.
cargo run --release -p moldyat_wgpu -- --headless --steps 300 --rng-seed 1 --seed-balls 2000 --line-segment 0,-30,0,30,2 --valve 0,0.4,0.6,closed --timings /tmp/timings.json --timing-warmup 50 --no-stdout --output /tmp/valves.jsonl
```

```bash
# All options. An invalid command line prints the error and this usage (exit 2).
cargo run -p moldyat_wgpu -- --help
```

```bash
# Version and the git commit of the build, e.g. "moldyat <version> (60c70fd)";
# -dirty means the build had uncommitted changes. Also shown in the window title.
cargo run -p moldyat_wgpu -- --version
```

```bash
# Performance baseline: fixed seeded scenarios, one JSON report with ms/step
# per phase. See docs/perf-baseline.md for scenarios and comparing branches.
cargo build --profile perf -p moldyat_wgpu && scripts/headless_perf.sh > perf.json
```

```bash
# Export summary + per-ball state (larger output)
cargo run -p moldyat_wgpu -- --headless --steps 200 --snapshot-every 20 --detail balls --output /tmp/balls.jsonl --no-stdout
```

```bash
# Export thermo grids (temperature + pressure)
cargo run -p moldyat_wgpu -- --headless --steps 200 --snapshot-every 20 --detail thermo --output /tmp/thermo.jsonl --no-stdout
```

```bash
# Export both balls and thermo sections in each frame record
cargo run -p moldyat_wgpu -- --headless --steps 200 --snapshot-every 20 --detail balls+thermo --output /tmp/full.jsonl --no-stdout
```

```bash
# Same as above, using the accepted alias for balls+thermo
cargo run -p moldyat_wgpu -- --headless --steps 200 --snapshot-every 20 --detail balls-and-thermo --output /tmp/full.jsonl --no-stdout
```

```bash
# Capture output in CI artifact path
cargo run -p moldyat_wgpu -- --headless --steps 1500 --snapshot-every 150 --rng-seed 7 --seed-balls 100 --output ./artifacts/headless-run.jsonl --no-stdout
```

```bash
# Quick smoke check: ensure expected record kinds are present
cargo run -p moldyat_wgpu -- --headless --steps 10 --snapshot-every 5 --output /tmp/smoke.jsonl --no-stdout
rg '"record_kind":"run_start"|"record_kind":"frame"|"record_kind":"run_end"' /tmp/smoke.jsonl
```

Key behavior:

- emits an initial frame at step `0`
- emits interval frames when `--snapshot-every > 0`
- always emits a final frame at the last step
- records are JSON Lines with `schema_version = "headless.v2"`
- non-finite numbers (`NaN`, `+Inf`, `-Inf`) are encoded as `null`

Seeded regression fixture workflow (used by CI; required for refactor PRs, see [CONTRIBUTING.md](CONTRIBUTING.md#refactor-prs-required-gates)):

```bash
# Run the fixture regression checks used in CI (the tests are #[ignore]d)
cargo ci-test-headless-regression
```

```bash
# Intentionally refresh the checked-in fixtures after approved contract changes
UPDATE_HEADLESS_FIXTURE=1 cargo ci-test-headless-regression
```

Fixture files:

- `wgpu_renderer/src/headless_runner/fixtures/seeded_headless_regression.jsonl`: free gas, summary records
- `wgpu_renderer/src/headless_runner/fixtures/seeded_headless_operators.jsonl`: walls, a valve, a thermostat and a source sphere, with per-ball state
- Regenerate only when changes to headless output are intentional and reviewed.

### Testing

```bash
cargo test --workspace
```

Suite aliases (centralized in `.cargo/config.toml`):

```bash
# default workspace suite (excludes ignored specialty suites)
cargo ci-test

# specialty suites
cargo ci-test-headless-regression
cargo ci-test-drift
```

Contributor command policy (including targeted test/lint commands) is maintained in
`.github/copilot-instructions.md#quick-build-test-and-lint-commands-explicit`.

### Linting & Formatting

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
```

## Controls

Keyboard (press `H` for the same list in the log):

<!-- key-table:start (generated from COMMANDS, see command_registry.rs) -->
| Key | Action |
|---|---|
| `SPACE` | Pause or resume the simulation. |
| `S` | Toggle step-by-step mode. |
| `G` | Cycle gravity: off -> 9.81 m/s^2 -> 49.05 m/s^2. |
| `=` / `Num +` | Double the simulation time step. |
| `-` / `Num -` | Halve the simulation time step. |
| `Alt + =` / `Alt + Num +` | Decrease the wait time between steps. |
| `Alt + -` / `Alt + Num -` | Increase the wait time between steps. |
| `ESC` | Clear all balls. |
| `F1` | Stop all balls (zero velocities). |
| `F5` | Add 100 random balls. |
| `N` | New balls and new source spheres get the next species (right click too). |
| `Alt + N` | New balls and new source spheres get the previous species. |
| `R` | Randomize all ball speeds. |
| `Shift` | Enable random ball speed. |
| `DEL` / `BACKSPACE` | Delete the highlighted object (hover it with its sidebar tool selected). |
| `Ctrl + S` | Save the world to a new file in the save directory (--save-dir, default ./saves). |
| `Ctrl + O` | Load the newest save from the save directory (paused, default view). |
| `0` / `Num 0` | Reset the viewport (1.0x zoom, centred). |
| `F2` | Toggle rendering. |
| `F3` | Cycle the cutoff-radius rings. |
| `T` | Cycle the thermo overlay: Off -> Grid -> Temperature -> Pressure -> Off. |
| `P` | Cycle the plot content. |
| `TAB` | Toggle the energy plot window. |
| `D` | Toggle the display of real data (SI units, see docs/units.md). |
| `B` | Toggle the ball statistics overlay. |
| `F4` | Toggle step timings in the log. |
| `V` | Set a ball verbose: type its ID (or 'a' for all, '0' for none) and a mode (c console, m main neighbours, o other neighbours, s speed) in the terminal. |
| `H` | Print this list of keys to the log. |
<!-- key-table:end -->

Mouse:

| Mouse | Action |
|---|---|
| Left click on a sidebar icon | Select or deselect that tool |
| Left click (ball tool) | Place a ball at the cursor |
| Right click | Next species for new balls and new source spheres (as `N`) |
| Right click on a source sphere (source tool) | Next species that sphere emits |
| Scroll wheel | Zoom in / out |
| Middle-click drag | Pan viewport |

### Logging

The simulation core never prints to stdout; everything goes through the log
on stderr, at level `info` by default. Set `RUST_LOG` to change the filter:

```bash
# Per-step output of balls in console verbose mode (key V, mode c)
RUST_LOG=info,moldyat::verbose=debug cargo run -p moldyat_wgpu
```

- `moldyat::verbose` (level `debug`): position, velocity and neighbour output of console-verbose balls. Hidden by default.
- `moldyat::timing` (level `info`): step timings while the timing display (`F4`) is on.
- `moldyat::resize` (level `debug`): timings of frames that apply a window resize. Hidden by default.

## Saving and loading a world

`Ctrl+S` writes `world_YYYYMMDD_HHMMSS.json` (UTC, never overwrites an earlier
save) to the save directory, `saves/` unless `--save-dir <dir>` says
otherwise. `Ctrl+O` loads the newest file there. To start from a specific
file, pass `--load <path>`; it works with the window and with `--headless`
(where the loaded world runs immediately and any further headless options such
as `--dt` or `--seed-balls` are applied on top).

```bash
cargo run -p moldyat_wgpu -- --load saves/world_20260101_120000.json
cargo run -p moldyat_wgpu -- --save-dir ~/lj-worlds
```

The file is versioned JSON (`"version": 2`) holding the world rectangle, time
step, gravity, the [simulation config](#simulation-config), every ball (`id`, `species`, `pos`, `v`, `pre_update_pos`,
`accel`) and all operators. Saves of format 1 (size classes) still load: each
size class becomes the species of the same name (`XS` to `XL`). The view
(window size, zoom, pan), the energy plot
and simulation statistics are not saved; a loaded world is shown in the
default view. The window is asked to take the world's shape; if the OS gives
it another size (screen too small, maximised or tiled window), the world is
letterboxed, never stretched or rescaled. A later window resize refits the
world to the window as usual. A file that fails validation is rejected and the running world
is left untouched. Results are logged. Saves from older versions are upgraded
when they are loaded. A save from a newer MolDyAt is rejected if it has a
newer format version or unknown settings in its config; other fields this
build does not know are skipped with a warning (saving again drops them).

## Simulation config

Settings that are not part of the world (time step, speed limit, force cap and
cutoff, Verlet skin, snapshot rate, species) come from a TOML file passed with
`--config <path>`, in the window and with `--headless`. Every section and key
is optional: missing ones keep their default, and without `--config` all
defaults apply, so results are the same as without a config file. Unknown keys
are rejected, so a typo is an error, not a silent default, and so is a value
outside its range (given after the default below). Flags that set the same
value, such as `--dt`, override the file.

The defaults, as a complete file, with each key's accepted range:

```toml
[integration]
time_step = 0.005             # 0.0003125..0.08, reduced time units; + / - change it while running
speed_limit_factor = 0.2      # 0.01..10, speed limit as a share of the shorter world side

[potential]
max_force = 100.0             # 1..10000, repulsive force where the sloped core starts
core_stiffness = 200.0        # 0..10000, force slope inside the core (0: flat)
cutoff_factor = 4.0           # 1.2..10, pair cutoff in units of sigma_ij
## Optional, one block per species pair: replaces the mixed sigma_ij and/or
## epsilon_ij of that pair (both orders); a missing value keeps the mixed one.
# [[potential.pair_override]]
# a = "N2"                    # species names of the run's table (see [[species]])
# b = "Ar"
# sigma = 1.1                 # 0.5..8
# epsilon = 1.2               # 0.1..10

[neighbours]
skin_factor = 0.2             # 0.01..1, Verlet skin as a share of the N2 cutoff

[display]
snapshot_interval_us = 16667  # 1000..1000000, least time between two frames sent to the window

## Optional: the species of the run. Without [[species]] the built-in library
## applies: N2, the former size classes XS, S, M, L, XL, and Ar, Ne, CH4, O2.
## With it, exactly the listed species, in this order; the first is the
## default for new balls. Literature single-site Lennard-Jones values; N2
## (3.698 A, 95.05 K, 28.0134 u) is the unit.
# [[species]]
# name = "N2"                 # unique; saves and pair overrides use it
# sigma_angstrom = 3.698      # 1.849..29.584
# epsilon_kelvin = 95.05      # 9.505..950.5
# mass_u = 28.0134            # 1.40..2801.34
# colour = [0.0, 1.0, 1.0]    # RGB or RGBA, each 0..1
# source = "Hirschfelder, Curtiss & Bird (1954)"   # optional
# [[species]]
# name = "Ar"
# sigma_angstrom = 3.405
# epsilon_kelvin = 119.8
# mass_u = 39.948
# colour = [0.6, 0.45, 1.0]
# display_radius_angstrom = 1.7   # optional, drawing only; default sigma / 2
```

```bash
cargo run -p moldyat_wgpu -- --config my-run.toml
cargo run -p moldyat_wgpu -- --headless --config my-run.toml --dt 0.005 --steps 1000
# N2 + Ar mixture, 1:1, from the species of my-run.toml (or the built-in library)
cargo run -p moldyat_wgpu -- --headless --config my-run.toml --seed-balls 400 --seed-species N2,Ar --rng-seed 1
```

A save stores the config of the run in its `config` section, and loading the
save restores it, so `--config` cannot be combined with `--load`. Saves
written before the config existed load with the defaults. Builds from before
the config ignore it and run such a save with the defaults; a build that does
not know a key in a save's `config` refuses to load it; pair overrides are
written only when there are any, so other saves stay loadable for builds from
before them. The same holds for `[[species]]`: a save of a run with its own
species list carries that list and loads with it; other saves name species of
the built-in library.

## Releases & Versioning

Versions follow [Semantic Versioning](https://semver.org/). Releases are
created by pushing a `vMAJOR.MINOR.PATCH` tag; the `Release` workflow
(`.github/workflows/release.yml`) builds the binaries and publishes the
release with generated notes. See the
[release list](https://github.com/rabnab/MolDyAt/releases)
for changes per version.

Note: the commits before v0.2.0 did not follow a strict versioning scheme, so there are versions in older commits:
- 0.1.0
- 0.1.1
- 1.3.0
- 1.3.1

## License

MolDyAt (formerly ThermoSolver) is licensed under the [Mozilla Public License 2.0](LICENSE)
(MPL-2.0). In short:

* You may use, modify and redistribute it, also commercially and inside
  proprietary products.
* If you distribute a modified version of a file covered by this licence, you
  must make the source of your changes to that file available under the
  MPL-2.0. Your own separate files and the larger product around it can stay
  under any licence.
* The licence and copyright notices must be kept.
* The software is provided as is, without warranty and without liability for
  any damage caused by using the source code or the binaries.

This licence applies to **all versions** of the project, including every
earlier commit, tag and release (v1.3.0 and v1.3.0-rc.2 included).

If you use MolDyAt in scientific work, please cite it; see
[CITATION.cff](CITATION.cff) (GitHub's "Cite this repository" button).

## Project Structure

```
Cargo.toml                     # Workspace root (members: core, wgpu_renderer)
core/
└── src/
    ├── application/           # Simulation controller, physics thread, stats
    │   ├── core/              # Ball management, simulation stepping, operators
    │   │   └── tests/         # Topic-based test files (ball_management, energy, …)
    │   ├── operator_command/  # Valve, thermostat, source commands
    │   ├── persistence/       # Versioned JSON save / load
    │   ├── sim_config.rs      # SimConfig: run settings, --config file
    │   └── physics_thread/    # Physics thread internals
    ├── graphics/
    │   ├── energy_plot/       # Energy plot window
    │   └── renderer/          # Renderer trait, viewport, batch renderer
    │       ├── event_loop/    # InputHandler, PhysicsCoordinator, Metrics, …
    │       └── viewport/      # ViewportTransform, coordinate mapping
    ├── objects2d/             # Ball physics, SIMD forces, Verlet lists, cell grid,
    │                          #   line segments, wall chains, operators
    └── logging/               # env_logger initialisation
wgpu_renderer/
└── src/
    ├── main.rs                # Entry point, Rayon init, window creation
    ├── cli/                   # Command-line parsing
    ├── headless_cli/          # Headless option parsing
    ├── headless_runner/       # Headless simulation driver + regression fixtures
    ├── headless_output/       # JSONL record writer
    └── wgpu/
        ├── wgpu_context.rs    # Device / queue / surface setup, backend choice
        ├── wgpu_window.rs     # winit event loop, frame submission
        ├── render_graph.rs    # RenderGraph orchestrator
        ├── wgpu_pipelines.rs  # Pipeline registry with named handles
        ├── pipelines/         # ball, grid, line, wall, valve, thermostat, sidebar, …
        ├── gpu/               # Textures, embedded icons
        ├── icons/             # Sidebar tool icons (PNG, embedded at compile time)
        └── shaders/           # WGSL shader sources
```

## Documentation

How the application works:

* [Runtime Architecture](docs/architecture.md)
* [Simulation Coordinate Systems](docs/simulation.md)
* [Reduced Units and Scales](docs/units.md)
* [Operators](docs/operators.md)
* [Headless Mode](docs/headless.md)
* [Drift Tests](docs/drift-test.md)
* [Performance Baseline](docs/perf-baseline.md)

Plans and history:

* [Refactoring Plan](docs/plan/refactoring-plan.md): open work
* [`docs/done/`](docs/done/): implemented plans and reviews, kept as history

Contributing:

* [Contributing Guide](CONTRIBUTING.md)
* [Copilot Instructions](.github/copilot-instructions.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow, code quality standards, and PR process.

## Future Plans

* Hierarchical particles for simulating thousands of particles/atoms
* Dockable log window
