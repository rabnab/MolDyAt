# Copilot Instructions for MolDyAt

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

Toolchain is pinned in `rust-toolchain.toml` (nightly). Use plain `cargo`. Use `--release` only for performance testing.

- Run a single test (name substring): `cargo test -p rust_thermo_core test_remove_invalid_balls`
- Run an exact test name: `cargo test -p rust_thermo_core test_remove_invalid_balls -- --exact`
- Run a single integration test file (`tests/<name>.rs`): `cargo test -p rust_thermo_core --test integration_file_name`
- Package vs binary: the cargo package is `rust_thermo_wgpu` (use it with `-p`); the executable it builds is `moldyat` (`target/<profile>/moldyat`).
- Minimal headless run: `cargo run -p rust_thermo_wgpu -- --headless`
- Deterministic seeded run (reproducible): `cargo run -p rust_thermo_wgpu -- --headless --steps 1000 --rng-seed 12345 --no-stdout`
- CI format check: `cargo fmt -- --check`
- Performance baseline (per-phase ms/step JSON, compare binaries): `scripts/headless_perf.sh [BASE_BIN NEW_BIN]`; see `docs/perf-baseline.md`

---

## Key repository-specific conventions and gotchas

- Index contracts: cells and Verlet lists store indices into the primary `Vec<Ball2D>`. After any index-affecting mutation (`Vec::retain()`, `remove()`, `swap_remove()`), clear all neighbor lists and rebuild cell indexes immediately; never cache indices across structural vector modifications.
- Energy conservation: force capping uses a quadratic potential below `r_cap` so energy stays continuous. If altering force or potential functions, implement matching potential extrapolation, add an energy test, and verify energy conservation.
- Cutoff: pairs interact only for edge distance `< lj_cutoff`; the potential is shifted by `U(lj_cutoff)` (`pot_at_cutoff`) so it is zero there. Neighbour lists (radius cutoff + skin) may hold pairs beyond the cutoff; they must contribute no force or energy, otherwise every list change moves the total energy.
- Force capping: when edge distance `< r_cap`, the force is `max_force + core_stiffness * (r_cap - r)` with potential `U(r) = U(r_cap) - max_force * (r - r_cap) + 0.5 * core_stiffness * (r - r_cap)^2` (F = -dU/dr, repulsive). The slope keeps deeper overlaps pushing harder than shallow ones; a flat cap lets crowded neighbours pin overlapping pairs (#240). `r_cap` is cached and computed via Newton's method in `calculate_force_cap_distance()`.
- Wall contacts: box walls and line segments are resolved in the position update along the wall normal (`ball_impl::wall_contact`), not by projecting or mirroring the whole overshoot, which gains energy on every contact. The velocity update applies the returned `WallContact`s and books the momentum change as wall momentum.
- Movement threshold: a ball rebuilds its Verlet list when its displacement since its last rebuild exceeds skin/3 (`get_min_movement_for_update_hood`). Rebuilds are per ball and asynchronous, so skin/2 is not enough; the drift suite's stale-list check (`missed_neighbor_pairs`) guards this.
- Radius awareness: distance and cutoff calculations must account for ball radii, not just centers. Balls with NaN positions are invalid and must be removed.
- SIMD kernels (`simd_forces`) use portable `std::simd` (`f32x4`) without runtime dispatch or multiversioning: the instruction set is fixed at compile time (target baseline in dev, `target-cpu=native` in the release profile). Compare f32 results and drift baselines only between builds for the same target and profile.
- Parallel physics: Rayon `par_iter()` with read-only neighbor access. Avoid introducing shared mutable state inside force loops.
- Viewport/coordinate rules: world-space is the canonical simulation space (see `docs/simulation.md`). Renderer flips Y for pixel-space; use `ViewportTransform` helpers.
- Avoid `unsafe` unless necessary for performance-critical sections; document why and which invariants hold.
- Line endings: Git normalizes text files to LF on commit (`.gitattributes`, `* text=auto`); no further check needed.
- Operator tools (line segment, valve, thermostat, source sphere): the icon-to-tool mapping must stay in sync across 3 files. See the `operator-framework` skill before adding, renaming or reordering tools.

---

## Testing Strategy

**Test Placement**:
- Unit tests: Place in a `tests/` subdirectory next to the module under test, with one file per topic (e.g. `src/application/core/tests/ball_management.rs`, `src/application/core/tests/energy_conservation.rs`)
- The `tests/` directory must have a `mod.rs` that declares each topic file as `mod topic_name;` and contains shared imports/helpers
- Inline `#[cfg(test)] mod tests` is acceptable only for small, self-contained modules where a single test file suffices (e.g. `mat3.rs`)
- Use descriptive test names, e.g. `test_remove_invalid_balls_clears_neighbor_indices`

---

## Git/PR details

- Commit trailer: include the Co-authored-by trailer in commits when using Copilot-assisted commits unless the author requests otherwise:

  Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>

- Reference issues/PRs in commit messages when relevant; use imperative mood ("Fix index invalidation").
- Squash and merge when appropriate; verify energy conservation for physics changes.

### GitHub CLI Usage (Issues/PRs)
- Prefer the GitHub CLI (`gh`) for all issue and pull-request operations (create, view, edit, label, assign, milestone, project linking, comments, review requests).
- Avoid using MCP GitHub tools for issue/PR updates when `gh` is available.
- Use non-interactive `gh` commands with explicit flags so actions are reproducible in logs and CI-like environments.
- For project metadata, set milestone and project item links explicitly during PR finalization.

### Non-interactive Git in CLI sessions
- Prefer non-interactive git commands in automated/CLI agent sessions.
- Disable pagers in scripted calls to avoid stuck sessions:
  - `git --no-pager status -sb`
  - `git --no-pager log --oneline -n 20`
- When continuing rebase/cherry-pick in a non-TTY shell, set an editor override to avoid Vim prompts:
  - `GIT_EDITOR=true git rebase --continue`
  - `GIT_EDITOR=true git cherry-pick --continue`
- Before pushing, verify divergence explicitly:
  - `git --no-pager rev-list --left-right --count HEAD...@{upstream}`
  - If behind, use `git pull --rebase --autostash` before push.
- After history rewrite (rebase/squash), prefer safe force push:
  - `git push --force-with-lease`
- If rebase stops on conflicts, resolve files, `git add <paths>`, then continue;
  skip only commits that are intentionally obsolete or already upstream.
