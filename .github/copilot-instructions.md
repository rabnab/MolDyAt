# Copilot Instructions for Molecular Lennard-Jones Simulation

## Project Overview

This is a **molecular dynamics physics simulation** implementing Lennard-Jones particle interactions with Verlet neighbor lists and spatial partitioning. The simulation is written in Rust and uses the ggez game framework for rendering. It should be possible to switch to a different implementation of renderer e.g. using wgpu directly.

**Critical Physics Constraint**: Energy conservation is a primary concern. Any changes to physics calculations, collision handling, or integration methods must preserve total system energy. Verify energy conservation when modifying physics code.

## Build & Toolchain

- **Rust Version**: This project uses Rust nightly. Always use the toolchain specified in `rust-toolchain` file
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
- **Use Feature Flags for Optional Functionality**: Use Cargo features for optional components (e.g., SIMD, alternate
  renderers).
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
- **Verlet Neighbor Lists**: Use indices (`Vec<usize>`) to track nearby particles, reducing O(N²) checks
- **Cutoff Distances**: Interactions beyond cutoff are ignored; account for particle radii in cutoff calculations
- **Movement Thresholds**: Neighbor lists rebuild when particles move beyond threshold (typically 0.75 × effective_cutoff)
- **Energy Conservation**: Total energy (kinetic + potential) should remain constant in isolated systems

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
