# Copilot Instructions for Molecular Lennard-Jones Simulation

This file guides Copilot-style assistants for making correct, safe, and useful edits in this repository.

NOTE: keep edits here aligned with CONTRIBUTING.md and README.md. Consult `docs/` for architecture details.

---

## Canonical command and policy references

Use this file as the single source of truth for commands and process rules. Other docs should link here instead of duplicating normative command lists.

### Cargo command policy (source of truth)

- Rust nightly is pinned by `rust-toolchain.toml`; use plain `cargo` commands.
- Prefer targeted package/test commands first, then expand scope only when needed.

### Validation policy (source of truth)

- Required before commit: `cargo fmt` and `cargo clippy --workspace -- -D warnings`.
- Run tests when `.rs`, `.toml`, or `.lock` files change; use the smallest command that covers modified behavior.

### Branch and PR policy (source of truth)

- Work on feature/bugfix branches only (`feature/master/*`, `bugfix/master/*`).
- Never commit directly to `master`; all changes go through PR review and CI.

### Drift threshold policy (source of truth)

- Drift thresholds may change only with intentional behavior changes and baseline evidence.
- Follow the process in `docs/drift-test.md` for baseline capture and PR justification.

---

## Quick build, test and lint commands (explicit)

Toolchain is pinned in `rust-toolchain.toml` (nightly). Use plain `cargo`.

General (workspace):
- Build workspace: `cargo build`
- Run all tests: `cargo test --workspace`
- Format: `cargo fmt`
- Lint (deny warnings): `cargo clippy --workspace -- -D warnings`

Per-package / single-target examples:
- Build single package: `cargo build -p rust_thermo_wgpu`
- Test single package: `cargo test -p rust_thermo_core`
- Run a single test (name substring):
  `cargo test -p rust_thermo_core test_remove_invalid_balls`
- Run an exact test name (use `-- --exact` forwarded to the test binary):
  `cargo test -p rust_thermo_core test_remove_invalid_balls -- --exact`
- Run a single integration test file (named `tests/<name>.rs`):
  `cargo test -p rust_thermo_core --test integration_file_name`

Headless/run examples (useful for CI & regression):
- Minimal headless run: `cargo run -p rust_thermo_wgpu -- --headless`
- Deterministic seeded run (reproducible):
  `cargo run -p rust_thermo_wgpu -- --headless --steps 1000 --rng-seed 12345 --no-stdout`

CI helpers:
- To check formatting only: `cargo fmt -- --check`
- To lint workspace only: `cargo clippy --workspace -- -D warnings`

---

## High-level architecture (concise)

- Workspace members: `core/` (rust_thermo_core) contains simulation, physics, operator logic, and tests; `wgpu_renderer/` (rust_thermo_wgpu) contains entrypoint, windowing and GPU pipelines.
- Simulation single-source-of-truth: world-space state lives in `core` (ball positions, velocities, operators). Renderer consumes that state via a `Renderer` trait.
- Physics hot path: `simulation_step()` → parallel force computation (Rayon) → SIMD force kernels (multiversion) → position update. Neighbor discovery: spatial CellGrid → Verlet neighbor lists (index-based Vec<usize>).
- Index contracts: cells and Verlet lists store indices into the primary `Vec<Ball2D>`; any operation that reorders or removes entries (e.g., `Vec::retain()`, `swap_remove`) invalidates indices and MUST trigger neighbor/cell rebuilds.
- Energy preservation: force-capping uses a linear potential beyond `r_cap` (computed via Newton's method) to ensure energy continuity. Any physics change must preserve this invariant and be validated with energy checks.

Relevant locations:
- core/src/objects2d/ball_impl/ (physics, verlet, simd_forces)
- core/src/objects2d/ball_cell/ (cell grid)
- core/src/frame/renderer/ (Renderer trait, viewport transforms)
- wgpu_renderer/src/wgpu/ (render pipelines, shaders)
- docs/ (architecture and simulation coordinate conventions)

---

## Key repository-specific conventions and gotchas

- Rust nightly required (see `rust-toolchain.toml`). Always run `cargo fmt` and `cargo clippy --workspace -- -D warnings` before committing.
- Neighbor lists are index-based and inexpensive to update when valid; do not try to cache indices across structural vector modifications.
- After any index-affecting mutation (remove/retain/swap_remove) clear all neighbor lists and rebuild cell indexes immediately.
- Force capping is implemented to preserve energy. If altering force or potential functions, implement matching potential extrapolation to avoid energy discontinuities.
- Movement threshold: Verlet lists rebuild when tracked displacement exceeds a threshold (≈ 0.75 × effective_cutoff). Look for movement-tracking helpers in the verlet module.
- SIMD kernels are multiversioned for CPU targets (SSE2/AVX2/NEON). Avoid changing calling conventions or ABI assumptions in simd_forces without updating multiversion attributes.
- Parallel physics: parallel iteration uses Rayon `par_iter()` with read-only neighbor access. Avoid introducing shared mutable state inside force loops.
- Viewport/coordinate rules: world-space is the canonical simulation space (see `docs/simulation.md`). Renderer flips Y for pixel-space; use `ViewportTransform` helpers.
- Tests organization: topic-based test files live next to the code under `tests/` directories. Use descriptive test names so targeted runs are easy.

---

## What Copilot-style assistants should do (short checklist)

- Respect the energy-conservation constraint for any physics edits; add unit tests verifying energy behaviour for changes affecting forces/integration.
- When changing data structures that store indices, update all dependent subsystems (cell grid, verlet lists, any cached indices).
- Prefer small, surgical edits. Run the minimal package tests and clippy/format before suggesting a PR.
- For performance/algorithm changes, include a brief benchmark or a `cargo bench` hint and a simple correctness test (energy or conserved quantity).
- If adding or renaming operator tools, update all three locations listed under the Operator Single-Source rules in this repo.

---

## Helpful links in-tree

- README.md — quick start and headless examples
- CONTRIBUTING.md — workflow and mandatory checks
- docs/simulation.md — coordinate systems and viewport rules
- core/src/objects2d/ball_impl/ — physics, verlet, simd
- core/src/objects2d/ball_cell/ — spatial partitioning
- wgpu_renderer/src/wgpu/ — render pipelines and shaders

---

## Git/PR details

- Branch names: `feature/master/*`, `bugfix/master/*` (follow CONTRIBUTING.md)
- Commit trailer: include the Co-authored-by trailer in commits when using Copilot-assisted commits unless the author requests otherwise:

  Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>

- Use `gh` CLI for PRs when scripting or creating reproducible CI-like commands.

---
 
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
1. **Physics Step**: `simulation_step()` → `compute_forces_parallel()` → `force_simd()` (parallel over balls) → `update_positions()` (parallel)
2. **Neighbor Updates**: When ball moves > threshold → clear all neighbor lists → rebuild via `collect_neighbours_by_cell()`
3. **Index Invalidation**: Ball removal via `Vec::retain()` → invalidates all indices → triggers full neighbor list rebuild

## Build & Toolchain

- **Rust Version**: This project uses Rust nightly. Always use the toolchain specified in `rust-toolchain` file
- **Development builds**: only use --release for performance testing, not for development
- **Dependencies**: Managed via Cargo.toml. Use `cargo update` to update dependencies, but review changes for
  compatibility.
- **CI/CD**: Set up continuous integration to run tests, formatting checks, and clippy on every pull request.
- **When to test**: only test when .rs, .toml or .lock files change
- **Build Command**: `cargo build`
- **Test Command**: `cargo test`
- **Format Command**: `cargo fmt`
- **Lint Command**: `cargo clippy --workspace -- -D warnings`

## Code Quality Standards

### Formatting & Linting (MANDATORY)
- **REQUIRED before every commit**:
  1. Run `cargo fmt` to ensure consistent formatting
  2. Run `cargo clippy --workspace -- -D warnings` to catch all warnings/errors
  3. Both must pass without errors before committing
- Address all clippy warnings unless there's a documented reason to allow them
- CI checks enforce: `cargo fmt -- --check` and `cargo clippy --workspace -- -D warnings`
- **Never commit code that fails formatting or clippy checks**
- **Always run formatting and clippy before committing - no exceptions**

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

### Method Length & Maintainability
- **Keep methods short and focused** - aim for single responsibility
- If a method exceeds ~50-100 lines, consider splitting it into smaller functions
- Break complex logic into well-named helper functions for clarity
- Each function should have a clear, testable purpose
- When in doubt, prefer multiple small functions over one large function
- Long methods are harder to test, debug, and maintain

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
- Unit tests: Place in a `tests/` subdirectory next to the module under test, with one file per topic (e.g. `src/application/core/tests/ball_management.rs`, `src/application/core/tests/energy_conservation.rs`)
- The `tests/` directory must have a `mod.rs` that declares each topic file as `mod topic_name;` and contains shared imports/helpers
- Inline `#[cfg(test)] mod tests` is acceptable only for small, self-contained modules where a single test file suffices (e.g. `mat3.rs`)
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

### Operator Framework (AI Implementation Contract)

Use this section when implementing or changing operators (line segment, valve, thermostat, source sphere).

#### Single Source Of Truth Rules
- Keep icon count and geometry constants in `core/src/graphics/sidebar.rs` as the shared source for both hit-testing and rendering.
- Keep icon-to-tool mapping synchronized across:
  - `core/src/graphics/renderer/event_loop/input_handler.rs` (`OperatorTool` enum)
  - `core/src/graphics/renderer/event_loop/sidebar_tool.rs` (`operator_tool_for_icon()`, `selected_sidebar_icon()`)
  - `wgpu_renderer/src/wgpu/sidebar_pipeline.rs` icon atlas order (`ICON_WALL`, `ICON_VALVE`, `ICON_THERMO`, `ICON_SOURCE_OPEN`)
- If you add/reorder/remove tools, update all three locations in the same change.

#### Event Loop And Command Flow
- UI selection/hit-testing stays in renderer state (`UnifiedRenderState`) and emits `InputAction` values.
- Physics-affecting changes must flow through commands (not direct mutation from renderer):
  - `SelectOperatorTool`
  - `CommitLineSegment`
  - `MoveLineSegmentEndpoint`
- Preserve separation of concerns:
  - Renderer/event loop: tool state and interaction
  - Application core: simulation state and collision/physics effects

#### World-Space Behavior Requirements
- Operator geometry is world-space state. Store operator positions in world coordinates.
- On world resize, preserve fractional location using shared helpers from `core/src/objects2d/operator.rs`:
  - `remap_point()` for endpoint/anchor remapping
  - `is_degenerate()` guard to avoid invalid remaps
- Keep clear/reset behavior deterministic:
  - Clearing simulation also clears operator collections
  - ID counters reset to zero on clear

#### Collision And Physics Requirements
- For impenetrable barriers (line segments), maintain this behavior:
  - Position correction to eliminate overlap
  - Velocity reflection using surface normal (`v' = v - 2(v·n)n`)
  - Reflect only when moving toward barrier (velocity gate)
  - Preserve speed magnitude for elastic reflection
- Keep pressure/momentum accounting wired into collision resolution when modifying barrier physics.

#### Sidebar Renderer Coupling Rules
- Sidebar hit-testing and WGPU geometry must agree on which icons are visible for a given window height.
- In sidebar geometry builders, clamp visible icons by both:
  - Available vertical space
  - Actual icon count in atlas/tool set
- When selected icon highlighting changes, ensure selected index semantics remain stable (`Option<usize>`).

#### Test Requirements For Operator Changes
- Place tests in topic files under `tests/` directories (no large inline test modules).
- Minimum coverage for operator changes:
  - Selection/mapping tests (icon index ↔ tool)
  - Placement/edit interaction tests (commit/move endpoint)
  - Collision behavior tests (reflect/moving-away/no-collision)
  - Rescale invariants (relative position preserved, degenerate-rect guard)
  - Sidebar visibility/geometry consistency tests
- Keep tests for each layer near code under test:
  - Shared operator primitives: `core/src/objects2d/operator.rs`
  - Core simulation/operator effects: `core/src/application/core/tests/`
  - Event loop + sidebar mapping: `core/src/graphics/renderer/event_loop/tests/`
  - WGPU sidebar geometry: `wgpu_renderer/src/wgpu/tests/`

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

### GitHub CLI Usage (Issues/PRs)
- Prefer the GitHub CLI (`gh`) for all issue and pull-request operations (create, view, edit, label, assign, milestone, project linking, comments, review requests).
- Avoid using MCP GitHub tools for issue/PR updates when `gh` is available.
- Use non-interactive `gh` commands with explicit flags so actions are reproducible in logs and CI-like environments.
- For project metadata, set milestone and project item links explicitly during PR finalization.

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
- Add *.md documentation files for high-level architecture and design overviews in the ./docs/ directory
- Update README.md for significant architectural changes

## Common Pitfalls

1. **Index Invalidation**: After `Vec::retain()`, `Vec::remove()`, or `Vec::swap_remove()`, all indices may be invalid
2. **Aliasing Violations**: Don't create immutable and mutable references to same data simultaneously
3. **Energy Drift**: Numerical integration errors accumulate; use appropriate timesteps and integration methods
4. **Neighbor List Staleness**: Update when particles move > threshold distance from last update position
5. **Radius Awareness**: Many calculations need effective distances accounting for ball radii, not just centers
6. **Force Cap Energy**: When modifying force capping, maintain linear potential extrapolation to preserve energy conservation
