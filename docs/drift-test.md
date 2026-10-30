# Drift Tests

The drift suite runs deterministic, headless simulations for many steps and
checks how far conserved quantities (energy, momentum) depart from their
initial values. Each scenario isolates one source of artifacts: the
integrator, the force-cap region, stale neighbour lists, or wall contacts.
Every metric has its own threshold, and every run prints a machine-readable
summary line, so a CI failure can be diagnosed without rendering.

## Running

```bash
cargo ci-test-drift   # = cargo test -p thermo_physics -- --ignored --nocapture
```

All scenarios are `#[ignore = "suite:drift"]`, so a plain `cargo test` skips
them. CI runs the alias in a step of the `test` job, after the unit tests and
reusing their build. The alias runs every ignored test of `thermo_physics`, so
it also runs the two `#[ignore = "suite:physics (slow)"]` tests
(`test_overlap_fallback_stays_rare_in_a_gas_with_walls`,
`test_virial_pressure_2d_matches_wall_pressure`); they are not drift
scenarios and print no summary line.

Commands follow the
[cargo command policy](../.github/copilot-instructions.md#cargo-command-policy-source-of-truth).

## Harness

The harness is `crates/thermo_physics/src/application/core/tests/drift_harness.rs`; the
scenarios are in `crates/thermo_physics/src/application/core/tests/drift_harness/`
(`dense_gas.rs`, `binary_mixture.rs`, `neighbor_lists.rs`, `wall_reflection.rs`,
`tests.rs`).

- **World.** `create_dense_harness_with_seed` builds a 100 x 100 world
  centred on the origin with a fixed RNG seed; `populate_grid_balls` places a
  grid of balls with seeded thermal velocities. Scenarios that need exact
  states set ball positions, velocities and operators directly.
- **Run.** `run_drift_harness` steps the simulation with a
  `DriftHarnessConfig { steps, sample_every, dt }`.
  `run_drift_harness_with_observer` adds a per-step callback, used for
  structural changes such as removing and adding balls mid-run.
- **Samples.** Every `sample_every` steps a `DriftSample` records kinetic,
  potential and total energy, momentum, maximum overlap depth, the force-cap
  hit ratio and the ball count.
- **Per-step pair checks.** After every step, not only on sample steps,
  `PairStepStats` records in-cutoff pairs missing from a neighbour list,
  pairs listed by only one ball, the deepest overlap, cap hits and the
  minimum edge distance (centre distance minus the pair's `σ_ij`,
  `BallPhysics::edge_gap`). A stale list that lasts only a few steps is caught.
- **Run metrics.** `DriftRunMetrics` holds the initial and final samples,
  the sample and pair histories, the number of steps with a neighbour-list
  rebuild, the per-ball rebuild count, and a bit-exact `trajectory_hash` of
  every sample, every step's pair stats and the final positions and
  velocities.
- **Fault injection.** `SimulationWorld::test_faults` (test builds only)
  scales the Verlet movement threshold or the wall restitution, so tests can
  show that the checks catch real regressions (see Notes).

## Output

Each scenario prints exactly one line, also when it fails:

- `DRIFT_SCENARIO name=… steps=… dt=… balls=… final_balls=…
  max_rel_energy_drift=… final_rel_energy_drift=… rel_momentum_drift=…
  max_cap_hit_ratio=… cap_hits=… min_edge_distance=… max_overlap_depth=…
  rebuild_steps=… ball_rebuild_rate=… max_missed_neighbor_pairs=…
  max_asymmetric_neighbor_pairs=… trajectory_hash=… result=…`
- `DRIFT_CONVERGENCE name=…` for the timestep convergence tests, with the
  coarse and fine drift and their ratio.
- `DRIFT_PRECISION name=… center=…` for the precision measurement
  (`precision.rs`, see [Precision far from the origin](#precision-far-from-the-origin)):
  one line per gas and world centre, with the drift at dt, dt/2 and dt/4.

`relative energy drift` is `|E_t - E_0| / max(|E_0|, eps)`.

`warn_near_threshold` prints a `DRIFT_WARNING name=… metric=… value=…
threshold=… ratio=…` line when a passing metric exceeds
`DRIFT_WARNING_FRACTION` (70 %) of its threshold, so shrinking margins show
up before they fail the build.

## Scenarios and thresholds

| Scenario | File | Metric | Threshold | Baseline | Margin |
|---|---|---|---:|---:|---|
| dense_gas_many_body (256 balls, 1000 steps, 9 head-on projectile pairs at ±3.5) | `dense_gas.rs` | max relative energy drift | 4.0e-3 | 3.50e-3 | ~1.14x; 88 % of the threshold, CI warns, see the #234 entry in [Baseline history](#baseline-history) |
| | | relative momentum drift | 1.5e-7 | 8.0e-9 | ~19x; f32 rounding noise, see below |
| | | per-ball rebuild rate | [5.0e-3, 5.0e-2] | 3.0e-2 | cadence band, not a drift threshold |
| | | in-cutoff pairs missing from a neighbour list (every step) | 0 | 0 | exact |
| | | pairs listed by only one ball (every step) | 0 | 0 | exact |
| | | same pair deeper than 0.5 x contact radius, consecutive steps | <= 5 | 0 | |
| | | force-cap region reached (validity) | >= 1 hit | 93 | |
| | | balls lost / wall clearance (validity) | none / >= 1.0 | none / 10.55 | |
| dense_gas_dt_quartering (dense gas at dt and dt/4, same physical time) | `dense_gas.rs` | max energy drift at dt/4 over that at dt | <= 0.125 | 0.122 | ~1.03x (dt^2 gives 0.0625); 97 % of the threshold, CI warns, see the #234 entry in [Baseline history](#baseline-history) |
| precision by distance (dense and cool gas centred at 0, 125, 500, 2000, each at dt, dt/2, dt/4) | `precision.rs` | dense gas: dt/4 drift over that at the origin | <= 1.6 | 1.07 (at 2000) | ~1.5x; f32 positions gave 2.57 |
| | | cool gas: dt/4 drift over that at the origin | <= 4.0 | 2.01 (at 2000) | 2x; f32 positions gave 22.6 (at 500) |
| binary_mixture (256 balls, checkerboard of `N2` and `XS` with pair overrides to σ = 1.2, ε = 1.5 and Lorentz–Berthelot cross pairs, 1000 steps) | `binary_mixture.rs` | max relative energy drift | 1.0e-3 | 5.67e-4 | ~1.8x; nearby starts give 5.7e-4..2.2e-3, see the #235 entry in [Baseline history](#baseline-history) |
| | | relative momentum drift | 1.0e-7 | 1.8e-8 | ~1.9x the largest of the nearby starts (1.6e-8..5.4e-8); f32 rounding noise |
| | | in-cutoff pairs missing / listed by one ball (every step) | 0 / 0 | 0 / 0 | exact |
| | | balls lost / wall clearance (validity) | none / >= 1.0 | none / 22.27 | |
| | | trajectory hash, 1 vs 4 Rayon threads | equal | equal | exact |
| two_body_lj_baseline (2 balls at 1.05 x their equilibrium distance, 2000 steps) | `tests.rs` | relative energy drift (final) | 3.0e-2 | 2.14e-3 (max 2.18e-3) | ~14x |
| | | relative momentum drift | 3.0e-3 | 0 | |
| force_cap_transition (2 balls driven into the force cap, cap lowered to 10, 300 steps) | `tests.rs` | relative energy drift (final) | 3.0e-1 | 8.31e-2 | ~3.6x; the cap region is not energy-conserving at this dt, the test checks continuity, not conservation (see #276 in [Baseline history](#baseline-history)) |
| | | relative momentum drift | 3.0e-3 | 0 | |
| | | force-cap region reached (max cap-hit ratio) | >= 1.0e-2 | 1.0 | |
| two_body_dt_halving (two-body scenario at dt and dt/2, same physical time) | `tests.rs` | relative energy drift at dt/2 vs at dt | <= 2 x coarse | 1.41e-3 vs 2.83e-3 (ratio 0.50) | weak check: it only rejects drift that grows when dt shrinks; the two-body drift converges ~linearly in dt, not as dt^2 |
| hot_sparse_gas (400 balls over the whole world, speeds up to ~8, 1000 steps) | `neighbor_lists.rs` | max relative energy drift | 8.0e-3 | 1.37e-3 | ~5.8x; kept at 8.0e-3 because the value is realisation noise, see [Baseline history](#baseline-history) |
| | | per-ball rebuild rate | [1.0e-1, 4.0e-1] | 2.2e-1 | cadence band, ~10x the dense gas |
| | | in-cutoff pairs missing / listed by one ball (every step) | 0 / 0 | 0 / 0 | exact |
| | | balls lost | none | none | |
| mixed_sizes_cell_borders (54 balls of all species, world of 2.2 x 3.3 `XL`–`XL` list radii, 300 steps) | `neighbor_lists.rs` | in-cutoff pairs missing / listed by one ball (every step) | 0 / 0 | 0 / 0 | exact |
| trajectory vs thread count (hot gas and dense gas, 1 vs 4 Rayon threads) | `neighbor_lists.rs` | trajectory hash | equal | equal | exact |
| balls_removed_and_added (hot gas, 3 x 300 steps: remove 40, then add 38) | `neighbor_lists.rs` | in-cutoff pairs missing / listed by one ball (every step after each rebuild) | 0 / 0 | 0 / 0 | exact |
| | | ball counts after removal and after adding | exact | 360 / 381 | |
| elastic_world_wall / elastic_line_segment (1 ball, 10000 steps) | `wall_reflection.rs` | relative speed change per reflection | 1.0e-5 | <= 1.1e-6 | |
| | | impulse alignment with surface normal | >= 0.999 | 1.0 | |
| | | kinetic energy deviation (every step and per event) | 1.0e-5 | 2.2e-6 | ~4.5x |
| | | kinetic energy slope per event | 1.0e-6 | ~0 | |
| | | reflection count vs ray-traced reference | +-1 | 15/15 and 26/26 | |
| | | ball stays inside the box and on its side of the barrier (every step) | penetration <= one step of travel | holds | |

**Baseline provenance.** Measured with `cargo ci-test-drift` (dev profile, which is optimized) on an x86_64 host, on the PR #205 branch on top of `master` at `4f8b801`, after the Verlet movement threshold changed to skin/3. `simd_forces` uses `std::simd` (`f32x4`), so the SIMD path is fixed at compile time by the target features: the dev profile sets no `target-cpu`, so x86_64 drift builds use SSE2 even on AVX2/AVX-512 hosts (aarch64 uses NEON). Only the release and perf profiles use `target-cpu=native` (with AMX disabled, see the workspace `Cargo.toml`). f32 results can differ between targets or `RUSTFLAGS`; rerun on the same target when comparing baselines.

Notes:

- The dense-gas grid starts in the world centre and the run asserts the balls never reach the wall region, so wall effects stay isolated in the reflection scenarios. The projectile pairs drive a few pairs into the force-cap region so the cap and overlap checks are not vacuous; this is also why the energy envelope is wider than a cool gas (7.2e-4) would need.
- Neighbour-list and overlap checks run after every step, not only on sample steps, so a stale list that lasts only a few steps before the next rebuild is still caught.
- Verlet movement threshold: balls rebuild their neighbour lists independently, so a ball may already be 2 thresholds from its last rebuild position when its partner rebuilds, and the partner may move one more. The threshold is therefore skin/3 (not the textbook skin/2 for synchronous rebuilds). With skin/2, the cool dense gas (no projectiles) had an in-cutoff pair missing from both lists at step 186. `test_cool_dense_gas_has_no_stale_neighbor_lists` checks both: skin/3 has no stale pairs, and skin/2 still reproduces the miss (so the test is not vacuous).
- Timestep convergence and the drift world's position: the drift world (`create_dense_harness_with_seed`) is 100 x 100 centred on the origin. It used to sit at x, y ~ 500..600, where an f32 resolves ~3e-5, while a step moves a ball ~1e-2 and the 1/2 a dt^2 term is ~5e-5. That rounding does not shrink with dt, and smaller steps make more of it, so the many-body energy envelope shrank only about linearly with dt there (dense gas max drift 1.91e-3, 1.56e-3, 0.67e-3 at dt, dt/2, dt/4). At the origin it shrinks about as dt^2 (2.05e-3, 0.82e-3, 0.13e-3). The force jump at the cutoff (|F(lj_cutoff)| ~ 1.5e-3; the potential is shifted to zero there, the force is not) is not a significant source: a force-shifted potential changed the results at 500 only within noise, and at the origin it lowers the cool gas at dt/4 from 0.64e-4 to 0.27e-4. The app's default world is also centred on the origin; large worlds (e.g. `--world-scale 8`, coordinates up to ~450) lose position precision the same way.
- Neighbour-list scenarios (`neighbor_lists.rs`): the hot sparse gas makes ~22% of balls rebuild per step, so many partner updates of one parallel rebuild pass meet; `test_hot_sparse_gas_catches_injected_stale_neighbor_lists` runs it with a 10x movement threshold and expects stale lists. The cell-border scenario places two pairs of the largest balls so that a grid with cells narrower than cutoff + skin + 2 x max radius misses them; with the old rounded-up cell count it fails at step 0 (2 pairs missing). The removal/add scenario panics on a stale index if `remove_invalid_balls` stops clearing the lists. Its segments drop the step-0 sample after each structural change, because the cleared or new lists are rebuilt at the start of the next step, before any force is computed. The thread-count test runs the scenarios on dedicated Rayon pools; it proves that the parallel passes do not depend on how work is split, on the same target.
- `rel_momentum_drift` in the wall scenarios is expected to be ~2: walls absorb momentum by design.
- `test_dense_gas_is_deterministic` compares a hash of the whole trajectory. It proves in-process reproducibility only, not reproducibility across runners or SIMD targets; the thresholds are the CI gate.
- Injected regressions are re-runnable: `SimulationWorld::test_faults` (test builds only) scales the Verlet movement threshold or the wall restitution. `test_dense_gas_catches_injected_stale_neighbor_lists` and `test_world_wall_catches_injected_restitution_gain` run the real simulation with a fault and expect the checks to fail. The non-ignored `*_checks_catch_injected_regressions` tests feed synthetic broken runs to the check functions.

## Precision far from the origin

`precision.rs` (P8a, #344) measures what f32 positions cost away from the
origin. It runs the dense gas (with projectiles) and the cool gas (without)
in the 100 x 100 drift world centred at x = y = 0, 125, 500 and 2000, each at
dt, dt/2 and dt/4 over the same physical time (1000, 2000, 4000 steps), and
prints one `DRIFT_PRECISION` line per gas and centre. It asserts that the
dense gas at the origin converges (dt/4 ratio <= 0.125, as
dense_gas_dt_quartering) and, since positions are f64 (#213), that the dt/4
drift at every centre stays within 1.6x (dense gas) or 4.0x (cool gas) of the
drift at the origin; see the scenario table. Position ulp is the f32 spacing
at the largest coordinate the gas reaches (centre + 20).

With f64 positions (#213, P8b), same runs:

| Gas | Centre | dt | dt/2 | dt/4 | dt/4 ratio | dt/4 over origin |
|---|---:|---:|---:|---:|---:|---:|
| dense (projectiles) | 0 | 3.51e-3 | 2.04e-3 | 4.26e-4 | 0.122 | 1.00 |
| | 125 | 3.52e-3 | 2.04e-3 | 4.25e-4 | 0.121 | 1.00 |
| | 500 | 3.51e-3 | 2.05e-3 | 4.51e-4 | 0.128 | 1.06 |
| | 2000 | 3.54e-3 | 2.04e-3 | 4.57e-4 | 0.129 | 1.07 |
| cool (no projectiles) | 0 | 9.00e-5 | 1.30e-5 | 1.50e-5 | 0.167 | 1.00 |
| | 125 | 9.11e-5 | 1.46e-5 | 1.46e-5 | 0.160 | 0.97 |
| | 500 | 9.75e-5 | 1.63e-5 | 1.22e-5 | 0.125 | 0.81 |
| | 2000 | 1.07e-4 | 1.98e-5 | 3.03e-5 | 0.283 | 2.01 |

The far runs now converge like the origin: the cool gas at 500 goes from
3.6e-4 to 1.2e-5 at dt/4 (30x), the dense gas at 2000 from 1.08e-3 to
4.6e-4. The small rest at 2000 (cool gas 2x the origin) comes from the parts
still in f32 (wall distances, the f32 copy of the position that other code
reads). The P8a measurement with f32 positions, which led to the change:

Max relative energy drift, seeded, dev profile on x86_64 (`d7bcf0c` + this test):

| Gas | Centre | Position ulp | dt | dt/2 | dt/4 | dt/4 ratio |
|---|---:|---:|---:|---:|---:|---:|
| dense (projectiles) | 0 | 1.9e-6 | 3.50e-3 | 2.04e-3 | 4.19e-4 | 0.120 |
| | 125 | 1.5e-5 | 3.52e-3 | 2.10e-3 | 3.51e-4 | 0.100 |
| | 500 | 6.1e-5 | 3.60e-3 | 2.75e-3 | 5.19e-4 | 0.144 |
| | 2000 | 1.2e-4 | 3.18e-3 | 2.64e-3 | 1.08e-3 | 0.339 |
| cool (no projectiles) | 0 | 1.9e-6 | 8.94e-5 | 1.25e-5 | 1.58e-5 | 0.177 |
| | 125 | 1.5e-5 | 8.35e-5 | 3.88e-5 | 8.45e-5 | 1.01 |
| | 500 | 6.1e-5 | 1.56e-4 | 8.74e-5 | 3.58e-4 | 2.29 |
| | 2000 | 1.2e-4 | 1.96e-4 | 3.40e-4 | 2.71e-4 | 1.38 |

Reading:

- The dense gas is dominated by the projectile pairs in the force-cap core,
  whose error is large at every dt, so rounding shows only at 2000 (the dt/4
  drift is 2.6x that at the origin, ratio 0.34 instead of 0.12).
- The cool gas shows the rounding directly. At the origin it reaches a floor
  of ~1.3e-5 already at dt/2 (the cutoff force jump and f32 sums, see the
  timestep note above). At 125, where a world of `--world-scale 2` or more
  already reaches, the floor is ~8e-5: smaller steps no longer help at all
  (ratio 1.0). At 500 (`--world-scale 8` reaches ~450) the drift *grows*
  as dt shrinks (ratio 2.3): every step rounds the position update, and more
  steps round more often. At 2000 the error is ~10x the origin value at dt
  and ~17x at dt/4.
- Two effects are mixed: rounding of the accumulated position (grows with
  the number of steps) and the coarse resolution of the pair distances the
  forces are computed from (at 2000 an ulp is 5e-5 of the equilibrium
  distance, which turns into force noise of ~13 x that).

Recommendation for #213 (P8b), implemented there: **f64 positions** (option 1 of the issue),
with everything else in f32. Positions are only ever updated by small f32
increments and read as differences; with f64 the increment is not rounded
away and pair distances are computed as an f64 difference then narrowed to
f32 for the SIMD kernel, so neither effect remains. Compensated f32
integration (option 3) removes only the accumulation and keeps the coarse
distances (the 2000 row would stay noisy); cell-relative coordinates
(option 2) fix both but touch every position consumer (cell moves, walls,
operators, snapshots) for the same result. The cost of f64 positions is 8
more bytes per ball and an f64 subtraction per gathered neighbour; P6 (#346)
puts positions into a hot array of their own first, so the change stays
local to that array, the kernel gather and the position update.

## CI reporting

The drift steps of the `test` job always upload `drift-harness.log` and write to the job summary:

- the runner arch, CPU model and best host SIMD flag, plus the compiled `target_feature`s, which decide the SIMD path the baselines depend on;
- every `DRIFT_SCENARIO`, `DRIFT_CONVERGENCE` and `DRIFT_PRECISION` line (matched anywhere in a log line: tests run in parallel, so a line can start after another test's output);
- every `DRIFT_WARNING` line, also emitted as a `::warning` annotation. A scenario prints one when a passing metric exceeds 70% (`DRIFT_WARNING_FRACTION`) of its threshold, so shrinking margins show up before they fail the build.

## Changing thresholds

Threshold updates are allowed only when behavior changed intentionally (for example integrator/physics updates) and the PR provides baseline evidence. Never loosen thresholds only to clear unrelated failures.

### Capturing a baseline

1. Run drift scenarios locally with deterministic inputs:
   ```bash
   cargo ci-test-drift   # = cargo test -p thermo_physics -- --ignored --nocapture
   ```
2. Collect the `DRIFT_SCENARIO` lines from:
   - current `master` (before change),
   - your branch (after change).
3. Keep the same seed/steps/scenario setup for both runs.

### Deriving a threshold

- Keep thresholds per scenario and per metric.
- Start from observed baseline on the changed branch.
- Use an explicit safety margin (for example 1.5-2.0x baseline drift) and round to a stable scientific-notation value.
- Prefer the smallest threshold that keeps deterministic reruns stable.

### PR justification

Any PR that modifies drift thresholds must include:

- reason for the physical/numerical behavior shift,
- before/after baseline summary lines,
- chosen threshold and margin rationale,
- linked issue describing the intended behavior change.

Use this compact table in the PR description:

| Scenario | Metric | Old threshold | New threshold | Baseline before | Baseline after | Margin | Reason |
|---|---|---:|---:|---:|---:|---:|---|
| two_body_lj_baseline | relative_energy_drift | 3.0e-2 | 4.5e-2 | 1.8e-2 | 2.2e-2 | 2.0x | Integrator update |

## Baseline history

Changes that moved the baselines, newest last. Thresholds changed only where
stated.

**Cutoff change.** The LJ interaction is now truncated at `lj_cutoff` and shifted to zero there (previously every listed pair interacted up to the list radius, so each neighbour-list change added or removed energy; see `BallPhysics::potential`). dense_gas baselines were re-measured after that change: max relative energy drift 2.05e-3 -> 1.91e-3, relative momentum drift 5.2e-9 -> 5.1e-8. The momentum value is f32 rounding noise, not an asymmetry (no pair is inside the cutoff for one ball and outside for the other): repeating the scenario with thermal speeds 1.00..1.35 gives 1.7e-8..7.4e-8 with the cutoff and 9.0e-8..3.0e-7 without it, so the old 5.2e-9 baseline was a lucky realisation and its 5.0e-8 threshold sat inside the noise band. The threshold is now 1.5e-7.

**Sorted neighbour lists.** Verlet lists are now kept sorted by ball index and rebuilt from the 3x3 cells only, and the cell grid rounds its cell count down so every cell is at least cutoff + skin + 2 x max radius wide (it rounded up before, so cells could be slightly smaller). The force kernel sums neighbours in list order, so the f32 sums changed order. dense_gas re-measured: max relative energy drift 1.914e-3 -> 1.914e-3, final 1.259e-3 -> 1.185e-3, relative momentum drift 5.06e-8 -> 7.79e-8 (inside the 1.5e-7 threshold and below the 70% warning line), rebuild rate 2.41e-2 -> 2.41e-2, no missed or asymmetric pairs. The wall-reflection scenarios are unchanged. Thresholds are unchanged.

**Drift world moved to the origin** (from x, y ~ 500..600, see the timestep note under Notes). Re-measured, thresholds unchanged: dense_gas max relative energy drift 1.914e-3 -> 2.049e-3, final 1.185e-3 -> 1.404e-3, relative momentum drift 7.79e-8 -> 5.59e-8, rebuild rate 2.41e-2 -> 2.43e-2, wall clearance 20.24 -> 20.07; elastic_line_segment energy drift 1.5e-6 -> 5.5e-7, elastic_world_wall 0 -> 0. Positions that tests used to hardcode in the old world (two-body and force-cap pair centres, the wall-test start and the line-segment barrier) are now offsets from `world_center`.

**Sloped core (#240).** Below `r_cap` the pair force now rises as `max_force + core_stiffness·(r_cap - r)` (k = 200) instead of staying flat at `max_force`, with the matching quadratic potential. With a flat cap, crowded neighbours pinned deeply overlapping pairs (no force gradient pushes them apart). Results above `r_cap` are bit-identical; only pairs inside the core change. Re-measured, thresholds unchanged: dense_gas max relative energy drift 2.049e-3 -> 2.000e-3, final 1.404e-3 -> 1.347e-3, relative momentum drift 5.59e-8 -> 8.06e-8, cap hits 39 -> 40; hot_sparse_gas max relative energy drift 4.75e-3 -> 6.53e-3 (82% of its threshold, so CI prints a warning) and max overlap depth 0.57 -> 0.097; balls_removed_and_added max overlap depth 0.58 -> 0.043. The hot-gas value is realisation noise, not a systematic increase: over five initial speeds (`HOT_V` 5.0, 5.5, 6.0, 6.5, 7.0) the max drift averages 3.85e-3 with the flat cap (range 1.7e-3..5.3e-3), 2.99e-3 with k = 100 and 3.65e-3 with k = 200 (range 1.9e-3..6.5e-3), and HOT_V = 6.0 is the worst of the five at k = 200. mixed_sizes_cell_borders (no energy threshold) moved 8.2e-3 -> 1.5e-2.

**Re-measurement on `master` at `6349f72`** (after #240, #242 and #244): every value in the scenario table is unchanged except dense_gas_dt_quartering, whose ratio is now 0.095 (coarse 2.000e-3, fine 1.893e-4). The table showed 0.064, measured before #240; the change comes from #240 (0.064 at `b599515^`, 0.095 at `b599515`). Only pairs inside `r_cap` changed there, which the dense gas's projectile pairs reach (40 cap hits); the fine run's max drift rose 1.32e-4 -> 1.89e-4 while the coarse run's fell 2.05e-3 -> 2.00e-3. It passes (threshold 0.125), and CI prints a `DRIFT_WARNING` at 76%. The threshold is unchanged. The two-body, force-cap and two-body dt-halving tests now print summary lines too; their values were measured in the same run and added to the table.

**Force-cap distance (#276).** `r_cap` solves `F_LJ(r_cap) = max_force`. The Newton step used `14·(σ/r)^12` instead of `26·(σ/r)^12` in `dF/dr`, so it never converged and a bisection over `[0.1σ, σ]` set `r_cap`, to within 0.1 % of `max_force` (default `r_cap` 0.919360 -> 0.919338). For caps below `F_LJ(σ) = 24ε/σ` the root lies above σ, so the bisection pinned `r_cap` at σ: in force_cap_transition (cap 10) the core force started at 24 while the core potential's slope started at 10, so F was not -dU/dr below `r_cap` (now `r_cap` = 1.0425). Newton now uses the correct derivative and converges; the fallback bisects over `[0.1σ, equilibrium distance]`. Only pairs inside the core change. Re-measured: force_cap_transition relative energy drift 5.27e-1 -> 1.40e-1 (threshold 7.0e-1 -> 3.0e-1, ~2.1x), cap hits 12 -> 14; hot_sparse_gas max relative energy drift 6.53e-3 -> 2.91e-3, so its CI warning is gone (threshold unchanged: over initial speeds the value is realisation noise, see #240 above); dense_gas max relative energy drift 2.000e-3 -> 2.021e-3, final 1.347e-3 -> 1.377e-3, relative momentum drift 8.06e-8 -> 9.83e-8 (66% of its threshold); dense_gas_dt_quartering ratio 0.095 -> 0.094; balls_removed_and_added max overlap depth 4.3e-2 -> 2.8e-2; mixed_sizes_cell_borders (no energy threshold) 1.5e-2 -> 1.7e-2. two_body_lj_baseline, two_body_dt_halving and the wall-reflection scenarios never reach the core and are bit-identical. The seeded operators headless fixture changed (energies in the 4th-5th significant digit, positions by up to 3e-4); the plain seeded fixture did not.

**Species: Lennard-Jones on the centre distance (#234).** The pair potential acts on the centre distance with a per-species-pair `σ_ij` instead of on the edge distance with one `σ`; walls reflect at `σ_i / 2` instead of the ball radius. The scenarios now use species `M` explicitly (the default for new balls became `N2`), the size class they were built with: as a species it keeps its mass and its like-pair equilibrium distance (2.32), but its core is softer (`r_cap` at centre distance 1.78, `U(r_cap) ≈ 14.5 ε`, where the edge-distance core started at 2.12 and ≈ 4.4 ε), it reaches further (cutoff 8.28 instead of 5.2) and stays 1.03 from walls instead of 0.6. Scenario changes: the two-body scenarios start at 1.05 x the `M`–`M` equilibrium distance (2.44, before 2.38); force_cap_transition places its balls without a placement check, since inside the core their contact discs overlap; dense_gas projectiles move at ±3.5 instead of ±2.0: ±2.0 no longer reaches the core (0 cap hits), and ±3.5 (≈ 21 ε per pair) drives the projectile pairs well into it, the more informative check of the new core. Re-measured against the part-1 branch (`ac0cf4c`, same results as `master`), thresholds unchanged: dense_gas max relative energy drift 2.02e-3 -> 3.50e-3 (88 % of its 4.0e-3 threshold, so CI prints a `DRIFT_WARNING`; kept deliberately, the threshold is not raised), final 1.38e-3 -> 2.71e-3, momentum 9.8e-8 -> 8.0e-9, rebuild rate 2.4e-2 -> 3.0e-2, cap hits 40 -> 93, wall clearance 20.07 -> 10.55; dense_gas_dt_quartering 0.094 -> 0.120 (coarse 3.50e-3, fine 4.19e-4; 96 % of its 0.125 threshold, CI warns: the harder core hits converge more slowly than dt²); two_body_lj_baseline final 4.46e-3 -> 2.14e-3; two_body_dt_halving 5.99e-3 / 2.98e-3 -> 2.83e-3 / 1.41e-3; force_cap_transition 1.40e-1 -> 8.31e-2; hot_sparse_gas 2.91e-3 -> 1.37e-3; balls_removed_and_added re-adds 21 balls instead of 38 (larger wall and contact distance) with max drift 4.15e-3 -> 5.96e-4; mixed_sizes_cell_borders places 54 balls instead of 21 (its world is sized in `XL`–`XL` list radii, now 21.9) and has no energy threshold (max drift 9.6e-2, before 1.7e-2); elastic_line_segment sees 26 reflections instead of 25. Both headless fixtures changed (`N2` balls, `headless.v2`).

**Binary mixture (#235).** New scenario binary_mixture (`binary_mixture.rs`): `N2` and a second species with σ ratio 1 : 1.2 and ε ratio 1 : 1.5, made from the built-in `XS` (mass 1) by two `[[potential.pair_override]]` entries (`XS`–`XS` σ = 1.2, ε = 1.5; `N2`–`XS` σ = 1.1, ε = √1.5), on a checkerboard at spacing 1.3 with thermal speed 1.5. Baseline on `feature/master/235-pair-overrides` (x86_64, dev profile): max relative energy drift 5.67e-4, final 2.39e-4, relative momentum drift 1.79e-8, 4 cap hits, rebuild rate 2.5e-2, no missed or asymmetric pairs, wall clearance 22.27, the same trajectory hash on 1 and 4 threads. Thresholds: energy 1.0e-3 (~1.8x), momentum 1.0e-7. The energy value depends on the realisation: thermal speeds 1.4 / 1.6 and spacings 1.28 / 1.32 / 1.31 (at 1.45) give 5.66e-4, 8.22e-4, 5.98e-4, 2.18e-3 and 6.69e-4, growing with the number of cap hits (0..13); momentum 1.6e-8..5.4e-8. A change to the start state therefore needs a new baseline. Momentum threshold basis: ~2x the largest value over the nearby starts (5.4e-8), as for dense_gas. Overlaps (`min_edge_distance` -0.093) are measured from each pair's contact distance `σ_ij` (`BallPhysics::edge_gap`, which follows the overrides). The other scenarios have no overrides and are bit-identical to `master` (same `trajectory_hash` values).

**Default time step 0.01 -> 0.005.** New worlds now start at dt = 0.005 (`DEFAULT_TIME_STEP`, about 11 fs in N2 units). The drift suite and the core tests keep their own step (`DT_GOAL` = 0.01): their worlds come from `test_config()` / `new_main_content()` in `core/tests/mod.rs`, so every scenario and threshold is bit-identical to `master`. The accepted range moved with the default (÷16..×16: 0.0003125..0.08). Both headless fixtures changed, because they run with the default step.

**f64 positions (#213).** A ball's position is f64 (`Ball2D::exact`, with an f32 copy for every reader); the box-wall position update runs in f64 and the force kernel takes pair differences in f64 (`KernelPoint` holds f64 coordinates) before narrowing them to f32. Line-wall contacts still place the ball in f32. Every trajectory moves by rounding, also at the origin. Re-measured against #346 (dev profile, x86_64), thresholds unchanged: dense_gas max relative energy drift 3.504e-3 -> 3.506e-3 (88 % of its threshold, CI warns as before), final 2.706e-3 -> 2.704e-3, momentum 8.0e-9 -> 2.9e-8 (19 % of 1.5e-7; f32 rounding noise, see the cutoff entry); dense_gas_dt_quartering 0.120 -> 0.122 (97 %, warns as before); binary_mixture 5.668e-4 -> 5.674e-4, momentum 1.8e-8 -> 5.8e-8 (58 % of 1.0e-7, below the warning line); hot_sparse_gas 1.373e-3 -> 4.643e-4; elastic_line_segment 3.0e-7 -> 4.2e-7; mixed_sizes_cell_borders 3.72e-1 -> 3.74e-1; two_body_lj_baseline, force_cap_transition and elastic_world_wall unchanged. balls_removed_and_added now tries 120 spots instead of 60 to re-add balls: with 60 it added 21 before and 19 after the change, below its validity bound of 20; with 120 it adds 46 (max drift 5.46e-4). Its max overlap depth is ~1.1 on both sides (1.26 before the change); the 2.8e-2 recorded in the #276 entry no longer applies. `precision.rs` became a gate (see [Precision far from the origin](#precision-far-from-the-origin)). Both headless fixtures changed (positions and energies in the last digits).
