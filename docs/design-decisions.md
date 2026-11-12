# Design Decisions

Decisions that bind every change. They were made while the
[refactoring plan](done/refactoring-plan.md) was carried out; the numbers
are the plan's, so older issues and PRs that cite "decision N" still match.
Changing one needs the owner's agreement and an update here.

## Scope

- The project stays 2D and CPU-physics: no GPU compute. A GPU force pass
  was measured and dropped (#357): it was 18–72× slower than the CPU step,
  and forces are only part of a step.
- wgpu is the only renderer backend.

## Reproducibility and saves

- Seeded runs on the same target and profile are deterministic; a change
  that alters results needs a drift baseline update per
  [drift-test.md](drift-test.md).
- **4.** Seeded runs are bit-reproducible across thread counts by default. A
  scheduling-dependent reduction is allowed where it buys speed, under the
  thread-count rule in [CONTRIBUTING](../CONTRIBUTING.md).
- Save files may get a format version bump with a migration; old saves keep
  loading.

## Structure

- **1.** The code is split into the crates described in
  [architecture.md](architecture.md#crates).
- **3.** Plots live in a separate wgpu window.
- **12.** Every PR updates the docs, skills and project rules its change
  touches; there is no separate docs step afterwards.

## Particles and interactions

- **2.** Species replace the old size classes; old saves migrate.
- **5.** Mixed-species simulation is a core feature.
- **7.** Interaction is Lennard-Jones on the centre distance; the radius is
  adjustable but visual only.
- **8.** Hard wall contact is at centre distance `σ_i / 2`, which matches the
  closest approach of two LJ balls (≈ 0.97 σ at T* = 1). The soft wall
  potential keeps the hard contact as a backstop.
- **9.** Every species pair has its own entry in the pair table, with its own
  `σ_ij` (arithmetic mixing) and `r_cap`.
- **10.** The cutoff is `4·σ_ij` per pair; the cell grid is sized for the
  species present in the world, not for the whole table, so worlds without
  large species keep small cells.
- **11.** `N2` is the default species for new balls and source spheres; the
  size classes `XS`–`XL` stay as built-in species for migrated saves.
- **13.** Water is the Mercedes-Benz model. Atomistic water (SPC/E, TIP4P) is
  not planned.
- **14.** Soft walls are the default; saves before format v4 run with hard
  walls.
- **15.** Ar is the unit species and defines the reduced units (replaces
  decision 6, N₂); old saves are rescaled to the same physical state.
