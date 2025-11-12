**ThermoSolver**

This is a molecular dynamics simulation of a Lennard-Jones gas in a finite box

## Technologies Used
* Rust (nightly)
* ggez for graphics output
* Rayon for parallelization
* SIMD vectorization (SSE2/AVX2/NEON)

## Features

### Performance Optimizations

* **Parallelization**: Multi-threaded physics calculations using Rayon (always enabled)
* **SIMD Vectorization**: CPU SIMD instructions for 2-4× speedup on force calculations (always enabled)
  - SSE2 (4-wide) for x86/x86_64
  - AVX2 (8-wide) for modern x86_64 CPUs
  - NEON (4-wide) for ARM/Apple Silicon
* **Spatial Partitioning**: Cell grid with Verlet neighbor lists for efficient collision detection

### Building

**Standard build (with all optimizations):**
```bash
cargo +nightly build --release
```

**Note:** Parallelization and SIMD are always enabled and cannot be disabled.

### Testing
```bash
cargo +nightly test
```

### Documentation

* [Performance Optimization Guide](PERFORMANCE_OPTIMIZATION.md) - Comprehensive guide covering parallelization, SIMD,
  and performance characteristics

## Future Plans
* use openGL with shaders for pretty output
* use some sort of hierarchical particles for simulating thousands of particles/atoms
* put obstacles in box
* make obstacles moveable by mouse
* add a (dockable) window with graphs
* add a (dockable) window with log
