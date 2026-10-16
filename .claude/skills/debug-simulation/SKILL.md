---
name: debug-simulation
description: Procedure for reproducing, narrowing down and pinning a simulation bug - energy jumps, balls escaping walls, NaN or vanished balls, stuck or overlapping balls, stale neighbour lists, wrong temperature or pressure, non-reproducible runs, rendering of wrong state. Use when investigating a bug report or an unexpected test or drift failure, before changing code.
---

# Debugging the Simulation

Goal: a deterministic reproduction first, then the smallest scenario that shows the bug, then a regression test that fails before the fix. Fixing without a reproduction is guessing.

## 1. Reproduce deterministically
- **Headless, seeded** (preferred; no window, byte-identical reruns):
  ```bash
  cargo run -p moldyat_wgpu -- --headless --rng-seed 12345 --steps 2000 \
    --seed-balls 500 --seed-speed 4 --snapshot-every 10 --detail balls \
    --output "$TMPDIR/run.jsonl" --no-stdout
  ```
  Add walls and valves with `--line-segment` / `--valve`, scale the world with `--world-scale`, change `--dt`. Watch `delta_energy` per frame; `--detail balls` gives every ball's position and velocity. Flags: `docs/headless.md`.
- **From the window:** pause (Space), save (Ctrl+S, writes to `saves/`), then replay the saved state headlessly with `--load saves/world_….json --headless --steps …`. Saves contain positions, velocities and accelerations, so the replay starts from the exact state.
- **Freeze a setup:** `--save-setup` writes the prepared headless world to `--save-dir`, so others can rerun the same start.
- If a run is not reproducible with the same seed and flags, that is the bug: check thread-count dependence with `RAYON_NUM_THREADS=1` vs the default (see the `physics-change` skill, Reproducibility).

## 2. Observe
- **Logs** (`env_logger`, default `info`):
  - `RUST_LOG=debug` for everything,
  - `RUST_LOG=info,moldyat::verbose=debug` for per-step output of console-verbose balls (key `V`, ball id, mode `c`),
  - `moldyat::timing` (info) for step timings while F4 is on,
  - `RUST_LOG=info,moldyat::resize=debug` for resize frame timings (#217).
- **In the window:** `V` makes a ball verbose (`c` console, `m` main neighbours, `o` other neighbours, `s` speed); `B` ball stats; `F3` cutoff rings; `T` thermo field; `F4` timings; `S` step-by-step mode; `D` real units.
- **Counters** in `SimStats`: `wall_contacts`, `wall_overlap_corrections` (should stay near 0), `wall_distance_checks`; invalid (NaN) balls are logged with `log::warn!` when removed.

## 3. Narrow down
Map the symptom to the step phase (`docs/architecture.md`, Simulation step) and the usual suspects:

| Symptom | Look at |
|---|---|
| Energy jumps when pairs approach or separate | force/potential mismatch, cutoff shift, force cap (`pair_potential.rs`), scalar vs SIMD path (`simd_forces.rs`) |
| Energy jumps at neighbour-list changes | pairs beyond the cutoff contributing, stale lists (skin/3 threshold, `verlet.rs`), cell size (`cell_grid.rs`) |
| Energy gain at walls, balls resting on walls drift | wall contact (`wall_contact.rs`), overlap fallback (`line_segments.rs`) |
| Balls escape through walls or valves | `collect_solid_obstacles`, valve state, ghost wall, thickness |
| Wrong ball touched after removal, panics on index | index contract after `remove_invalid_balls` |
| Drift grows in large worlds | f32 precision far from the origin (#213) |
| Temperature or pressure looks wrong | COM-subtracted temperature, virial pressure (`thermo_field.rs`), sampling stride |
| UI shows old or wrong state | snapshot layers, `world_generation`, snapshot cadence |

Shrink the scenario: fewer balls, fewer steps, one wall, two balls if possible. The drift harness (`core/src/application/core/tests/drift_harness.rs`) gives per-step checks for free: missed or asymmetric neighbour pairs, deepest overlap, cap hits, energy and momentum drift.

## 4. Pin it with a test
- Write the smallest failing test in the matching topic file under `core/src/application/core/tests/` (or a drift scenario if it needs many steps). Build worlds centred on the origin; set positions and velocities explicitly.
- Confirm it fails before the fix and passes after.
- Physics fixes then follow the `physics-change` skill (drift baselines, fixtures).

## Tools
- `cargo test -p moldyat_core <name> -- --nocapture` to see log output of one test.
- `cargo ci-test-drift` for the drift suite summary lines.
- Delete `$TMPDIR` run files when done; do not commit saves or JSONL dumps.
