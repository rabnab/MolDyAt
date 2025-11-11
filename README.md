**ThermoSolver**

This is a molecular dynamics simulation of a Lennard-Jones gas in a finite box

## Technologies Used
* Rust (nightly)
* ggez for graphics output
* Rayon for parallelization
* SIMD vectorization (SSE2/AVX2/NEON)

## Features

### Performance Optimizations
* **Parallelization**: Multi-threaded physics calculations using Rayon
* **SIMD Vectorization**: CPU SIMD instructions for 2-4× speedup on force calculations
  - SSE2 (4-wide) for x86/x86_64
  - AVX2 (8-wide) for modern x86_64 CPUs
  - NEON (4-wide) for ARM/Apple Silicon
* **Spatial Partitioning**: Cell grid with Verlet neighbor lists for efficient collision detection
* **Combined Performance**: Up to 48× speedup on modern CPUs

### Building

**Standard build (with all optimizations):**
```bash
cargo +nightly build --release
```

**Without SIMD:**
```bash
cargo +nightly build --release --no-default-features --features renderer-ggez,parallel
```

**Without parallelization:**
```bash
cargo +nightly build --release --no-default-features --features renderer-ggez,simd
```

**Minimal build (no optimizations):**
```bash
cargo +nightly build --release --no-default-features --features renderer-ggez
```

### Testing
```bash
cargo +nightly test
```

### Documentation
* [Parallel Implementation](PARALLEL_IMPLEMENTATION.md)
* [SIMD Implementation](SIMD_IMPLEMENTATION.md)

## Future Plans
* use openGL with shaders for pretty output
* use some sort of hierarchical particles for simulating thousands of particles/atoms
* put obstacles in box
* make obstacles moveable by mouse
* add a (dockable) window with graphs
* add a (dockable) window with log
