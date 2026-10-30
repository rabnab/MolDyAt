---
name: physics-change
description: Contract for changing simulation physics - pair forces and potentials, force capping, cutoff, the integrator, wall contacts, neighbour lists, the cell grid, the step order, thermostats, or anything that changes simulated trajectories. Use before editing ball_physics.rs, pair_potential.rs, simd_forces.rs, verlet.rs, wall_contact.rs, simulation_engine.rs, cell_grid.rs or the thermostat/source physics.
---

# Physics Change Contract

The simulation must conserve energy where the physics says it should, and seeded runs must stay reproducible. Every physics change proves both with tests and, if results move, with baseline evidence.

## Invariants
Read `.github/instructions/physics.instructions.md` first: force and potential match, cutoff, force cap, neighbour lists, index contract, walls, species, reproducibility and precision. Line walls and valves: `operator-framework` skill.

## Tests to run
Smallest first; all must pass before a PR.

| Check | Command |
|---|---|
| Force/potential, cap, SIMD vs scalar | `cargo test -p thermo_physics ball_impl` |
| Energy conservation | `cargo test -p thermo_physics energy` (covers `energy_conservation.rs`, `barrier_energy.rs`) |
| Full physics suite | `cargo test -p thermo_physics` (physics thread, snapshots and headless: `cargo test -p thermo_runtime`) |
| Drift suite | `cargo ci-test-drift` |
| Headless fixtures | `cargo ci-test-headless-regression` |
| Thread-count reproducibility | `cargo test -p thermo_runtime test_seeded_operators_run_does_not_depend_on_thread_count` |

A change to the force or potential needs a new or updated energy test. A new physics effect needs a test that fails without it.

## When results change
Intended trajectory changes (new potential, step order, summation order) move the drift baselines and the headless fixtures.

1. Run `cargo ci-test-drift` on `master` and on the branch with the same target and profile, and collect the `DRIFT_SCENARIO` / `DRIFT_CONVERGENCE` lines. `simd_forces` is compiled for the build target (`dev`: baseline SSE2/NEON; `release` and `perf`: `target-cpu=native`), so compare only like with like.
2. Thresholds change only with an intentional behaviour change. Follow `docs/drift-test.md` (Changing thresholds): margin 1.5–2x the new baseline, per scenario and metric, and the PR table with old/new thresholds and baselines.
3. Add an entry to "Baseline history" in `docs/drift-test.md` and update the scenario table.
4. Refresh the headless fixtures only after the drift suite is understood, as the `headless-contract` skill (Fixtures) describes.
5. Explain in the PR why the results moved. A `DRIFT_WARNING` (metric above 70 % of its threshold) needs a sentence, not a silent pass.

Never loosen a threshold to clear an unrelated failure; find the cause.

## Performance-sensitive code
Force kernels, neighbour rebuilds and the apply pass are hot. If a change touches them, follow the `perf-compare` skill.

## Documentation
Update the docs that describe the changed behaviour in the same PR: `docs/units.md` (potential, units), `docs/simulation.md` (cell grid, thermo fields), `docs/architecture.md` (step order), `docs/operators.md` (wall and thermostat physics), and the invariants in `.github/instructions/physics.instructions.md`.
