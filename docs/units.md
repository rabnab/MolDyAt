# Reduced Units and Scales

The simulation runs in reduced Lennard-Jones units with N₂ as the reference substance (#236): the built-in species `N2` has `σ* = ε* = m* = 1`. `UnitSystem` (`crates/thermo_physics/src/application/units.rs`, `UnitSystem::N2`) holds the N₂ parameters and converts both ways: reduced results into SI or common units for the real-units display (key `D`), and literature parameters of other species into reduced values. The parameters are fitted to 3D gases, so in this 2D simulation the SI values are qualitative.

## Pair potential

`U(r) = 4 ε_ij ((σ_ij/r)^12 − (σ_ij/r)^6)`, with `r` the centre distance. Each ball has a species with its own `σ`, `ε` and mass; a pair uses Lorentz–Berthelot mixing, `σ_ij = (σ_i + σ_j)/2` and `ε_ij = √(ε_i ε_j)` (`PairTable` in `pair_potential.rs`, #234).

- σ_ij is the zero crossing of `U`; the drawing radius of a ball (`display_radius`, default `σ/2`) has no effect on the physics.
- The minimum (equilibrium distance) is at `r_m = 2^(1/6) σ_ij ≈ 1.1225 σ_ij`.
- The code (`crates/thermo_physics/src/objects2d/ball_impl/pair_potential.rs`) separates the law (`LennardJones`, a `PairLaw`) from the force cap below `r_cap` and the cutoff at `4 σ_ij`, where `U` is shifted to zero (`CappedPotential`), one per species pair.
- Walls reflect a ball at centre distance `σ_i / 2` (`WallLaw`, hard contact).
- A config can override `σ_ij` and/or `ε_ij` of named species pairs (`[[potential.pair_override]]` with `a`, `b`, `sigma`, `epsilon`; see the README, #235). The override replaces the mixed value of exactly that pair, in both orders; its cutoff (`cutoff_factor · σ_ij`), `r_cap` and `U(cutoff)` follow from the overridden law, and the cell grid widens if the pair reaches further than the others present. Placement and overlap checks measure from each pair's contact distance `σ_ij` (`BallPhysics::contact_distance`, which follows the override), and an overridden like pair `σ_ii` also moves the species' wall contact to `σ_ii / 2`. Only drawing and the headless `contact_radius` field keep the species' own `σ_i / 2`.

### Built-in species (species library, #237)

Without a `[[species]]` list in the config, every world uses this table (`SpeciesTable::builtin()`, in this order; the order is what the UI steps through):

| Species | σ [Å] | ε/k_B [K] | m [u] | σ* | ε* | m* | Note |
|---|---|---|---|---|---|---|---|
| N2 | 3.698 | 95.05 | 28.0134 | 1 | 1 | 1 | default for new balls; the unit species |
| XS, S, M, L, XL | – | – | – | `2·R·2^(-1/6) + 1` (1.27, 1.53, 2.07, 3.14, 5.28) | 1 | 1.0 … 6.16 | the former size classes of radius `R` = 0.15 … 2.4, kept for migrated saves |
| Ar | 3.405 | 119.8 | 39.948 | 0.9208 | 1.2604 | 1.4260 | (a) |
| Ne | 2.749 | 35.60 | 20.180 | 0.7434 | 0.3745 | 0.7204 | (a) |
| CH4 | 3.817 | 148.2 | 16.043 | 1.0322 | 1.5592 | 0.5727 | (a) |
| O2 | 3.58 | 117.5 | 31.999 | 0.9681 | 1.2362 | 1.1423 | (a) |

- (a) `σ` and `ε/k_B`: Hirschfelder, Curtiss and Bird, *Molecular Theory of Gases and Liquids* (Wiley, 1954), table I-A, from second virial coefficients, the same source as N₂. Masses: IUPAC standard atomic weights. The library is `LIBRARY` in `crates/thermo_physics/src/objects2d/ball_impl/species.rs`, converted with the N₂ constants there (`UnitSystem::N2` uses the same ones). `builtin_table_is_pinned` pins the whole table: saves name species, so changing an entry is a format change (see the `save-format` skill); add new ones at the end.

A config file's `[[species]]` list (README, "Simulation config") replaces the whole table: `name`, `sigma_angstrom`, `epsilon_kelvin`, `mass_u`, `colour` (RGB or RGBA), optional `display_radius_angstrom` and `source`. The values are converted with `σ* = σ / 3.698 Å`, `ε* = (ε/k_B) / 95.05 K`, `m* = m / 28.0134 u` and must lie in `σ* ∈ 0.5..8`, `ε* ∈ 0.1..10`, `m* ∈ 0.05..100`. The first entry is the default for new balls and source spheres. The units stay N₂'s whether or not the list contains `N2`. A save carries the list in its `config`, so it loads with the species it was saved with.

Before #234 the potential acted on the edge distance (centre distance minus both radii) with `σ = 1` for every ball. With `σ_i = 2·R_i·2^(-1/6) + 1` and arithmetic mixing, every pair of former size classes keeps its old equilibrium centre distance `R_i + R_j + 2^(1/6)`, so saves migrate (format v2). The shape of the potential still changes: the larger size classes are softer and reach further (cutoff `4 σ_ij`), and they stay `σ_i / 2` from walls instead of `R_i` (e.g. `M`: 1.03 instead of 0.6).

## Reduced units

All internal quantities are reduced (marked `*`). The N₂ parameters are single-site Lennard-Jones values from second virial coefficients: Hirschfelder, Curtiss and Bird, *Molecular Theory of Gases and Liquids* (Wiley, 1954), table I-A. Published single-site fits for N₂ range over about 70–100 K for `ε/k_B`, depending on the fitted property.

| Reduced | Quantity | SI value (N₂) |
|---|---|---|
| σ* = 1 | length | σ = 3.698 Å = 3.698E-10 m |
| ε* = 1 | energy | ε = 95.05 k_B = 1.312E-21 J |
| m* = 1 | mass | m = 28.0134 u = 4.652E-26 kg |
| k_B* = 1 | temperature | T = ε/k_B = 95.05 K |

## Conversion to SI

`X*` is the reduced value. The real-units display shows time in ps, the time step in fs, temperature in K, energies in J and speeds in m/s (`SimStats::get_status_text_real`).

| Quantity | Formula |
|---|---|
| length `L` [m] | `L* σ` |
| energy `E` [J] | `E* ε` |
| temperature `T` [K] | `T* ε/k_B` |
| mass `M` [kg] | `M* m` |
| time `t` [s] | `t* τ`, with `τ = σ √(m/ε) = 2.202E-12 s` |
| velocity `v` [m/s] | `v* σ/τ` (`σ/τ` = 168.0 m/s) |
| acceleration `a` [m/s²] | `a* σ/τ²` (gravity: `a* = g τ²/σ`) |

Gravity (key `G`) is 1 g = 9.81 m/s², `a* = 1.2859E-13` in N₂ units, or 5 g. Before #236 the units were those of argon and 1 g was `a* = 1.3157E-13`. Saves store the reduced value, so such a save runs with its old gravity, and `G` still treats it as 1 g (`MainContent::toggle_gravity`).

## Temperature (2D)

With `k_B* = 1`: `T* = 1/N Σᵢ E_kin,i*`, where `E_kin* = ½ m* v*²` (2 degrees of freedom per ball).

## Equation of motion

There are no free constants in reduced units:

`m* d²x*/dt*² = F*`, with `F* = −dU*/dx*`.

Derivation: `F = −dU/dx = (ε/σ) F*` and `m a = m (σ/τ²) m* a*`. With `τ² = m σ²/ε` this gives `F* = m* a*`.

## Other species in N₂ units

For another species, convert its single-site LJ parameters with `σ* = σ/3.698 Å`, `ε* = (ε/k_B)/95.05 K` and `m* = m/28.0134 u` (`UnitSystem::reduced_lennard_jones`). The values below other than N₂ are typical literature numbers, rounded, for scale; the built-in species and their source are listed above (Ar there uses the second-virial ε/k_B = 119.8 K). Check other values against a cited source before relying on them.

| Species | σ [Å] | ε/k_B [K] | m [u] | σ* | ε* | m* | τ [ps] |
|---|---|---|---|---|---|---|---|
| N₂ | 3.698 | 95.05 | 28.0134 | 1.000 | 1.000 | 1.000 | 2.20 |
| Ar | 3.405 | 122 | 39.948 | 0.921 | 1.284 | 1.426 | 2.14 |
| H₂ | 2.915 | 38.0 | 2.016 | 0.788 | 0.400 | 0.072 | 0.74 |
| H₂O (a) | 3.166 | 78.2 | 18.015 | 0.856 | 0.823 | 0.643 | 1.67 |
| Au (b) | 2.629 | 2662 | 196.967 | 0.711 | 28.01 | 7.031 | 0.78 |

The `τ` column is `σ √(m/ε)` of that species. In N₂ units the same timescale ratio is `σ* √(m*/ε*)` (Ar 0.97, H₂ 0.33, H₂O 0.76, Au 0.36), so light or stiff species need a smaller `dt`.

- (a) O-site of SPC/E water with the whole-molecule mass. LJ alone is not water: it needs charges (hydrogen bonds) and a rigid-molecule geometry.
- (b) Heinz et al. 12-6 fit (ε = 5.29 kcal/mol, `r_m` = 2.951 Å, `σ = r_m / 2^(1/6)`). Metals are many-body (EAM-like); LJ only gives a rough cohesive/lattice model.
- H₂: quantum effects matter below about 100 K; classical LJ is approximate.
