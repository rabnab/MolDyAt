# ThermoSolver

A molecular dynamics simulation of a Lennard-Jones gas in a finite box, written in Rust.

[![Release](https://img.shields.io/github/v/release/rabnab/molecularLenardJones)](https://github.com/rabnab/molecularLenardJones/releases/latest)
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
* **5 ball size classes** with radius-aware interactions
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
[release](https://github.com/rabnab/molecularLenardJones/releases/latest).

**Windows (PowerShell):**

```powershell
$tag = "v1.3.0"
$name = "rust_thermo_wgpu-$tag-x86_64-pc-windows-msvc"
Invoke-WebRequest "https://github.com/rabnab/molecularLenardJones/releases/download/$tag/$name.zip" -OutFile "$name.zip"
Expand-Archive "$name.zip" -DestinationPath .
& ".\$name\rust_thermo_wgpu.exe"
```

**Linux:**

```bash
tag=v1.3.0
name=rust_thermo_wgpu-$tag-x86_64-unknown-linux-gnu
curl -LO "https://github.com/rabnab/molecularLenardJones/releases/download/$tag/$name.tar.gz"
tar xzf "$name.tar.gz"
./"$name"/rust_thermo_wgpu
```

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
cargo build -p rust_thermo_wgpu
cargo run   -p rust_thermo_wgpu
```

### Headless Mode (Examples)

Headless mode runs the simulation without opening a window and emits structured output for CI, regression checks, scripting, and offline analysis.

```bash
# Minimal headless run (defaults: --steps 1, summary output, stdout enabled)
cargo run -p rust_thermo_wgpu -- --headless
```

```bash
# Run 500 steps, emit snapshots every 50 steps to stdout
cargo run -p rust_thermo_wgpu -- --headless --steps 500 --snapshot-every 50
```

```bash
# Write JSONL records to a file while keeping console summaries
cargo run -p rust_thermo_wgpu -- --headless --steps 300 --snapshot-every 25 --output /tmp/headless.jsonl
```

```bash
# File-only mode (required when disabling stdout)
cargo run -p rust_thermo_wgpu -- --headless --steps 300 --output /tmp/headless.jsonl --no-stdout
```

```bash
# Deterministic seeded run for reproducible CI fixtures
cargo run -p rust_thermo_wgpu -- --headless --steps 1000 --snapshot-every 100 --rng-seed 12345 --seed-balls 64 --output /tmp/seeded.jsonl --no-stdout
```

```bash
# Custom time step and interior walls (world coordinates, x1,y1,x2,y2[,thickness]).
# Walls are placed before --seed-balls, so no ball starts inside a wall.
# run_start records the effective dt, the walls and the world_rect bounds.
cargo run -p rust_thermo_wgpu -- --headless --steps 20000 --snapshot-every 100 --rng-seed 12345 --seed-balls 800 --dt 0.005 --line-segment -30,-20,-30,20 --line-segment -20,15,20,15,3 --output /tmp/walls.jsonl --no-stdout
```

```bash
# Large, hot world for performance runs: --world-scale multiplies the default
# world's width and height, --seed-speed gives seeded balls speeds in 0..max.
# run_start records both. scripts/headless_perf.sh times such runs per step.
cargo run --release -p rust_thermo_wgpu -- --headless --steps 300 --rng-seed 1 --seed-balls 12000 --world-scale 8 --seed-speed 6 --stdout
```

```bash
# Export summary + per-ball state (larger output)
cargo run -p rust_thermo_wgpu -- --headless --steps 200 --snapshot-every 20 --detail balls --output /tmp/balls.jsonl --no-stdout
```

```bash
# Export thermo grids (temperature + pressure)
cargo run -p rust_thermo_wgpu -- --headless --steps 200 --snapshot-every 20 --detail thermo --output /tmp/thermo.jsonl --no-stdout
```

```bash
# Export both balls and thermo sections in each frame record
cargo run -p rust_thermo_wgpu -- --headless --steps 200 --snapshot-every 20 --detail balls+thermo --output /tmp/full.jsonl --no-stdout
```

```bash
# Same as above, using the accepted alias for balls+thermo
cargo run -p rust_thermo_wgpu -- --headless --steps 200 --snapshot-every 20 --detail balls-and-thermo --output /tmp/full.jsonl --no-stdout
```

```bash
# Capture output in CI artifact path
cargo run -p rust_thermo_wgpu -- --headless --steps 1500 --snapshot-every 150 --rng-seed 7 --seed-balls 100 --output ./artifacts/headless-run.jsonl --no-stdout
```

```bash
# Quick smoke check: ensure expected record kinds are present
cargo run -p rust_thermo_wgpu -- --headless --steps 10 --snapshot-every 5 --output /tmp/smoke.jsonl --no-stdout
rg '"record_kind":"run_start"|"record_kind":"frame"|"record_kind":"run_end"' /tmp/smoke.jsonl
```

Key behavior:

- emits an initial frame at step `0`
- emits interval frames when `--snapshot-every > 0`
- always emits a final frame at the last step
- records are JSON Lines with `schema_version = "headless.v1"`
- non-finite numbers (`NaN`, `+Inf`, `-Inf`) are encoded as `null`

Seeded regression fixture workflow (used by CI):

```bash
# Run the fixture regression check used in CI
cargo test -p rust_thermo_wgpu seeded_headless_regression_fixture_matches_checked_in_jsonl
```

```bash
# Intentionally refresh the checked-in fixture after approved contract changes
UPDATE_HEADLESS_FIXTURE=1 cargo test -p rust_thermo_wgpu seeded_headless_regression_fixture_matches_checked_in_jsonl
```

Fixture file:

- `wgpu_renderer/src/headless_runner/fixtures/seeded_headless_regression.jsonl`
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

| Key / Mouse | Action |
|---|---|
| `Ctrl` (hold) | Show placement crosshair |
| `Ctrl` (release) | Place ball at cursor |
| `Shift` (hold while placing) | Place ball with random velocity |
| `Tab` | Toggle energy plot window |
| `Space` | Pause / resume simulation |
| `Escape` | Clear all balls |
| `F1` | Stop all balls (zero velocities) |
| `F3` | Toggle cutoff-radius rings |
| `F5` | Add 100 random balls |
| `+` / `=` | Double simulation time step |
| `-` | Halve simulation time step |
| `Alt` + `+` | Decrease wait time between steps |
| `Alt` + `-` | Increase wait time between steps |
| `R` | Randomize ball speeds |
| `S` | Toggle stepwise mode |
| `D` | Toggle real-data display |
| `T` | Cycle thermo overlay (Off → Grid → Temperature → Pressure → Off) |
| `B` | Toggle ball stats overlay |
| `P` | Cycle plot content |
| `Ctrl` + `S` | Save the world (balls, walls, valves, thermostats, sources) to a new file in the save directory |
| `Ctrl` + `O` | Load the newest save from the save directory; the loaded world starts paused in the default view |
| `H` | Print help to log |
| `0` | Reset viewport (zoom + pan) |
| Scroll wheel | Zoom in / out |
| Middle-click drag | Pan viewport |

## Saving and loading a world

`Ctrl+S` writes `world_YYYYMMDD_HHMMSS.json` (UTC, never overwrites an earlier
save) to the save directory, `saves/` unless `--save-dir <dir>` says
otherwise. `Ctrl+O` loads the newest file there. To start from a specific
file, pass `--load <path>`; it works with the window and with `--headless`
(where the loaded world runs immediately and any further headless options such
as `--dt` or `--seed-balls` are applied on top).

```bash
cargo run -p rust_thermo_wgpu -- --load saves/world_20260101_120000.json
cargo run -p rust_thermo_wgpu -- --save-dir ~/lj-worlds
```

The file is versioned JSON (`"version": 1`) holding the world rectangle, time
step, gravity, every ball (`id`, `size_idx`, `pos`, `v`, `pre_update_pos`,
`accel`) and all operators. The view (window size, zoom, pan), the energy plot
and simulation statistics are not saved; a loaded world is shown in the
default view. The window is asked to take the world's shape; if the OS gives
it another size (screen too small, maximised or tiled window), the world is
letterboxed, never stretched or rescaled. A later window resize refits the
world to the window as usual. A file that fails validation is rejected and the running world
is left untouched. Results are logged.

## Releases & Versioning

Versions follow [Semantic Versioning](https://semver.org/). Releases are
created by pushing a `vMAJOR.MINOR.PATCH` tag; the `Release` workflow
(`.github/workflows/release.yml`) builds the binaries and publishes the
release with generated notes. See the
[release list](https://github.com/rabnab/molecularLenardJones/releases)
for changes per version.

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

* [Contributing Guide](CONTRIBUTING.md)
* [Runtime Architecture](docs/architecture.md)
* [Simulation Coordinate Systems](docs/simulation.md)
* [Headless Renderer Plan and Contract](docs/headless-renderer-plan.md)
* [Drift Test Process](docs/drift-test.md)
* [Operator Implementation Guide](docs/operator-implementation-guide.md)
* [Nosé-Hoover Thermostat Plan](docs/nose-hoover.md)
* [Copilot Instructions](.github/copilot-instructions.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow, code quality standards, and PR process.

## Future Plans

* Hierarchical particles for simulating thousands of particles/atoms
* Dockable log window
