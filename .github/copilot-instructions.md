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

Toolchain is pinned in `rust-toolchain.toml` (nightly). Use plain `cargo`. Use `--profile perf` for performance comparisons (`release` with thin LTO, faster to build); use `--release` only to build the optimised binary itself.

- Crates: `thermo_physics` (physics, world, stepping, config, saves), `thermo_runtime` (physics thread, commands, snapshots, headless runs), both without a windowing stack; `thermo_ui` (UI layer: event loop, tools, viewport, sidebar, energy window); `moldyat_wgpu` (binary). Each crate is used under its own name; there are no re-exports between them. Physics tests live in `thermo_physics`, physics-thread, snapshot and headless tests in `thermo_runtime`. `thermo_physics` exposes only what the other crates use: `SimulationWorld` has accessors, no public fields (see `docs/architecture.md#the-thermo_physics-api`).
- Run a single test (name substring): `cargo test -p thermo_physics test_remove_invalid_balls`
- Run an exact test name: `cargo test -p thermo_physics test_remove_invalid_balls -- --exact`
- Run a single integration test file (`tests/<name>.rs`): `cargo test -p thermo_physics --test integration_file_name`
- Package vs binary: the cargo package is `moldyat_wgpu` (use it with `-p`); the executable it builds is `moldyat` (`target/<profile>/moldyat`).
- Minimal headless run: `cargo run -p moldyat_wgpu -- --headless`
- Deterministic seeded run (reproducible): `cargo run -p moldyat_wgpu -- --headless --steps 1000 --rng-seed 12345 --no-stdout`
- CI format check: `cargo fmt -- --check`
- Performance baseline (per-phase ms/step JSON, compare binaries): `scripts/headless_perf.sh [BASE_BIN NEW_BIN]`; see `docs/perf-baseline.md`

---

## Key repository-specific conventions and gotchas

- Index contracts: cells and Verlet lists store indices into the primary `Vec<Ball2D>`. After any index-affecting mutation (`Vec::retain()`, `remove()`, `swap_remove()`), clear all neighbor lists and rebuild cell indexes immediately; never cache indices across structural vector modifications.
- Energy conservation: force capping uses a quadratic potential below `r_cap` so energy stays continuous. If altering force or potential functions, implement matching potential extrapolation, add an energy test, and verify energy conservation.
- Species (`ball_impl::species`): a ball stores a `SpeciesId`; mass, `σ`, `ε`, colour and `display_radius` come from the world's `SpeciesTable`, an `Arc` owned by `BallPhysics` (`SimulationWorld::species()` reads it there) and injected into the `BallFactory` (`BallFactory::with_species`, swapped only by `use_species`, which resets the species of new balls for another table) and every `RendererSnapshot`. `WorldFile::validate` and `SimConfig::validated_species` resolve it once as `ResolvedSpecies`, which `SimulationWorld::with_resolved` takes. Without a config `[[species]]` list it is the built-in library (`SpeciesTable::builtin()`: `N2`, the default and unit species, the former size classes `XS`–`XL`, then `Ar`, `Ne`, `CH4`, `O2`); a list replaces the whole table and its first entry is the default. Never look species up in `SpeciesTable::builtin()` outside tests and the default; use the world's table. `display_radius` is for drawing only and must not reach the physics.
- Pair potentials (`ball_impl::pair_potential`): a `PairLaw` (e.g. `LennardJones { sigma, epsilon }`) is the bare law; `CappedPotential` adds the force cap and the cutoff shift once for every law; a `PairTable` holds one capped potential per species pair (Lorentz–Berthelot mixing: `σ_ij = (σ_i + σ_j)/2`, `ε_ij = √(ε_i ε_j)`, except pairs with a `PairOverride` from the config's `[[potential.pair_override]]`, kept by `BallPhysics::pair_overrides()`), and the `Potential` enum lists the laws in use. `CapSettings` (`max_force`, `core_stiffness`, `cutoff_factor`) is owned by the potential and fixed when it is built (read it via `BallPhysics::cap_settings()`); `PhysicsConstants` holds only runtime settings (gravity). A new interaction law implements `PairLaw` (scalar, and `Lanes` for four pairs with different parameters), gets a `Potential` variant and a match arm in `simd_forces::force_simd`, which dispatches once per ball to the monomorphised `force_kernel`; the kernel gathers each chunk's four pair potentials by the neighbours' species (`CappedLanes::gather`), with no per-neighbour branching.
- Run settings (`application::sim_config`): the size of a new world (`[world] scale`, `WorldConfig::rect()`; a loaded save keeps its `world_rect`, a window resize keeps the world units per pixel), the time step, speed limit factor, force cap and cutoff (`CapSettings`), Verlet skin factor and snapshot interval come from `SimConfig` (defaults, then a `--config` TOML file, then CLI flags such as `--dt` and `--world-scale`); `MainContent::with_config` builds the world from it and `MainContent::config()` reads it back. A new tunable becomes a defaulted `SimConfig` key whose default is today's value, not a new constant. Saves store the config; changing a default changes how saves without a config run (see the `save-format` skill).
- Cutoff: all pair distances are centre distances. A pair interacts only for centre distance `< cutoff_ij` (`cutoff_factor * σ_ij`); its potential is shifted by `U(cutoff_ij)` (`pot_at_cutoff`) so it is zero there. Neighbour lists use the per-pair list radius `cutoff_ij + skin` (`BallPhysics::list_radius`) and may hold pairs beyond the cutoff; they must contribute no force or energy, otherwise every list change moves the total energy.
- Force capping: when centre distance `< r_cap` (per pair), the force is `max_force + core_stiffness * (r_cap - r)` with potential `U(r) = U(r_cap) - max_force * (r - r_cap) + 0.5 * core_stiffness * (r - r_cap)^2` (F = -dU/dr, repulsive). The slope keeps deeper overlaps pushing harder than shallow ones; a flat cap lets crowded neighbours pin overlapping pairs (#240). `r_cap` is cached in each `CappedPotential` and computed by `force_cap_distance()` (Newton, accepted only if the root lies in `PairLaw::cap_search_range()` and hits `max_force`; otherwise a logged bisection over that range, by default `[0.1·σ, equilibrium distance]`); the law's `force_derivative` must be exact or Newton does not converge (#276).
- Wall contacts: box walls and line segments are resolved in the position update along the wall normal (`ball_impl::wall_contact`), not by projecting or mirroring the whole overshoot, which gains energy on every contact. A ball is reflected at the centre distance its `WallLaw` gives (`BallPhysics::wall_contact_distance`; hard contact at `σ_i / 2`); wall code asks the law, it does not use a ball radius. The velocity update applies the returned `WallContact`s and books the momentum change as wall momentum.
- Movement threshold: a ball rebuilds its Verlet list when its displacement since its last rebuild exceeds skin/3 (`get_min_movement_for_update_hood`). Rebuilds are per ball and asynchronous, so skin/2 is not enough; the drift suite's stale-list check (`missed_neighbor_pairs`) guards this.
- Cell grid: cells are at least the largest list radius among the species present in the world (`SimulationWorld::species_present`, `CellGrid::list_radius`). Adding a ball of a new species marks the need; `fit_cell_grid` rebuilds the grid before the next neighbour rebuild, when every ball is in its cell. Never rebuild the grid between a position update and the grid moves.
- Overlap and placement checks use the edge gap of a pair, its centre distance minus its contact distance `σ_ij` (`BallPhysics::edge_gap` / `contact_distance`), which is zero where the pair potential crosses zero, also for an overridden pair. Without an override it equals the gap between contact discs (`σ/2` per ball, `Ball2D::get_edge_distance`). A `σ_ii` override also sets the species' wall contact (`σ_ii / 2`). Balls with NaN positions are invalid and must be removed.
- SIMD kernels (`simd_forces`) use portable `std::simd` (`f32x4`) without runtime dispatch or multiversioning: the instruction set is fixed at compile time (target baseline in dev, `target-cpu=native` in the release and perf profiles). Compare f32 results and drift baselines only between builds for the same target and profile.
- Parallel physics: Rayon `par_iter()` with read-only neighbor access. Avoid introducing shared mutable state inside force loops.
- Viewport/coordinate rules: world-space is the canonical simulation space (see `docs/simulation.md`). Renderer flips Y for pixel-space; use `ViewportTransform` helpers.
- Avoid `unsafe` unless necessary for performance-critical sections; document why and which invariants hold.
- Docs layout: `docs/` describes the current code, `docs/plan/` holds open plans, `docs/done/` implemented plans kept as history. See the `docs-maintenance` skill before adding, moving or renaming a doc.
- Line endings: Git normalizes text files to LF on commit (`.gitattributes`, `* text=auto`); no further check needed.
- Keyboard commands: one `CommandDescriptor` per command in `COMMANDS` (`graphics/renderer/event_loop/command_registry.rs`) holds its default bindings and its UI-vs-domain routing, and the key dispatch and the help text (`H`) are generated from them; add an entry plus its handler (a `UiAction` arm or a `DomainCommand` variant), never a key `match`. Bind every key that types a character (letters, digits, `+`, `-`, the numpad) by that character (`BindKey::Symbol`, `symbol('s', …)`), so shortcuts sit where they are printed on every layout; only keys that type none (SPACE, ESC, TAB, DEL, BACKSPACE, Shift, F-keys) by physical `KeyCode` (#327). The bindings in effect are a `KeyMap`: the defaults plus the `[key_bindings]` of a UI settings file (`ui.toml` / `--ui-config`, #335), a UI setting that never goes into `SimConfig` or saves. Ids (`sim.toggle_pause`) are stable. A test keeps the README key table in sync. See `docs/architecture.md#keyboard-commands`.
- Operator tools (line segment, valve, thermostat, source sphere): one `TOOLS` list (`crates/thermo_ui/src/graphics/sidebar.rs`) sets the icon order and drives the hit-testing and the wgpu icon atlas. See the `operator-framework` skill before adding, renaming or reordering tools.

---

## Testing Strategy

**Test Placement**:
- Unit tests: Place in a `tests/` subdirectory next to the module under test, with one file per topic (e.g. `src/application/core/tests/ball_management.rs`, `src/application/core/tests/energy_conservation.rs`)
- The `tests/` directory must have a `mod.rs` that declares each topic file as `mod topic_name;` and contains shared imports/helpers
- Inline `#[cfg(test)] mod tests` is acceptable only for small, self-contained modules where a single test file suffices (e.g. `mat3.rs`)
- Use descriptive test names, e.g. `test_remove_invalid_balls_clears_neighbor_indices`

---

## Git/PR details

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
