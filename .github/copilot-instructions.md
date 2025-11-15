# Copilot Instructions for Molecular Lennard-Jones Simulation

## Project Overview

This is a **molecular dynamics physics simulation** implementing Lennard-Jones particle interactions with Verlet neighbor lists, spatial partitioning, and force capping. The simulation is written in Rust and uses the ggez game framework for rendering.

**Critical Physics Constraint**: Energy conservation is the primary concern. Any changes to physics calculations (especially force capping, collision handling, or integration methods) must preserve total system energy. Verify energy conservation when modifying physics code - the force cap feature uses a linear potential extrapolation beyond `r_cap` to maintain energy continuity.

## Architecture Overview

### Core Components
- **MainContent** (`src/frame/application_master/`): Central simulation controller using `Rc<RefCell<dyn Renderer>>` pattern for renderer abstraction
- **Ball2D** (`src/objects2d/ball_impl/`): Direct `Vec<Ball2D>` ownership (not `Rc<RefCell<>>`), optimized for performance
- **BallPhysics** (`src/objects2d/ball_impl/ball_physics.rs`): Physics engine with configurable `max_force` cap and cached `r_cap` distance
- **CellGrid** (`src/objects2d/ball_cell/`): Spatial partitioning with index-based (`Vec<usize>`) cell references to main ball vector
- **Verlet Lists** (`src/objects2d/ball_impl/verlet.rs`): Index-based neighbor tracking, rebuilt when movement exceeds threshold
- **SIMD Forces** (`src/objects2d/ball_impl/simd_forces.rs`): Multi-platform vectorization using `multiversion` crate

### Renderer Architecture
- **Trait-based** (`src/frame/renderer/`): `Renderer` trait allows swapping backends (currently ggez, wgpu planned)
- **Backend pattern**: `GgezRenderer` + `GgezBackend` separate rendering logic from framework-specific code
- **Caching**: Mesh caching system via `take_cache()`/`put_cache()` to minimize allocations

### Key Data Flows
1. **Physics Step**: `simulation_step()` → `compute_forces_parallel()` → `force_indexed_simd()` (parallel over balls) → `update_positions()` (parallel)
2. **Neighbor Updates**: When ball moves > threshold → clear all neighbor lists → rebuild via `collect_neighbours_by_cell()`
3. **Index Invalidation**: Ball removal via `Vec::retain()` → invalidates all indices → triggers full neighbor list rebuild

## Build & Toolchain

- **Rust Version**: This project uses Rust nightly. Always use the toolchain specified in `rust-toolchain` file
- **Development builds**: only use --release for performance testing, not for development
- **Dependencies**: Managed via Cargo.toml. Use `cargo update` to update dependencies, but review changes for
  compatibility.
- **CI/CD**: Set up continuous integration to run tests, formatting checks, and clippy on every pull request.
- **When to test**: only test when .rs, .toml or .lock files change
- **Build Command**: `cargo +nightly build`
- **Test Command**: `cargo +nightly test`
- **Format Command**: `cargo +nightly fmt`
- **Lint Command**: `cargo +nightly clippy -- -D warnings`

## Code Quality Standards

### Formatting & Linting
- **Before every commit**: Run `cargo fmt` to ensure consistent formatting
- Run `clippy` to catch common mistakes and non-idiomatic code
- Address all clippy warnings unless there's a documented reason to allow them
- Optional: Add CI checks for `cargo +nightly fmt -- --check` and `cargo +nightly test`

### Line Endings
- **Always use LF (Unix-style) line endings** for all source files
- **RustRover Configuration**:
  1. Go to `File` → `Settings` (or `Ctrl+Alt+S` on Windows)
  2. Navigate to `Editor` → `Code Style`
  3. Under `Line separator`, select `Unix and macOS (\n)`
  4. Click `Apply` and `OK`
  5. Alternatively, set per-project: Right-click project root → `Line Separators` → `LF - Unix and macOS (\n)`

### Safety & Correctness
- **Avoid `unsafe` code** unless absolutely necessary for performance-critical sections
- When `unsafe` is required:
  - Document why it's necessary
  - Explain the invariants being maintained
  - Ensure no undefined behavior (aliasing violations, data races, etc.)
- Prefer borrow checker solutions over unsafe workarounds
- Index-based references are often safer than raw pointers for spatial structures

### Coding Best Practices

- **Builder Pattern for Dependency Injection**: For structs with dependencies unchanged after initialization, use the
  Builder pattern to make dependencies explicit and improve testability.
- **Prefer Immutability**: Use immutable data wherever possible, especially in physics and spatial structures, to reduce
  bugs and improve code clarity.
- **Explicit Error Handling**: Use `Result` and `Option` for error-prone operations. Avoid panics in library code;
  propagate errors with context.
- **Minimize Allocation in Hot Paths**: Pre-allocate vectors and buffers for per-frame operations. Avoid frequent heap
  allocations in update loops.
- **Leverage Rust’s Type System for Safety**: Use newtype wrappers for units (e.g., `struct Energy(f64)`) and enums for
  state machines and ball types.
- **Use Consts and Statics for Physical Constants**: Define simulation constants as `const` or `static` to avoid magic
  numbers.
- **Document Invariants and Algorithms**: Add doc comments explaining invariants and algorithms, especially for physics
  and index validity.
- **Limit Scope of Unsafe Code**: Encapsulate unsafe code in private functions/modules and expose only safe APIs.
- **Favor Small, Focused Functions**: Break complex logic into small, testable functions.
- **Use Iterators and Functional Patterns**: Prefer iterator combinators (`map`, `filter`, `fold`) over manual loops.
- **Profile and Benchmark Regularly**: Use `cargo bench` and flamegraph to identify and optimize bottlenecks.
- **Avoid Global State**: Pass dependencies explicitly; avoid global mutable state.
- **Thread Safety and Concurrency**: Use `Send`/`Sync` bounds and document thread safety if parallelism is introduced.
- **Consistent Naming Conventions**: Use descriptive, consistent names following Rust guidelines.
- **Unit and Integration Test Coverage**: Cover all critical paths with tests; use property-based testing for physics
  invariants if possible.
- **Avoid Premature Optimization**: Write clear, correct code first; optimize after profiling.
- **Use Feature Flags for Optional Functionality**: Use Cargo features for truly optional components (e.g., alternate
  renderers). SIMD and parallelization are now core features and cannot be disabled.
- **Code Review Discipline**: Require code review for all changes and address all comments before merging.
- **Continuous Integration Enforcement**: Automate formatting, linting, and testing in CI to enforce standards.

## Testing Strategy

**Test Placement**:
- Unit tests: Place in `src/module_name/tests.rs` or inline with `#[cfg(test)]` for small modules
- Integration tests: Place in `src/frame/application_master/tests.rs` or similar module-level test files
- Test files should be next to the code they test for discoverability

**What to Test**:
- Physics correctness: Force calculations, collision detection, energy conservation
- Spatial data structures: Cell grid operations, neighbor list updates, index validity
- Edge cases: Ball removal/addition, boundary conditions, size variations
- Performance-critical paths: Ensure optimizations don't break correctness

**Test Naming**:
- Use descriptive names: `test_remove_invalid_balls_clears_neighbor_indices`
- Group related tests in the same test module
- Use `#[should_panic]` for expected failure cases

## Domain-Specific Knowledge

### Physics Implementation
- **Lennard-Jones Potential**: Inter-particle forces with attractive/repulsive components
- **Force Capping**: When edge distance `< r_cap`, force is capped at `max_force` with linear potential `U(r) = U(r_cap) - max_force * (r - r_cap)` to preserve energy (note the minus sign ensures F = -dU/dr = +max_force repulsive)
- **r_cap Calculation**: Cached distance where LJ force equals `max_force`, computed via Newton's method in `calculate_force_cap_distance()`
- **Verlet Neighbor Lists**: Use indices (`Vec<usize>`) to track nearby particles, reducing O(N²) checks
- **Cutoff Distances**: Interactions beyond cutoff are ignored; account for particle radii in cutoff calculations
- **Movement Thresholds**: Neighbor lists rebuild when particles move beyond threshold (typically 0.75 × effective_cutoff)
- **Energy Conservation**: Total energy (kinetic + potential) should remain constant in isolated systems
- **Parallel Force Computation**: Uses Rayon's `par_iter()` with read-only neighbor access for thread safety

### Spatial Data Structures
- **Cell Grid**: Divides space into cells for efficient nearest-neighbor queries
- **Index-Based References**: Cells store `Vec<usize>` indices into main ball vector (`v_points`)
- **Index Invalidation**: Operations like `Vec::retain()` invalidate indices; rebuild dependent structures
- **Neighbor List Consistency**: When balls are removed, clear all neighbor lists and force rebuild

### Ball Management
- **Size Classes**: 5 different ball sizes with radii: `MIN_BALL_SIZE × 2^idx` (max ≈ 2.4)
- **Direct Ownership**: Balls stored in `Vec<Ball2D>` (not `Rc<RefCell<>>`) for performance
- **Validity**: Balls with NaN positions are invalid and should be removed
- **State Consistency**: When modifying ball positions or removing balls, update spatial structures (cell grid, neighbor lists)

## Git Workflow

### Pull Request Workflow (REQUIRED)
- **Never commit directly to master** - All changes must go through pull requests
- Create a feature branch for every change, no matter how small
- Push your branch and create a pull request for review
- Wait for CI checks to pass before merging
- Use "Squash and merge" for cleaner history when appropriate

### Branch Strategy
- Feature branches: `feature/master/descriptive-name`
- Bugfix branches: `bugfix/master/descriptive-name`
- Keep commits atomic and focused
- Ensure all tests pass before pushing
- Run `cargo fmt` before committing

### Commit Messages
- Use imperative mood: "Fix index invalidation" not "Fixed" or "Fixes"
- Be specific: "Account for ball radii in min_movement_for_update_hood calculation"
- Reference issues/PRs when relevant: "Addresses review comment in PR #18"

### Pull Requests
- Write clear PR titles and descriptions
- Link related issues (e.g., "Closes #23" or "Addresses #18")
- Address all review comments before merging
- Ensure CI passes (tests, formatting, clippy)
- Update documentation if public APIs change
- Verify energy conservation for physics changes
- Request reviews from relevant team members

## Performance Considerations

- **Hot Paths**: Physics calculations, neighbor list updates, collision detection
- **Profiling**: Use `cargo bench` or profiling tools for performance-critical changes
- **Allocations**: Minimize allocations in per-frame update loops
- **Borrowing Patterns**: Prefer split borrows and indexing over Rc/RefCell for better performance
- **SIMD Opportunities**: Consider vectorization for bulk operations (if using nightly features)

## Documentation

- Add doc comments (`///`) for public APIs and complex algorithms
- Explain non-obvious design decisions in code comments
- Document invariants that must be maintained (especially for unsafe code)
- Update README.md for significant architectural changes

## Common Pitfalls

1. **Index Invalidation**: After `Vec::retain()`, `Vec::remove()`, or `Vec::swap_remove()`, all indices may be invalid
2. **Aliasing Violations**: Don't create immutable and mutable references to same data simultaneously
3. **Energy Drift**: Numerical integration errors accumulate; use appropriate timesteps and integration methods
4. **Neighbor List Staleness**: Update when particles move > threshold distance from last update position
5. **Radius Awareness**: Many calculations need effective distances accounting for ball radii, not just centers
6. **Force Cap Energy**: When modifying force capping, maintain linear potential extrapolation to preserve energy conservation
