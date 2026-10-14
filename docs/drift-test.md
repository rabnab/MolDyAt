# Drift Tests

The drift suite runs deterministic, headless simulations for many steps and
checks how far conserved quantities (energy, momentum) depart from their
initial values. Each scenario isolates one source of artifacts: the
integrator, the force-cap region, stale neighbour lists, or wall contacts.
Every metric has its own threshold, and every run prints a machine-readable
summary line, so a CI failure can be diagnosed without rendering.

## Running

```bash
cargo ci-test-drift   # = cargo test -p moldyat_core -- --ignored --nocapture
```

All scenarios are `#[ignore = "suite:drift"]`, so a plain `cargo test` skips
them. CI runs the alias in a step of the `test` job, after the unit tests and
reusing their build. The alias runs every ignored test of `moldyat_core`, so
it also runs the two `#[ignore = "suite:physics (slow)"]` tests
(`test_overlap_fallback_stays_rare_in_a_gas_with_walls`,
`test_virial_pressure_2d_matches_wall_pressure`); they are not drift
scenarios and print no summary line.

Commands follow the
[cargo command policy](../.github/copilot-instructions.md#cargo-command-policy-source-of-truth).

## Harness

The harness is `core/src/application/core/tests/drift_harness.rs`; the
scenarios are in `core/src/application/core/tests/drift_harness/`
(`dense_gas.rs`, `neighbor_lists.rs`, `wall_reflection.rs`, `tests.rs`).

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
  minimum edge distance. A stale list that lasts only a few steps is caught.
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

`relative energy drift` is `|E_t - E_0| / max(|E_0|, eps)`.

`warn_near_threshold` prints a `DRIFT_WARNING name=… metric=… value=…
threshold=… ratio=…` line when a passing metric exceeds
`DRIFT_WARNING_FRACTION` (70 %) of its threshold, so shrinking margins show
up before they fail the build.

## Scenarios and thresholds

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
| dense_gas_dt_quartering (dense gas at dt and dt/4, same physical time) | `dense_gas.rs` | max energy drift at dt/4 over that at dt | <= 0.125 | 0.095 | ~1.3x (dt^2 gives 0.0625); 76% of the threshold, CI warns, see [Baseline history](#baseline-history) |
| two_body_lj_baseline (2 balls near equilibrium distance, 2000 steps) | `tests.rs` | relative energy drift (final) | 3.0e-2 | 4.46e-3 (max 4.59e-3) | ~6.7x |
| | | relative momentum drift | 3.0e-3 | 0 | |
| force_cap_transition (2 balls driven into the force cap, cap lowered to 10, 300 steps) | `tests.rs` | relative energy drift (final) | 7.0e-1 | 5.27e-1 | ~1.3x; the cap region is not energy-conserving at this dt, the test checks continuity, not conservation |
| | | relative momentum drift | 3.0e-3 | 0 | |
| | | force-cap region reached (max cap-hit ratio) | >= 1.0e-2 | 1.0 | |
| two_body_dt_halving (two-body scenario at dt and dt/2, same physical time) | `tests.rs` | relative energy drift at dt/2 vs at dt | <= 2 x coarse | 2.98e-3 vs 5.99e-3 (ratio 0.50) | weak check: it only rejects drift that grows when dt shrinks; the two-body drift converges ~linearly in dt, not as dt^2 |
| hot_sparse_gas (400 balls over the whole world, speeds up to ~8, 1000 steps) | `neighbor_lists.rs` | max relative energy drift | 8.0e-3 | 6.53e-3 | ~1.2x; realisation noise, see [Baseline history](#baseline-history) |
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

**Baseline provenance.** Measured with `cargo ci-test-drift` (dev profile, which is optimized) on an x86_64 host, on the PR #205 branch on top of `master` at `4f8b801`, after the Verlet movement threshold changed to skin/3. `simd_forces` uses `std::simd` (`f32x4`), so the SIMD path is fixed at compile time by the target features: the dev profile sets no `target-cpu`, so x86_64 drift builds use SSE2 even on AVX2/AVX-512 hosts (aarch64 uses NEON). Only the release profile uses `target-cpu=native` (with AMX disabled, see the workspace `Cargo.toml`). f32 results can differ between targets or `RUSTFLAGS`; rerun on the same target when comparing baselines.

Notes:

- The dense-gas grid starts in the world centre and the run asserts the balls never reach the wall region, so wall effects stay isolated in the reflection scenarios. The projectile pairs drive a few pairs into the force-cap region so the cap and overlap checks are not vacuous; this is also why the energy envelope is wider than a cool gas (7.2e-4) would need.
- Neighbour-list and overlap checks run after every step, not only on sample steps, so a stale list that lasts only a few steps before the next rebuild is still caught.
- Verlet movement threshold: balls rebuild their neighbour lists independently, so a ball may already be 2 thresholds from its last rebuild position when its partner rebuilds, and the partner may move one more. The threshold is therefore skin/3 (not the textbook skin/2 for synchronous rebuilds). With skin/2, the cool dense gas (no projectiles) had an in-cutoff pair missing from both lists at step 186. `test_cool_dense_gas_has_no_stale_neighbor_lists` checks both: skin/3 has no stale pairs, and skin/2 still reproduces the miss (so the test is not vacuous).
- Timestep convergence and the drift world's position: the drift world (`create_dense_harness_with_seed`) is 100 x 100 centred on the origin. It used to sit at x, y ~ 500..600, where an f32 resolves ~3e-5, while a step moves a ball ~1e-2 and the 1/2 a dt^2 term is ~5e-5. That rounding does not shrink with dt, and smaller steps make more of it, so the many-body energy envelope shrank only about linearly with dt there (dense gas max drift 1.91e-3, 1.56e-3, 0.67e-3 at dt, dt/2, dt/4). At the origin it shrinks about as dt^2 (2.05e-3, 0.82e-3, 0.13e-3). The force jump at the cutoff (|F(lj_cutoff)| ~ 1.5e-3; the potential is shifted to zero there, the force is not) is not a significant source: a force-shifted potential changed the results at 500 only within noise, and at the origin it lowers the cool gas at dt/4 from 0.64e-4 to 0.27e-4. The app's default world is also centred on the origin; large worlds (e.g. `--world-scale 8`, coordinates up to ~450) lose position precision the same way.
- Neighbour-list scenarios (`neighbor_lists.rs`): the hot sparse gas makes ~22% of balls rebuild per step, so many partner updates of one parallel rebuild pass meet; `test_hot_sparse_gas_catches_injected_stale_neighbor_lists` runs it with a 10x movement threshold and expects stale lists. The cell-border scenario places two pairs of the largest balls so that a grid with cells narrower than cutoff + skin + 2 x max radius misses them; with the old rounded-up cell count it fails at step 0 (2 pairs missing). The removal/add scenario panics on a stale index if `remove_invalid_balls` stops clearing the lists. Its segments drop the step-0 sample after each structural change, because the cleared or new lists are rebuilt at the start of the next step, before any force is computed. The thread-count test runs the scenarios on dedicated Rayon pools; it proves that the parallel passes do not depend on how work is split, on the same target.
- `rel_momentum_drift` in the wall scenarios is expected to be ~2: walls absorb momentum by design.
- `test_dense_gas_is_deterministic` compares a hash of the whole trajectory. It proves in-process reproducibility only, not reproducibility across runners or SIMD targets; the thresholds are the CI gate.
- Injected regressions are re-runnable: `SimulationWorld::test_faults` (test builds only) scales the Verlet movement threshold or the wall restitution. `test_dense_gas_catches_injected_stale_neighbor_lists` and `test_world_wall_catches_injected_restitution_gain` run the real simulation with a fault and expect the checks to fail. The non-ignored `*_checks_catch_injected_regressions` tests feed synthetic broken runs to the check functions.

## CI reporting

The drift steps of the `test` job always upload `drift-harness.log` and write to the job summary:

- the runner arch, CPU model and best host SIMD flag, plus the compiled `target_feature`s, which decide the SIMD path the baselines depend on;
- every `DRIFT_SCENARIO` and `DRIFT_CONVERGENCE` line (matched anywhere in a log line: tests run in parallel, so a line can start after another test's output);
- every `DRIFT_WARNING` line, also emitted as a `::warning` annotation. A scenario prints one when a passing metric exceeds 70% (`DRIFT_WARNING_FRACTION`) of its threshold, so shrinking margins show up before they fail the build.

## Changing thresholds

Threshold updates are allowed only when behavior changed intentionally (for example integrator/physics updates) and the PR provides baseline evidence. Never loosen thresholds only to clear unrelated failures.

### Capturing a baseline

1. Run drift scenarios locally with deterministic inputs:
   ```bash
   cargo ci-test-drift   # = cargo test -p moldyat_core -- --ignored --nocapture
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
