# Contributing to MolecularLenardJones

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

For architectural placement and migration-boundary guidance, also review:

- **[Runtime Architecture](docs/architecture.md)** - Target ownership boundaries, command flow, and current migration shims
- **[Simulation Coordinate Systems](docs/simulation.md)** - World, viewport, and grid-space conventions

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
