# Drift Tests for Headless Molecular Dynamics

This document proposes a deterministic, headless drift-test harness and a compact set of high-value scenarios to detect physical correctness regressions and characterize simulation artifacts.

## Goal

Measure how much the simulation departs from expected conserved quantities over many steps, and identify where artifacts arise (integrator error, force capping region, neighbor-list staleness, collision handling).

## Core Principles

- **Deterministic runs**: fixed seed, fixed dt, fixed initial state.
- **Long horizons**: enough steps to expose slow drift.
- **Invariant tracking**: energy and momentum as primary signals.
- **Threshold-based assertions**: pass/fail with explicit tolerances.
- **Artifact diagnostics**: emit metrics that help explain failures, not just detect them.

## Harness Design (Headless)

Implement a reusable headless test helper (for example in `../core/src/application/core/tests`):

1. **Scenario builder**
   - Construct balls with exact positions, velocities, radii, and masses.
   - Configure world bounds and operator/barrier state (if used).
   - Configure physics parameters: `dt`, cutoff, `max_force`, neighbor-list update thresholds.
   - Optional deterministic RNG seed for stochastic initial layouts.

2. **Simulation runner**
   - Run `N` steps in headless mode.
   - At fixed sampling intervals (or every step), collect:
     - Total kinetic energy
     - Total potential energy (including force-cap linear continuation branch)
     - Total energy = kinetic + potential
     - Total momentum vector
     - Max overlap depth
     - Force-cap hit count / ratio
     - Neighbor-list rebuild count

3. **Metrics and assertions**
   - `relative_energy_drift = |E_t - E_0| / max(|E_0|, eps)`
   - `momentum_drift = |P_t - P_0|`
   - Track max/mean drift over horizon.
   - Assert scenario-specific tolerances.
   - On failure, print compact diagnostic summary (step of first violation, metrics snapshot).

4. **Parameter sweep support (optional but useful)**
   - Re-run same scenario at `dt`, `dt/2` to confirm expected convergence behavior.
   - Re-run with and without force cap (where stable) to isolate cap-related artifacts.

## Suggested Test Cases (3–5)

Use these 5 cases first; they cover the dominant artifact sources with minimal overlap:

### 1) Two-body symmetric LJ orbit/scatter (no boundaries)

**Purpose**: baseline integrator + LJ force correctness in simplest closed system.

- Two equal particles, mirrored initial positions/velocities.
- No wall interaction during test horizon.
- Assertions:
  - Low relative energy drift over long run.
  - Momentum remains near zero (symmetric initial condition).

### 2) Force-cap transition continuity probe

**Purpose**: validate energy continuity and absence of spikes when pair distance crosses `r_cap`.

- Initialize a pair to repeatedly pass through cap boundary (approach/retreat).
- Enable `max_force` so cap branch is active.
- Assertions:
  - No discontinuous jump in total energy when crossing `r_cap`.
  - Cap-hit ratio > 0 (test actually exercises cap region).
  - Drift remains bounded by cap-specific tolerance.

### 3) Dense gas in periodic-like interior regime (no wall hits early)

**Purpose**: stress neighbor lists and many-body interactions for cumulative drift.

- Moderate/high particle count, randomized deterministic initial layout and velocities.
- Place particles away from walls or keep horizon short enough to avoid boundary effects.
- Assertions:
  - Bounded energy drift envelope over many steps.
  - Neighbor-list rebuild cadence in expected range (not pathological under/over rebuild).
  - No persistent large overlaps.

### 4) Elastic wall reflection invariants

**Purpose**: isolate collision response artifacts from LJ pair-force behavior.

- One or few particles aimed at barrier/wall with known normal.
- Disable or minimize inter-particle interactions if needed.
- Assertions:
  - Speed magnitude conserved across elastic reflection.
  - Normal velocity component sign flips correctly when approaching wall.
  - No energy injection at collision events.

### 5) Timestep sensitivity/convergence check

**Purpose**: distinguish numerical integration error from logic bugs.

- Reuse scenario #1 or #3, run with `dt` and `dt/2` for equal physical time.
- Assertions:
  - Drift at `dt/2` is lower than or equal to drift at `dt` by expected margin.
  - If not, flag likely non-integrator source (e.g., neighbor/collision/cap discontinuity).

## Practical Tolerance Strategy

- Start with empirical tolerances from current stable baseline (recorded once and reviewed).
- Keep separate thresholds per scenario (do not force one global number).
- Use two-tier checks:
  - **Hard fail**: clear regression.
  - **Warning band** (optional log only): early indication of creeping drift.

## Reporting

For each scenario, print one concise line:

- scenario name
- steps, dt
- max relative energy drift
- final momentum drift
- cap-hit ratio
- rebuild count
- pass/fail

This makes CI failures diagnosable without rendering.

## CI Integration

Drift scenarios are part of CI through a step of the `test` job (after the unit tests, reusing their build) that executes:

```bash
cargo ci-test-drift   # = cargo test -p moldyat_core -- --ignored --nocapture
```

All scenarios are `#[ignore = "suite:drift"]`, so a plain `cargo test` (without `--ignored`) skips them and prints no `DRIFT_SCENARIO` lines. The alias runs them with `--nocapture` so each scenario emits one machine-readable `DRIFT_SCENARIO ...` summary line (convergence tests a `DRIFT_CONVERGENCE` line) directly in CI logs.

The alias runs every ignored test of `moldyat_core`, so it also runs the two `#[ignore = "suite:physics (slow)"]` tests (`test_overlap_fallback_stays_rare_in_a_gas_with_walls`, `test_virial_pressure_2d_matches_wall_pressure`). They are not drift scenarios and print no summary line.

Canonical command policy reference:
- `.github/copilot-instructions.md#cargo-command-policy-source-of-truth`
- `.github/copilot-instructions.md#quick-build-test-and-lint-commands-explicit`

## Threshold Calibration and Change-Control

Threshold updates are allowed only when behavior changed intentionally (for example integrator/physics updates) and the PR provides baseline evidence. Never loosen thresholds only to clear unrelated failures.

### Baseline capture procedure

1. Run drift scenarios locally with deterministic inputs:
   ```bash
   cargo ci-test-drift   # = cargo test -p moldyat_core -- --ignored --nocapture
   ```
2. Collect the `DRIFT_SCENARIO` lines from:
   - current `master` (before change),
   - your branch (after change).
3. Keep the same seed/steps/scenario setup for both runs.

### Threshold derivation rule

- Keep thresholds per scenario and per metric.
- Start from observed baseline on the changed branch.
- Use an explicit safety margin (for example 1.5-2.0x baseline drift) and round to a stable scientific-notation value.
- Prefer the smallest threshold that keeps deterministic reruns stable.

### Required PR justification for threshold changes

Any PR that modifies drift thresholds must include:

- reason for the physical/numerical behavior shift,
- before/after baseline summary lines,
- chosen threshold and margin rationale,
- linked issue describing the intended behavior change.

Use this compact table in the PR description:

| Scenario | Metric | Old threshold | New threshold | Baseline before | Baseline after | Margin | Reason |
|---|---|---:|---:|---:|---:|---:|---|
| two_body_lj_baseline | relative_energy_drift | 3.0e-2 | 4.5e-2 | 1.8e-2 | 2.2e-2 | 2.0x | Integrator update |

## Implemented Scenarios and Thresholds

All scenarios live in `core/src/application/core/tests/drift_harness/` and run in the drift suite (`cargo ci-test-drift`). Each prints exactly one `DRIFT_SCENARIO` line (convergence tests a `DRIFT_CONVERGENCE` line), also when it fails.

Cutoff change: the LJ interaction is now truncated at `lj_cutoff` and shifted to zero there (previously every listed pair interacted up to the list radius, so each neighbour-list change added or removed energy; see `BallPhysics::potential`). dense_gas baselines were re-measured after that change: max relative energy drift 2.05e-3 -> 1.91e-3, relative momentum drift 5.2e-9 -> 5.1e-8. The momentum value is f32 rounding noise, not an asymmetry (no pair is inside the cutoff for one ball and outside for the other): repeating the scenario with thermal speeds 1.00..1.35 gives 1.7e-8..7.4e-8 with the cutoff and 9.0e-8..3.0e-7 without it, so the old 5.2e-9 baseline was a lucky realisation and its 5.0e-8 threshold sat inside the noise band. The threshold is now 1.5e-7.

Sorted neighbour lists: Verlet lists are now kept sorted by ball index and rebuilt from the 3x3 cells only, and the cell grid rounds its cell count down so every cell is at least cutoff + skin + 2 x max radius wide (it rounded up before, so cells could be slightly smaller). The force kernel sums neighbours in list order, so the f32 sums changed order. dense_gas re-measured: max relative energy drift 1.914e-3 -> 1.914e-3, final 1.259e-3 -> 1.185e-3, relative momentum drift 5.06e-8 -> 7.79e-8 (inside the 1.5e-7 threshold and below the 70% warning line), rebuild rate 2.41e-2 -> 2.41e-2, no missed or asymmetric pairs. The wall-reflection scenarios are unchanged. Thresholds are unchanged.

Drift world moved to the origin (from x, y ~ 500..600, see the timestep note below). Re-measured, thresholds unchanged: dense_gas max relative energy drift 1.914e-3 -> 2.049e-3, final 1.185e-3 -> 1.404e-3, relative momentum drift 7.79e-8 -> 5.59e-8, rebuild rate 2.41e-2 -> 2.43e-2, wall clearance 20.24 -> 20.07; elastic_line_segment energy drift 1.5e-6 -> 5.5e-7, elastic_world_wall 0 -> 0. Positions that tests used to hardcode in the old world (two-body and force-cap pair centres, the wall-test start and the line-segment barrier) are now offsets from `world_center`.

Sloped core (#240): below `r_cap` the pair force now rises as `max_force + core_stiffness·(r_cap - r)` (k = 200) instead of staying flat at `max_force`, with the matching quadratic potential. With a flat cap, crowded neighbours pinned deeply overlapping pairs (no force gradient pushes them apart). Results above `r_cap` are bit-identical; only pairs inside the core change. Re-measured, thresholds unchanged: dense_gas max relative energy drift 2.049e-3 -> 2.000e-3, final 1.404e-3 -> 1.347e-3, relative momentum drift 5.59e-8 -> 8.06e-8, cap hits 39 -> 40; hot_sparse_gas max relative energy drift 4.75e-3 -> 6.53e-3 (82% of its threshold, so CI prints a warning) and max overlap depth 0.57 -> 0.097; balls_removed_and_added max overlap depth 0.58 -> 0.043. The hot-gas value is realisation noise, not a systematic increase: over five initial speeds (`HOT_V` 5.0, 5.5, 6.0, 6.5, 7.0) the max drift averages 3.85e-3 with the flat cap (range 1.7e-3..5.3e-3), 2.99e-3 with k = 100 and 3.65e-3 with k = 200 (range 1.9e-3..6.5e-3), and HOT_V = 6.0 is the worst of the five at k = 200. mixed_sizes_cell_borders (no energy threshold) moved 8.2e-3 -> 1.5e-2.

Re-measurement on `master` at `6349f72` (after #240, #242 and #244): every value in the table below is unchanged except dense_gas_dt_quartering, whose ratio is now 0.095 (coarse 2.000e-3, fine 1.893e-4). The table showed 0.064, measured before #240; the change comes from #240 (0.064 at `b599515^`, 0.095 at `b599515`). Only pairs inside `r_cap` changed there, which the dense gas's projectile pairs reach (40 cap hits); the fine run's max drift rose 1.32e-4 -> 1.89e-4 while the coarse run's fell 2.05e-3 -> 2.00e-3. It passes (threshold 0.125), and CI prints a `DRIFT_WARNING` at 76%. The threshold is unchanged. The two-body, force-cap and two-body dt-halving tests now print summary lines too; their values were measured in the same run and added to the table.

Baseline provenance: measured with `cargo ci-test-drift` (dev profile, which is optimized) on an x86_64 host, on the PR #205 branch on top of `master` at `4f8b801`, after the Verlet movement threshold changed to skin/3. `simd_forces` uses `std::simd` (`f32x4`), so the SIMD path is fixed at compile time by the target features: the dev profile sets no `target-cpu`, so x86_64 drift builds use SSE2 even on AVX2/AVX-512 hosts (aarch64 uses NEON). Only the release profile uses `target-cpu=native` (with AMX disabled, see the workspace `Cargo.toml`). f32 results can differ between targets or `RUSTFLAGS`; rerun on the same target when comparing baselines.

| Scenario | File | Metric | Threshold | Baseline | Margin |
|---|---|---|---:|---:|---|
| dense_gas_many_body (256 balls, 1000 steps, 9 head-on projectile pairs) | `dense_gas.rs` | max relative energy drift | 4.0e-3 | 2.00e-3 | ~2x |
| | | relative momentum drift | 1.5e-7 | 8.1e-8 | ~1.9x; f32 rounding noise, see below |
| | | per-ball rebuild rate | [5.0e-3, 5.0e-2] | 2.4e-2 | cadence band, not a drift threshold |
| | | in-cutoff pairs missing from a neighbour list (every step) | 0 | 0 | exact |
| | | pairs listed by only one ball (every step) | 0 | 0 | exact |
| | | same pair deeper than 0.5 x radius, consecutive steps | <= 5 | 0 | |
| | | force-cap region reached (validity) | >= 1 hit | 40 | |
| | | balls lost / wall clearance (validity) | none / >= 1.0 | none / 20.07 | |
| dense_gas_dt_quartering (dense gas at dt and dt/4, same physical time) | `dense_gas.rs` | max energy drift at dt/4 over that at dt | <= 0.125 | 0.095 | ~1.3x (dt^2 gives 0.0625); 76% of the threshold, CI warns, see the re-measurement note |
| two_body_lj_baseline (2 balls near equilibrium distance, 2000 steps) | `tests.rs` | relative energy drift (final) | 3.0e-2 | 4.46e-3 (max 4.59e-3) | ~6.7x |
| | | relative momentum drift | 3.0e-3 | 0 | |
| force_cap_transition (2 balls driven into the force cap, cap lowered to 10, 300 steps) | `tests.rs` | relative energy drift (final) | 7.0e-1 | 5.27e-1 | ~1.3x; the cap region is not energy-conserving at this dt, the test checks continuity, not conservation |
| | | relative momentum drift | 3.0e-3 | 0 | |
| | | force-cap region reached (max cap-hit ratio) | >= 1.0e-2 | 1.0 | |
| two_body_dt_halving (two-body scenario at dt and dt/2, same physical time) | `tests.rs` | relative energy drift at dt/2 vs at dt | <= 2 x coarse | 2.98e-3 vs 5.99e-3 (ratio 0.50) | weak check: it only rejects drift that grows when dt shrinks; the two-body drift converges ~linearly in dt, not as dt^2 |
| hot_sparse_gas (400 balls over the whole world, speeds up to ~8, 1000 steps) | `neighbor_lists.rs` | max relative energy drift | 8.0e-3 | 6.53e-3 | ~1.2x; realisation noise, see the sloped-core note |
| | | per-ball rebuild rate | [1.0e-1, 4.0e-1] | 2.2e-1 | cadence band, ~10x the dense gas |
| | | in-cutoff pairs missing / listed by one ball (every step) | 0 / 0 | 0 / 0 | exact |
| | | balls lost | none | none | |
| mixed_sizes_cell_borders (21 balls of all sizes, world of 2.2 x 3.3 interaction ranges, 300 steps) | `neighbor_lists.rs` | in-cutoff pairs missing / listed by one ball (every step) | 0 / 0 | 0 / 0 | exact |
| trajectory vs thread count (hot gas and dense gas, 1 vs 4 Rayon threads) | `neighbor_lists.rs` | trajectory hash | equal | equal | exact |
| balls_removed_and_added (hot gas, 3 x 300 steps: remove 40, then add 38) | `neighbor_lists.rs` | in-cutoff pairs missing / listed by one ball (every step after each rebuild) | 0 / 0 | 0 / 0 | exact |
| | | ball counts after removal and after adding | exact | 360 / 398 | |
| elastic_world_wall / elastic_line_segment (1 ball, 10000 steps) | `wall_reflection.rs` | relative speed change per reflection | 1.0e-5 | <= 1.1e-6 | |
| | | impulse alignment with surface normal | >= 0.999 | 1.0 | |
| | | kinetic energy deviation (every step and per event) | 1.0e-5 | 2.2e-6 | ~4.5x |
| | | kinetic energy slope per event | 1.0e-6 | ~0 | |
| | | reflection count vs ray-traced reference | +-1 | 15/15 and 25/25 | |
| | | ball stays inside the box and on its side of the barrier (every step) | penetration <= one step of travel | holds | |

Notes:

- The dense-gas grid starts in the world centre and the run asserts the balls never reach the wall region, so wall effects stay isolated in the reflection scenarios. The projectile pairs drive a few pairs into the force-cap region so the cap and overlap checks are not vacuous; this is also why the energy envelope is wider than a cool gas (7.2e-4) would need.
- Neighbour-list and overlap checks run after every step, not only on sample steps, so a stale list that lasts only a few steps before the next rebuild is still caught.
- Verlet movement threshold: balls rebuild their neighbour lists independently, so a ball may already be 2 thresholds from its last rebuild position when its partner rebuilds, and the partner may move one more. The threshold is therefore skin/3 (not the textbook skin/2 for synchronous rebuilds). With skin/2, the cool dense gas (no projectiles) had an in-cutoff pair missing from both lists at step 186. `test_cool_dense_gas_has_no_stale_neighbor_lists` checks both: skin/3 has no stale pairs, and skin/2 still reproduces the miss (so the test is not vacuous).
- Timestep convergence and the drift world's position: the drift world (`create_dense_harness_with_seed`) is 100 x 100 centred on the origin. It used to sit at x, y ~ 500..600, where an f32 resolves ~3e-5, while a step moves a ball ~1e-2 and the 1/2 a dt^2 term is ~5e-5. That rounding does not shrink with dt, and smaller steps make more of it, so the many-body energy envelope shrank only about linearly with dt there (dense gas max drift 1.91e-3, 1.56e-3, 0.67e-3 at dt, dt/2, dt/4). At the origin it shrinks about as dt^2 (2.05e-3, 0.82e-3, 0.13e-3). The force jump at the cutoff (|F(lj_cutoff)| ~ 1.5e-3; the potential is shifted to zero there, the force is not) is not a significant source: a force-shifted potential changed the results at 500 only within noise, and at the origin it lowers the cool gas at dt/4 from 0.64e-4 to 0.27e-4. The app's default world is also centred on the origin; large worlds (e.g. `--world-scale 8`, coordinates up to ~450) lose position precision the same way.
- Neighbour-list scenarios (`neighbor_lists.rs`): the hot sparse gas makes ~22% of balls rebuild per step, so many partner updates of one parallel rebuild pass meet; `test_hot_sparse_gas_catches_injected_stale_neighbor_lists` runs it with a 10x movement threshold and expects stale lists. The cell-border scenario places two pairs of the largest balls so that a grid with cells narrower than cutoff + skin + 2 x max radius misses them; with the old rounded-up cell count it fails at step 0 (2 pairs missing). The removal/add scenario panics on a stale index if `remove_invalid_balls` stops clearing the lists. Its segments drop the step-0 sample after each structural change, because the cleared or new lists are rebuilt at the start of the next step, before any force is computed. The thread-count test runs the scenarios on dedicated Rayon pools; it proves that the parallel passes do not depend on how work is split, on the same target.
- `rel_momentum_drift` in the wall scenarios is expected to be ~2: walls absorb momentum by design.
- `test_dense_gas_is_deterministic` compares a hash of the whole trajectory. It proves in-process reproducibility only, not reproducibility across runners or SIMD targets; the thresholds are the CI gate.
- Injected regressions are re-runnable: `SimulationWorld::test_faults` (test builds only) scales the Verlet movement threshold or the wall restitution. `test_dense_gas_catches_injected_stale_neighbor_lists` and `test_world_wall_catches_injected_restitution_gain` run the real simulation with a fault and expect the checks to fail. The non-ignored `*_checks_catch_injected_regressions` tests feed synthetic broken runs to the check functions.

### CI reporting

The drift steps of the `test` job always upload `drift-harness.log` and write to the job summary:

- the runner arch, CPU model and best host SIMD flag, plus the compiled `target_feature`s, which decide the SIMD path the baselines depend on;
- every `DRIFT_SCENARIO` and `DRIFT_CONVERGENCE` line (matched anywhere in a log line: tests run in parallel, so a line can start after another test's output);
- every `DRIFT_WARNING` line, also emitted as a `::warning` annotation. A scenario prints one when a passing metric exceeds 70% (`DRIFT_WARNING_FRACTION`) of its threshold, so shrinking margins show up before they fail the build.

## Recommended Rollout

1. Add harness + scenario #1 and #2 first (fastest high-signal checks).
2. Add dense-gas test with conservative tolerances.
3. Add wall-reflection and timestep convergence checks.
4. Tune thresholds only with documented baseline data, not ad-hoc per failure.
