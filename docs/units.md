# Reduced Units and Scales

The simulation runs in reduced Lennard-Jones units with argon as the reference substance. SI conversions are used only for the real-units display (key `D`). The constants live in `core/src/application/sim_stats.rs`; keep this document in sync with them.

## Pair potential

`U(r) = 4 ε_ij ((σ_ij/r)^12 − (σ_ij/r)^6)`, with `r` the centre distance. Each ball has a species with its own `σ`, `ε` and mass; a pair uses Lorentz–Berthelot mixing, `σ_ij = (σ_i + σ_j)/2` and `ε_ij = √(ε_i ε_j)` (`PairTable` in `pair_potential.rs`, #234).

- σ_ij is the zero crossing of `U`; the drawing radius of a ball (`display_radius`, default `σ/2`) has no effect on the physics.
- The minimum (equilibrium distance) is at `r_m = 2^(1/6) σ_ij ≈ 1.1225 σ_ij`.
- The code (`core/src/objects2d/ball_impl/pair_potential.rs`) separates the law (`LennardJones`, a `PairLaw`) from the force cap below `r_cap` and the cutoff at `4 σ_ij`, where `U` is shifted to zero (`CappedPotential`), one per species pair.
- Walls reflect a ball at centre distance `σ_i / 2` (`WallLaw`, hard contact).

### Built-in species

| Species | σ* | ε* | m* | Note |
|---|---|---|---|---|
| N2 | 1 | 1 | 1 | default for new balls; the unit species |
| XS, S, M, L, XL | `2·R·2^(-1/6) + 1` (1.27, 1.53, 2.07, 3.14, 5.28) | 1 | 1.0 … 6.16 | the former size classes of radius `R` = 0.15 … 2.4 |

Before #234 the potential acted on the edge distance (centre distance minus both radii) with `σ = 1` for every ball. With `σ_i = 2·R_i·2^(-1/6) + 1` and arithmetic mixing, every pair of former size classes keeps its old equilibrium centre distance `R_i + R_j + 2^(1/6)`, so saves migrate (format v2). The shape of the potential still changes: the larger size classes are softer and reach further (cutoff `4 σ_ij`), and they stay `σ_i / 2` from walls instead of `R_i` (e.g. `M`: 1.03 instead of 0.6).

The reduced units below still use argon for the SI display; `N2` here means only the unit species until the units move to N₂ (#236).

## Reduced units

All internal quantities are reduced (marked `*`).

| Reduced | Quantity | SI value (argon) |
|---|---|---|
| σ* = 1 | length | σ = 3.405E-10 m |
| ε* = 1 | energy | ε = 122 k_B = 1.684E-21 J |
| m* = 1 | mass | m = 39.948 u = 6.63E-26 kg |
| k_B* = 1 | temperature | T = ε/k_B = 122 K |

## Conversion to SI

`X*` is the reduced value.

| Quantity | Formula |
|---|---|
| length `L` [m] | `L* σ` |
| energy `E` [J] | `E* ε` |
| temperature `T` [K] | `T* ε/k_B` |
| mass `M` [kg] | `M* m` |
| time `t` [s] | `t* τ`, with `τ = σ √(m/ε) = 2.137E-12 s` |
| velocity `v` [m/s] | `v* σ/τ` |
| acceleration `a` [m/s²] | `a* σ/τ²` (gravity: `a* = g τ²/σ`) |

## Temperature (2D)

With `k_B* = 1`: `T* = 1/N Σᵢ E_kin,i*`, where `E_kin* = ½ m* v*²` (2 degrees of freedom per ball).

## Equation of motion

There are no free constants in reduced units:

`m* d²x*/dt*² = F*`, with `F* = −dU*/dx*`.

Derivation: `F = −dU/dx = (ε/σ) F*` and `m a = m (σ/τ²) m* a*`. With `τ² = m σ²/ε` this gives `F* = m* a*`.

## Other species in argon units

For another species, convert its single-site LJ parameters with `σ* = σ/3.405 Å`, `ε* = (ε/k_B)/122 K` and `m* = m/39.948 u`. The values below are typical literature numbers, rounded. Check them against a cited source before relying on them.

The built-in species are listed above; a species library and the N₂ unit system are planned (see [refactoring-plan.md](plan/refactoring-plan.md), items I5 and I6).

| Species | σ [Å] | ε/k_B [K] | m [u] | σ* | ε* | m* | τ [ps] |
|---|---|---|---|---|---|---|---|
| Ar | 3.405 | 122 | 39.948 | 1.000 | 1.000 | 1.000 | 2.14 |
| N₂ | 3.70 | 95 | 28.014 | 1.087 | 0.779 | 0.701 | 2.20 |
| H₂ | 2.915 | 38.0 | 2.016 | 0.856 | 0.311 | 0.050 | 0.74 |
| H₂O (a) | 3.166 | 78.2 | 18.015 | 0.930 | 0.641 | 0.451 | 1.67 |
| Au (b) | 2.629 | 2662 | 196.967 | 0.772 | 21.82 | 4.931 | 0.78 |

The `τ` column is `σ √(m/ε)` of that species. In argon units the same timescale ratio is `σ* √(m*/ε*)` (N₂ 1.03, H₂ 0.35, H₂O 0.78, Au 0.37), so light or stiff species need a smaller `dt` in argon units.

- (a) O-site of SPC/E water with the whole-molecule mass. LJ alone is not water: it needs charges (hydrogen bonds) and a rigid-molecule geometry.
- (b) Heinz et al. 12-6 fit (ε = 5.29 kcal/mol, `r_m` = 2.951 Å, `σ = r_m / 2^(1/6)`). Metals are many-body (EAM-like); LJ only gives a rough cohesive/lattice model.
- H₂: quantum effects matter below about 100 K; classical LJ is approximate.
