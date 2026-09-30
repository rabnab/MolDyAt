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

Drift scenarios are part of CI through a dedicated workflow step that executes:

```bash
cargo ci-test-drift   # = cargo test -p rust_thermo_core -- --ignored --nocapture
```

All scenarios are `#[ignore = "suite:drift"]`, so a plain `cargo test` (without `--ignored`) skips them and prints no `DRIFT_SCENARIO` lines. The alias runs them with `--nocapture` so each scenario emits one machine-readable `DRIFT_SCENARIO ...` summary line directly in CI logs.

Canonical command policy reference:
- `.github/copilot-instructions.md#cargo-command-policy-source-of-truth`
- `.github/copilot-instructions.md#quick-build-test-and-lint-commands-explicit`

## Threshold Calibration and Change-Control

Threshold updates are allowed only when behavior changed intentionally (for example integrator/physics updates) and the PR provides baseline evidence. Never loosen thresholds only to clear unrelated failures.

### Baseline capture procedure

1. Run drift scenarios locally with deterministic inputs:
   ```bash
   cargo ci-test-drift   # = cargo test -p rust_thermo_core -- --ignored --nocapture
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

All scenarios live in `core/src/application/core/tests/drift_harness/` and run in the drift suite (`cargo ci-test-drift`). Each prints exactly one `DRIFT_SCENARIO` line, also when it fails.

Baseline provenance: measured with `cargo ci-test-drift` (dev profile, which is optimized) on an x86_64 host, on the PR #205 branch on top of `master` at `4f8b801`, after the Verlet movement threshold changed to skin/3. `simd_forces` uses `std::simd` (`f32x4`), so the SIMD path is fixed at compile time by the target features: the dev profile sets no `target-cpu`, so x86_64 drift builds use SSE2 even on AVX2/AVX-512 hosts (aarch64 uses NEON). Only the release profile uses `target-cpu=native` (with AMX disabled, see the workspace `Cargo.toml`). f32 results can differ between targets or `RUSTFLAGS`; rerun on the same target when comparing baselines.

| Scenario | File | Metric | Threshold | Baseline | Margin |
|---|---|---|---:|---:|---|
| dense_gas_many_body (256 balls, 1000 steps, 9 head-on projectile pairs) | `dense_gas.rs` | max relative energy drift | 4.0e-3 | 2.05e-3 | ~2x |
| | | relative momentum drift | 5.0e-8 | 5.2e-9 | ~10x for cross-target f32 summation order |
| | | per-ball rebuild rate | [5.0e-3, 5.0e-2] | 2.4e-2 | cadence band, not a drift threshold |
| | | in-cutoff pairs missing from a neighbour list (every step) | 0 | 0 | exact |
| | | pairs listed by only one ball (every step) | 0 | 0 | exact |
| | | same pair deeper than 0.5 x radius, consecutive steps | <= 5 | 0 | |
| | | force-cap region reached (validity) | >= 1 hit | 39 | |
| | | balls lost / wall clearance (validity) | none / >= 1.0 | none / 20.1 | |
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
- `rel_momentum_drift` in the wall scenarios is expected to be ~2: walls absorb momentum by design.
- `test_dense_gas_is_deterministic` compares a hash of the whole trajectory. It proves in-process reproducibility only, not reproducibility across runners or SIMD targets; the thresholds are the CI gate.
- Injected regressions are re-runnable: `SimulationWorld::test_faults` (test builds only) scales the Verlet movement threshold or the wall restitution. `test_dense_gas_catches_injected_stale_neighbor_lists` and `test_world_wall_catches_injected_restitution_gain` run the real simulation with a fault and expect the checks to fail. The non-ignored `*_checks_catch_injected_regressions` tests feed synthetic broken runs to the check functions.

### CI reporting

The `drift` job always uploads `drift-harness.log` and writes to the job summary:

- the runner arch, CPU model and best host SIMD flag, plus the compiled `target_feature`s, which decide the SIMD path the baselines depend on;
- every `DRIFT_SCENARIO` line;
- every `DRIFT_WARNING` line, also emitted as a `::warning` annotation. A scenario prints one when a passing metric exceeds 70% (`DRIFT_WARNING_FRACTION`) of its threshold, so shrinking margins show up before they fail the build.

## Recommended Rollout

1. Add harness + scenario #1 and #2 first (fastest high-signal checks).
2. Add dense-gas test with conservative tolerances.
3. Add wall-reflection and timestep convergence checks.
4. Tune thresholds only with documented baseline data, not ad-hoc per failure.
