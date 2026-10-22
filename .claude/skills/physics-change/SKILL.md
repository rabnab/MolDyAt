---
name: physics-change
description: Contract for changing simulation physics - pair forces and potentials, force capping, cutoff, the integrator, wall contacts, neighbour lists, the cell grid, the step order, thermostats, or anything that changes simulated trajectories. Use before editing ball_physics.rs, pair_potential.rs, simd_forces.rs, verlet.rs, wall_contact.rs, simulation_engine.rs, cell_grid.rs or the thermostat/source physics.
---

# Physics Change Contract

The simulation must conserve energy where the physics says it should, and seeded runs must stay reproducible. Every physics change proves both with tests and, if results move, with baseline evidence.

## Invariants
- **Force and potential match.** `F = -dU/dr` everywhere, in the scalar path (`PairPotential::force` / `potential`) and the SIMD path (`force_simd` / `potential_simd`), for every `PairLaw` and in `CappedPotential`. Any change to one needs the matching change to the other and to both paths.
- **Cutoff.** Distances are centre distances, and every species pair has its own potential (`PairTable`, Lorentz–Berthelot mixing unless the config overrides the pair, #235). A pair interacts only below its `cutoff_ij` (`PairPotential::cutoff`); `U` is shifted by `U(cutoff_ij)` so it is zero there. Pairs in a neighbour list beyond their cutoff contribute no force and no energy.
- **Force cap.** Below `r_cap` the force is `max_force + core_stiffness * (r_cap - r)` with `U(r) = U(r_cap) - max_force * (r - r_cap) + 0.5 * core_stiffness * (r - r_cap)^2`. Cap and cutoff shift are implemented once in `CappedPotential`, generic over the `PairLaw`; `r_cap` is cached there and computed by `force_cap_distance()`. Do not flatten the cap (#240). `max_force`, `core_stiffness` and `cutoff_factor` are `SimConfig` keys (`[potential]`, #231); their defaults are the reference physics, and saves without a config run with them.
- **Neighbour lists.** A ball rebuilds its Verlet list when its displacement since its last rebuild exceeds skin/3 (`get_min_movement_for_update_hood`); rebuilds are per ball and asynchronous, so skin/2 is not enough. A pair is listed below its list radius `cutoff_ij + skin`. Cells are at least the largest list radius among the species present (cell count rounded down); `fit_cell_grid` grows them before the next rebuild when a larger species appears. Lists are sorted by ball index. The skin is `skin_factor · cutoff_factor · σ_N2` (`[neighbours]` in `SimConfig`).
- **Index contract.** Cells and Verlet lists store indices into `balls`. After `retain`/`remove`/`swap_remove`, clear all neighbour lists and rebuild the cell grid immediately (`remove_invalid_balls`). Never keep indices across a structural change.
- **Walls.** Contacts are resolved along the wall normal in the position update (`ball_impl::wall_contact`); never project or mirror the whole overshoot. See the `operator-framework` skill for line walls and valves.
- **Species, not radii.** Physics reads `σ`, `ε` and mass from the species; `display_radius` is drawing only (`display_radius_does_not_change_physics`). Walls take their contact distance from the `WallLaw` (`σ_i / 2`). Balls with NaN positions are removed.
- **Reproducibility.** Results must not depend on the Rayon thread count or scheduling: no shared mutable state in force loops, no reductions whose order depends on work splitting. Sums over balls run in a fixed order.
- **Precision.** f32 positions lose resolution far from the origin (~3e-5 at 500). Tests build worlds centred on the origin.

## Tests to run
Smallest first; all must pass before a PR.

| Check | Command |
|---|---|
| Force/potential, cap, SIMD vs scalar | `cargo test -p thermo_physics ball_impl` |
| Energy conservation | `cargo test -p thermo_physics energy` (covers `energy_conservation.rs`, `barrier_energy.rs`) |
| Full physics suite | `cargo test -p thermo_physics` (physics thread and snapshots: `cargo test -p moldyat_core`) |
| Drift suite | `cargo ci-test-drift` |
| Headless fixtures | `cargo ci-test-headless-regression` |
| Thread-count reproducibility | `cargo test -p moldyat_wgpu test_seeded_operators_run_does_not_depend_on_thread_count` |
| Lint | `cargo fmt` and `cargo clippy --workspace -- -D warnings` |

A change to the force or potential needs a new or updated energy test. A new physics effect needs a test that fails without it.

## When results change
Intended trajectory changes (new potential, step order, summation order) move the drift baselines and the headless fixtures.

1. Run `cargo ci-test-drift` on `master` and on the branch with the same target and profile, and collect the `DRIFT_SCENARIO` / `DRIFT_CONVERGENCE` lines. `simd_forces` is compiled for the build target (`dev`: baseline SSE2/NEON; `release` and `perf`: `target-cpu=native`), so compare only like with like.
2. Thresholds change only with an intentional behaviour change. Follow `docs/drift-test.md` (Changing thresholds): margin 1.5–2x the new baseline, per scenario and metric, and the PR table with old/new thresholds and baselines.
3. Add an entry to "Baseline history" in `docs/drift-test.md` and update the scenario table.
4. Refresh the fixtures only after the drift suite is understood: `UPDATE_HEADLESS_FIXTURE=1 cargo ci-test-headless-regression`, then rerun without the variable. Refresh on x86_64 Linux with the dev profile, as CI does; never on Windows, where `sin`/`cos`/`exp` can give different bits. Include the fixture diff in the PR (details in the `headless-contract` skill).
5. Explain in the PR why the results moved. A `DRIFT_WARNING` (metric above 70 % of its threshold) needs a sentence, not a silent pass.

Never loosen a threshold to clear an unrelated failure; find the cause.

## Performance-sensitive code
Force kernels, neighbour rebuilds and the apply pass are hot. If a change touches them, compare binaries with `scripts/headless_perf.sh BASE_BIN NEW_BIN` (see `docs/perf-baseline.md`) and report the `total` and changed-phase medians.

## Documentation
Update the docs that describe the changed behaviour in the same PR: `docs/units.md` (potential, units), `docs/simulation.md` (cell grid, thermo fields), `docs/architecture.md` (step order), `docs/operators.md` (wall and thermostat physics), and the invariants in `.github/copilot-instructions.md`.
