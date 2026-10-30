# Copilot Instructions for MolDyAt

Commands and process rules for AI assistants; other docs link here instead of copying them. Architecture is in `docs/`.

---

## Canonical command and policy references

### Cargo command policy (source of truth)

- Rust nightly is pinned by `rust-toolchain.toml`; use plain `cargo` commands.
- Use `--profile perf` for performance comparisons (`release` with thin LTO, faster to build); use `--release` only to build the optimised binary itself.

### Validation policy (source of truth)

- Required before commit: `cargo fmt` and `cargo clippy --workspace -- -D warnings`.
- Run tests when `.rs`, `.toml`, or `.lock` files change; use the smallest command that covers modified behavior, then widen only when needed.
- New `.rs` and `.wgsl` files start with the MPL-2.0 header (`CONTRIBUTING.md`, checked by `scripts/check_release_files.sh`).

### Branch and PR policy (source of truth)

- Work on feature/bugfix branches only (`feature/master/*`, `bugfix/master/*`).
- Never commit directly to `master`; all changes go through PR review and CI.

### Drift threshold policy (source of truth)

- Drift thresholds may change only with intentional behavior changes and baseline evidence; follow `docs/drift-test.md`.

---

## Quick build, test and lint commands (explicit)

- Crates: `thermo_physics` (physics, world, stepping, config, saves), `thermo_runtime` (physics thread, commands, snapshots, headless runs), both without a windowing stack; `thermo_ui` (UI layer: event loop, tools, viewport, sidebar, energy window); `moldyat_wgpu` (binary). Each crate is used under its own name; there are no re-exports between them. Physics tests live in `thermo_physics`, physics-thread, snapshot and headless tests in `thermo_runtime`. `thermo_physics` exposes only what the other crates use: `SimulationWorld` has accessors, no public fields (see `docs/architecture.md#the-thermo_physics-api`).
- One test: `cargo test -p thermo_physics test_remove_invalid_balls` (name substring; add `-- --exact` for the exact name, `--test <file>` for `tests/<file>.rs`)
- Package vs binary: the cargo package is `moldyat_wgpu` (use it with `-p`); the executable it builds is `moldyat` (`target/<profile>/moldyat`).
- Headless run: `cargo run -p moldyat_wgpu -- --headless`; seeded and reproducible: add `--steps 1000 --rng-seed 12345 --no-stdout`
- CI format check: `cargo fmt -- --check`
- Performance baseline (per-phase ms/step JSON, compare binaries): `scripts/headless_perf.sh [BASE_BIN NEW_BIN]`; see the `perf-compare` skill

---

## Area rules

Read the matching file before changing code in its area (Copilot applies them by path):

| Code | Rules |
|---|---|
| `crates/thermo_physics/`, `crates/thermo_runtime/`: physics, species, potentials, config, neighbour lists, walls | `.github/instructions/physics.instructions.md` |
| `crates/thermo_ui/`, `wgpu_renderer/`: coordinates, keyboard commands, operator tools | `.github/instructions/ui.instructions.md` |

- Avoid `unsafe` unless necessary for performance-critical sections; document why and which invariants hold.
- Docs layout: `docs/` describes the current code, `docs/plan/` holds open plans, `docs/done/` implemented plans kept as history. See the `docs-maintenance` skill before adding, moving or renaming a doc.
- Line endings: Git normalizes text files to LF on commit (`.gitattributes`); no further check needed.

---

## Writing instructions and skills

This file is loaded into every session; skills and area files load only when used. Keep them small:
- Put a rule in the narrowest place: one area of the code → `.github/instructions/*.instructions.md`, a procedure → a skill, this file only for what every session needs.
- State each rule once and point to it elsewhere; never copy it.
- Rules, not history: no backstories, past migrations or descriptions of how the code works (that is `docs/`). Keep an issue number only where it explains a rule.
- Commands longer than a line go into `scripts/`.
- When adding text, cut what it makes redundant. Budget: this file 1,100 words, a skill or area file 1,500 words; trim or split before going over.

---

## Testing Strategy

- Unit tests go in a `tests/` subdirectory next to the module under test, one file per topic (e.g. `src/application/core/tests/energy_conservation.rs`), with a `mod.rs` that declares each topic file and holds shared imports and helpers.
- Inline `#[cfg(test)] mod tests` only for small, self-contained modules where one test file suffices (e.g. `mat3.rs`).
- Descriptive test names, e.g. `test_remove_invalid_balls_clears_neighbor_indices`.

---

## Tools

- Files: read, search and edit with MCP tools. Use the IDE's MCP tools when connected (JetBrains `mcp__rustrover__*`, local sessions), otherwise the built-in file tools (Read, Edit, Write, Grep, Glob). Never read or search files with `cat`, `sed -n`, `head`, `tail`, `grep`, `find` or `ls`, and never write files from the shell or with `python` (`sed -i`, heredocs, scripts). Read only the part of a large file you need.
- Shell: only `cargo`, `scripts/*`, git, and `gh` where the next point allows it.
- GitHub (issues, PRs, CI runs, releases): use the GitHub MCP tools when they are connected (cloud sessions, whose sandbox has no `gh`); use `gh` only when they are not (local sessions). Do not probe for `gh` when GitHub MCP tools are listed. Skills write GitHub steps as `gh` commands: they name the operation; carry it out with the matching MCP tool.
- Output: keep command output small. Use `-q` for `cargo test` and `cargo clippy`, read only the failed steps of CI logs, and cut long output down (e.g. `| tail -n 40`, or only `error`/`FAILED` lines) before reading it.
- If no available tool can do an operation (e.g. linking a project board), say so in the report or PR instead of trying alternatives.

---

## Git/PR details

- Reference issues/PRs in commit messages when relevant; use imperative mood ("Fix index invalidation").
- Follow `.github/commit_template.txt`. AI-assisted commits end with an `assisted by: <Tool> <email>` trailer and never mention the AI session: no `Claude-Session:` trailer, session link or ID in commits or PR bodies.
- Squash and merge when appropriate; verify energy conservation for physics changes.
- With `gh`, use non-interactive commands with explicit flags. Set milestone and project item links explicitly during PR finalization.
- Git without a TTY: `git --no-pager` for output, `GIT_EDITOR=true` for `rebase --continue` / `cherry-pick --continue`.
- Before pushing, check `git --no-pager rev-list --left-right --count HEAD...@{upstream}`; if behind, `git pull --rebase --autostash`. After a history rewrite, `git push --force-with-lease`.
- On rebase conflicts, resolve, `git add <paths>`, then continue; skip only commits that are obsolete or already upstream.
