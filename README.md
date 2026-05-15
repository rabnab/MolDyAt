# ThermoSolver

A molecular dynamics simulation of a Lennard-Jones gas in a finite box, written in Rust.

![Rust](https://img.shields.io/badge/Rust-nightly-orange)

> **Note:** Earlier versions used [ggez](https://github.com/ggez/ggez) as the rendering backend.
> The renderer has since been replaced by a custom wgpu/WGSL pipeline.

## Technologies Used

* **Rust** (nightly) — edition 2024, Cargo workspace
* **wgpu** — GPU-accelerated renderer with custom WGSL shaders
* **winit** — cross-platform window and event loop
* **Rayon** — parallel physics calculations
* **SIMD** — vectorized force computation (SSE2 / AVX2 / NEON)

## Features

### Physics Engine

* **Lennard-Jones potential** with configurable force capping for energy conservation
* **Verlet neighbor lists** with automatic rebuild based on movement threshold
* **Spatial partitioning** via cell grid for O(N) neighbor lookups
* **5 ball size classes** with radius-aware interactions
* **Physics thread** — simulation runs independently from the render loop

### Performance Optimizations

* **Parallelization**: Multi-threaded physics via Rayon (always enabled)
* **SIMD Vectorization**: 2-4× speedup on force calculations (always enabled)
  - SSE2 (4-wide) for x86/x86_64
  - AVX2 (4-wide with FMA) for modern x86_64 CPUs
  - NEON (4-wide) for ARM/Apple Silicon
* **Spatial Partitioning**: Cell grid with Verlet neighbor lists

### Renderer

* **RenderGraph architecture** — `RenderPipeline` trait with named pipeline handles
* **6 GPU pipelines** — frame border, checkerboard grid, lines, balls, crosshair, text overlay
* **Instanced ball rendering** — position/radius/color uploaded via GPU storage buffer
* **SDF circles** — anti-aliased ball fill, equilibrium ring (magenta), cutoff ring (gray)
* **Viewport** — zoom (scroll wheel), pan (middle-click drag), reset (`0`)
* **Energy plot window** — dockable overlay toggled at runtime (`TAB`)

## Getting Started

### Prerequisites

* [Rust nightly toolchain](https://rustup.rs/) — selected automatically via `rust-toolchain.toml`

### Building & Running

```bash
cargo +nightly build -p rust_thermo_wgpu
cargo +nightly run   -p rust_thermo_wgpu
```

### Testing

```bash
cargo +nightly test --workspace
```

### Linting & Formatting

```bash
cargo +nightly fmt
cargo +nightly clippy --workspace -- -D warnings
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
| `H` | Print help to log |
| `0` | Reset viewport (zoom + pan) |
| Scroll wheel | Zoom in / out |
| Middle-click drag | Pan viewport |

## Project Structure

```
Cargo.toml                     # Workspace root (members: core, wgpu_renderer)
core/
└── src/
    ├── application/           # Simulation controller, physics thread, stats
    │   └── core/              # Ball management, simulation stepping
    │       └── tests/         # Topic-based test files (ball_management, energy, …)
    ├── graphics/
    │   └── renderer/          # Renderer trait, viewport, batch renderer
    │       ├── event_loop/    # InputHandler, PhysicsCoordinator, Metrics,
    │       │                  #   UnifiedRenderState, EnergyWindowManager
    │       │   └── tests/     # Topic-based test files (key_actions, mouse, …)
    │       └── viewport/      # ViewportTransform, coordinate mapping
    │           └── tests.rs   # Viewport transform tests
    ├── objects2d/             # Ball physics, SIMD forces, Verlet lists, cell grid
    ├── logging/               # env_logger initialisation
    └── timing_monad.rs        # Performance measurement helpers
wgpu_renderer/
└── src/
    ├── main.rs                # Entry point, Rayon init, window creation
    └── wgpu/
        ├── wgpu_context.rs    # Device / queue / surface setup
        ├── wgpu_window.rs     # winit event loop, frame submission
        ├── render_graph.rs    # RenderGraph orchestrator
        ├── wgpu_pipelines.rs  # Pipeline registry with named handles
        ├── wgpu_input.rs      # WGPU-specific input action execution
        ├── *_pipeline.rs      # ball, grid, line, frame, text, crosshair
        └── shaders/           # WGSL shader sources
```

## Documentation

* [Contributing Guide](CONTRIBUTING.md)
* [Runtime Architecture](docs/architecture.md)
* [Simulation Coordinate Systems](docs/simulation.md)
* [Copilot Instructions](.github/copilot-instructions.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow, code quality standards, and PR process.

## Future Plans

* Hierarchical particles for simulating thousands of particles/atoms
* Interactive obstacles (placeable and moveable by mouse)
* Dockable graph window (energy plots, temperature)
* Dockable log window
