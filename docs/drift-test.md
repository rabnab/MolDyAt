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

## Recommended Rollout

1. Add harness + scenario #1 and #2 first (fastest high-signal checks).
2. Add dense-gas test with conservative tolerances.
3. Add wall-reflection and timestep convergence checks.
4. Tune thresholds only with documented baseline data, not ad-hoc per failure.
