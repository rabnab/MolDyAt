# ThermoSolver

A molecular dynamics simulation of a Lennard-Jones gas in a finite box, written in Rust.

![Rust](https://img.shields.io/badge/Rust-nightly-orange)

## Technologies Used

* **Rust** (nightly) — edition 2024
* **Rendering** — dual backend via cargo features:
  - **ggez** — 2D game framework (legacy renderer)
  - **wgpu** — modern GPU-accelerated renderer with custom shaders (active development)
* **Rayon** — parallel physics calculations
* **SIMD** — vectorized force computation (SSE2 / AVX2 / NEON)

## Features

### Physics Engine

* **Lennard-Jones potential** with configurable force capping for energy conservation
* **Verlet neighbor lists** with automatic rebuild based on movement threshold
* **Spatial partitioning** via cell grid for O(N) neighbor lookups
* **5 ball size classes** with correct radius-aware interactions

### Performance Optimizations

* **Parallelization**: Multi-threaded physics via Rayon (always enabled)
* **SIMD Vectorization**: 2-4× speedup on force calculations (always enabled)
  - SSE2 (4-wide) for x86/x86_64
  - AVX2 (4-wide with FMA) for modern x86_64 CPUs
  - NEON (4-wide) for ARM/Apple Silicon
* **Spatial Partitioning**: Cell grid with Verlet neighbor lists

### WGPU Renderer

The wgpu renderer (`--features renderer-wgpu`) provides:

* **RenderGraph architecture** — structured pipeline management with `RenderPipeline` trait
* **6 GPU pipelines** — frame border, grid, lines, balls, crosshair, text overlay
* **Instanced rendering** — thousands of particles via GPU instancing
* **Runtime pipeline control** — enable/disable and reorder pipelines via handles
* **Custom WGSL shaders** — all rendering done through GPU shaders
* **Threaded rendering** — physics runs on a separate thread from the render loop

## Getting Started

### Prerequisites

* [Rust nightly toolchain](https://rustup.rs/) (the `rust-toolchain.toml` file selects it automatically)

### Building

Choose **one** renderer feature — they are mutually exclusive:

```bash
# wgpu renderer (recommended for development)
cargo +nightly build --no-default-features --features renderer-wgpu

# ggez renderer (legacy)
cargo +nightly build --no-default-features --features renderer-ggez
```

### Running

```bash
# wgpu renderer
cargo +nightly run --no-default-features --features renderer-wgpu

# ggez renderer
cargo +nightly run --no-default-features --features renderer-ggez
```

### Testing

```bash
# Run all tests (with wgpu feature)
cargo +nightly test --no-default-features --features renderer-wgpu

# Run all tests (with ggez feature)
cargo +nightly test --no-default-features --features renderer-ggez
```

### Linting & Formatting

```bash
cargo +nightly fmt
cargo +nightly clippy --no-default-features --features renderer-wgpu -- -D warnings
```

> **Note:** Parallelization and SIMD are always enabled and cannot be disabled.

## Project Structure

```
src/
├── main.rs                    # Entry point, feature-gated renderer selection
├── application/               # Simulation logic, physics thread, stats
│   └── core/                  # Ball management, simulation stepping
├── graphics/
│   ├── renderer/              # Shared: viewport, input handling, batch rendering
│   ├── ggez/                  # ggez backend (legacy)
│   └── wgpu/                  # wgpu backend
│       ├── render_graph.rs    # RenderPipeline trait, RenderGraph orchestrator
│       ├── wgpu_pipelines.rs  # Pipeline collection with named handles
│       ├── wgpu_window.rs     # Window/event loop
│       ├── *_pipeline.rs      # Individual GPU pipelines
│       └── shaders/           # WGSL shader sources
├── objects2d/                 # Ball physics, SIMD forces, Verlet lists, cell grid
└── timing_monad.rs            # Performance measurement
```

## Documentation

* [Performance Optimization Guide](PERFORMANCE_OPTIMIZATION.md) — parallelization, SIMD, and benchmarking
* [Engine Improvements Analysis](docs/engine-improvements.md) — architectural analysis and improvement roadmap
* [Contributing Guide](CONTRIBUTING.md) — development workflow, code quality standards, PR process
* [Copilot Instructions](.github/copilot-instructions.md) — guidelines for AI-assisted development

## Contributing

Want to contribute? Check out our [Contributing Guide](CONTRIBUTING.md) for development workflow, code quality standards, and how to submit pull requests.

## Future Plans

* Hierarchical particles for simulating thousands of particles/atoms
* Interactive obstacles (placeable and moveable by mouse)
* Dockable graph window (energy plots, temperature)
* Dockable log window
