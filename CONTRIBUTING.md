# Contributing to MolDyAt

Thank you for your interest in contributing to this molecular dynamics simulation project!

## Development Setup

This project uses Rust nightly. Make sure you have the correct toolchain installed:

```bash
rustup install nightly
```

## GitHub Copilot

This project has comprehensive instructions for GitHub Copilot to help with development. If you're using GitHub Copilot, please review:

- **[Copilot Instructions](.github/copilot-instructions.md)** - Complete guidelines for working with Copilot on this project
- **[Quick Reference](.github/instructions/Common_rules)** - IDE-specific quick reference for JetBrains/RustRover

These instructions cover:
- Project architecture and physics constraints
- Build and test commands
- Code quality standards
- Testing strategy
- Git workflow requirements

For how the application is structured, also review:

- **[Runtime Architecture](docs/architecture.md)** - Crates, threads, layers, command and snapshot flow
- **[Simulation Coordinate Systems](docs/simulation.md)** - World, viewport, and grid-space conventions
- **[Reduced Units and Scales](docs/units.md)** - LJ reduced units, SI conversion, and parameters of other species
- **[Operators](docs/operators.md)** - Walls, valves, thermostats, source spheres and their tools
- **[Headless Mode](docs/headless.md)** - CLI, JSONL records and regression fixtures
- **[Drift Tests](docs/drift-test.md)** - Energy and momentum drift suite and threshold changes
- **[Performance Baseline](docs/perf-baseline.md)** - Benchmark scenarios and comparing binaries

Docs in `docs/` describe the current code; open plans are in `docs/plan/`, implemented ones in `docs/done/`.

### Canonical command and policy references

To avoid documentation drift, treat these sections in `.github/copilot-instructions.md` as authoritative:

- [Cargo command policy](.github/copilot-instructions.md#cargo-command-policy-source-of-truth)
- [Validation policy](.github/copilot-instructions.md#validation-policy-source-of-truth)
- [Branch and PR policy](.github/copilot-instructions.md#branch-and-pr-policy-source-of-truth)
- [Quick build, test and lint commands](.github/copilot-instructions.md#quick-build-test-and-lint-commands-explicit)

## Development Workflow

### Before Making Changes

1. **Create a feature branch** - Never commit directly to master
   ```bash
   git checkout -b feature/master/descriptive-name
   ```

2. **Read the Copilot Instructions** - Familiarize yourself with the project structure and constraints

### Making Changes

1. **Follow code quality standards**:
   - Keep methods short and focused
   - Avoid `unsafe` code unless necessary
   - Document complex algorithms
   - Maintain energy conservation in physics code

2. **Run tests frequently**:
   ```bash
   cargo test --workspace
   ```

3. **Format your code**:
   ```bash
   cargo fmt
   ```

4. **Run linter**:
   ```bash
   cargo clippy --workspace -- -D warnings
   ```

### Before Committing

**MANDATORY steps before every commit:**

1. Run formatting: `cargo fmt`
2. Run clippy: `cargo clippy --workspace -- -D warnings`
3. Run tests: `cargo test --workspace` (only if .rs, .toml, or .lock files changed)

Both formatting and linting must pass without errors.

### Refactor PRs: required gates

A refactor must not change simulation results. Besides the steps above, a PR
that restructures code without an intended behavior change must pass these
gates unchanged; CI runs all of them in the `test` job:

1. **Seeded headless fixtures**: `cargo ci-test-headless-regression`
   - `wgpu_renderer/src/headless_runner/fixtures/seeded_headless_regression.jsonl`: free gas, summary records.
   - `wgpu_renderer/src/headless_runner/fixtures/seeded_headless_operators.jsonl`: walls, a valve, a thermostat and a source sphere, with every ball's position and velocity.
2. **Drift suite**: `cargo ci-test-drift` (see [docs/drift-test.md](docs/drift-test.md)).
3. **Thread-count reproducibility**: `cargo test --workspace thread_count`.
   The same seed must give bit-identical energies, wall momentum and ball
   state with 1 and with 4 Rayon threads. Collect parallel results in index order and reduce
   them serially; a parallel `sum`/`reduce` over floats breaks this.

A refactor PR must not touch the fixture files or the drift thresholds. If a
fixture has to change, the PR changes behavior: say so in the description,
refresh the fixture with `UPDATE_HEADLESS_FIXTURE=1 cargo ci-test-headless-regression`
and include the fixture diff as evidence. Fixtures hold f32 results, so
refresh them with the dev profile on the same target as CI (x86_64 Linux).
`sin`, `cos` and `exp` come from the platform math library and can give
different bits on Windows (MSVC) than on Linux (glibc), so the fixture tests
may fail there without any behavior change. On Windows, run the fixture
checks via CI or WSL; never refresh fixtures there.

### Save format: reference saves

`core/src/application/persistence/tests/fixtures/` holds one reference save per
world-file format version (`v1.json`, ...), each written by the build that
introduced that version. `cargo test -p moldyat_core reference_saves` loads
every one of them through the real load path and checks its contents.

- **Never edit or regenerate an existing reference save.** It stands for the
  files users already have.
- A format change that bumps `WORLD_FILE_VERSION` adds a new fixture (and its
  entry in `FIXTURES` in `tests/reference_saves.rs`); all older fixtures must
  keep loading. The test fails if the current version has no fixture.
- Details: the `save-format` skill and the module doc of
  `core/src/application/persistence`.

### Submitting Changes

1. **Push your branch**:
   ```bash
   git push origin feature/master/descriptive-name
   ```

2. **Create a Pull Request** on GitHub:
   - Write a clear title and description
   - Link related issues (e.g., "Closes #23")
   - Wait for CI checks to pass
   - Address review comments

3. **Never commit directly to master** - All changes go through pull requests

## Licensing of Contributions

The project is licensed under the [MPL-2.0](LICENSE). By submitting a
contribution you agree that it is licensed under the same terms. New `.rs` and
`.wgsl` files must start with the MPL-2.0 notice used by the existing files:

```rust
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
```

## Commit Message Guidelines

- Use imperative mood: "Fix bug" not "Fixed bug" or "Fixes bug"
- Be specific: "Account for ball radii in collision detection"
- Reference issues when relevant: "Addresses #18"

## Code Review

- All pull requests require review
- Address all review comments before merging
- CI must pass (tests, formatting, clippy)
- Verify energy conservation for physics changes

## Questions?

If you have questions about the project structure, physics implementation, or development workflow, please refer to the [Copilot Instructions](.github/copilot-instructions.md) for detailed information.
