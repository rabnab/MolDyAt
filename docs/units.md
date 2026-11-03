# Reduced Units and Scales

The simulation runs in reduced Lennard-Jones units with argon as the reference substance (#405): the built-in species `Ar` has `σ* = ε* = m* = 1`. N₂ was the reference from #236 to #405; `N2` stays the default species for new balls. `UnitSystem` (`crates/thermo_physics/src/application/units.rs`, `UnitSystem::AR`, also `UnitSystem::REDUCED`) holds the Ar parameters and converts both ways: reduced results into SI or common units for the real-units display (key `D`), and literature parameters of other species into reduced values. The parameters are fitted to 3D gases, so in this 2D simulation the SI values are qualitative.

## Pair potential

`U(r) = 4 ε_ij ((σ_ij/r)^12 − (σ_ij/r)^6)`, with `r` the centre distance. Each ball has a species with its own `σ`, `ε` and mass; a pair uses Lorentz–Berthelot mixing, `σ_ij = (σ_i + σ_j)/2` and `ε_ij = √(ε_i ε_j)` (`PairTable` in `pair_potential.rs`, #234).

- σ_ij is the zero crossing of `U`; the drawing radius of a ball (`display_radius`, default `σ/2`) has no effect on the physics.
- The minimum (equilibrium distance) is at `r_m = 2^(1/6) σ_ij ≈ 1.1225 σ_ij`.
- The code (`crates/thermo_physics/src/objects2d/ball_impl/pair_potential.rs`) separates the law (`LennardJones`, a `PairLaw`) from the force cap below `r_cap` and the cutoff at `4 σ_ij`, where `U` is shifted to zero (`CappedPotential`), one per species pair.
- Walls (#280, `wall_law.rs`): by default a soft wall potential, the Lennard-Jones law in the distance `z` of the centre from the wall surface, with the wall as a pseudo-species (`σ_w = 0`, `ε_w = 1` by default, `[walls]` in the config): `σ_iw = (σ_i + σ_w)/2`, `ε_iw = √(ε_i ε_w)`, capped like a pair, cut and shifted at its minimum `2^(1/6) σ_iw` (WCA, purely repulsive) unless `attractive`. With `σ_w = 0` its zero crossing is the hard contact distance `σ_i / 2`. A hard contact stays as a backstop at `0.3 σ_i`, where the default wall energy is ≈ 24 ε_N2 for `N2`. `[walls] model = "hard"` (and every save from before format v4) reflects at `σ_i / 2` instead, without a potential.
- A config can override `σ_ij` and/or `ε_ij` of named species pairs (`[[potential.pair_override]]` with `a`, `b`, `sigma`, `epsilon`; see the README, #235). The override replaces the mixed value of exactly that pair, in both orders; its cutoff (`cutoff_factor · σ_ij`), `r_cap` and `U(cutoff)` follow from the overridden law, and the cell grid widens if the pair reaches further than the others present. Placement and overlap checks measure from each pair's contact distance `σ_ij` (`BallPhysics::contact_distance`, which follows the override), and an overridden like pair `σ_ii` also moves the species' wall contact to `σ_ii / 2`. Only drawing and the headless `contact_radius` field keep the species' own `σ_i / 2`.

### Built-in species (species library, #237)

Without a `[[species]]` list in the config, every world uses this table (`SpeciesTable::builtin()`, in this order; the order is what the UI steps through):

| Species | σ [Å] | ε/k_B [K] | m [u] | σ* | ε* | m* | Note |
|---|---|---|---|---|---|---|---|
| N2 | 3.698 | 95.05 | 28.0134 | 1.0860 | 0.7934 | 0.7012 | (a); default for new balls |
| XS, S, M, L, XL | – | 95.05 | – | `(2·R·2^(-1/6) + 1)·1.0860` (1.376, 1.667, 2.247, 3.408, 5.730) | 0.7934 | 0.701 … 4.317 | the former size classes of radius `R` = 0.15 … 2.4 in N₂ units, kept for migrated saves |
| Ar | 3.405 | 119.8 | 39.948 | 1 | 1 | 1 | (a); the unit species |
| Ne | 2.749 | 35.60 | 20.180 | 0.8073 | 0.2972 | 0.5052 | (a) |
| CH4 | 3.73 | 148.0 | 16.043 | 1.0954 | 1.2354 | 0.4016 | (b), united atom |
| O2 | 3.58 | 117.5 | 31.999 | 1.0514 | 0.9808 | 0.8010 | (a) |
| NH3 | 2.900 | 558.3 | 17.031 | 0.8517 | 4.6603 | 0.4263 | (c) |
| CH4_HCB | 3.817 | 148.2 | 16.043 | 1.1210 | 1.2371 | 0.4016 | (a); `CH4` before #409, for saves that used it |

- (b) TraPPE united atom: Martin and Siepmann, *J. Phys. Chem. B* 102, 2569 (1998) (#409). Saves from before format v7 on the built-in table name their `CH4` `CH4_HCB`, so they keep the old values.
- (c) Svehla, *Estimated viscosities and thermal conductivities of gases at high temperatures*, NASA Technical Report R-132 (1962), from viscosity (#409). NH₃ is polar and forms hydrogen bonds; single-site Lennard-Jones ignores both, so it is only a rough, sticky sphere. With `ε* ≈ 4.7` and `m* ≈ 0.43` its timescale `σ* √(m*/ε*)` is 0.26 of Ar's: use about a quarter of the time step for NH₃-rich runs.
- (a) `σ` and `ε/k_B`: Hirschfelder, Curtiss and Bird, *Molecular Theory of Gases and Liquids* (Wiley, 1954), table I-A, from second virial coefficients, the same source as N₂. Masses: IUPAC standard atomic weights. The library is `LIBRARY` in `crates/thermo_physics/src/objects2d/ball_impl/species.rs`, converted with the Ar constants there (`UnitSystem::AR` uses the same ones). `builtin_table_is_pinned` pins the whole table: saves name species, so changing an entry is a format change (see the `save-format` skill); add new ones at the end.

A config file's `[[species]]` list (README, "Simulation config") replaces the whole table: `name`, `sigma_angstrom`, `epsilon_kelvin`, `mass_u`, `colour` (RGB or RGBA), optional `display_radius_angstrom` and `source`. The values are converted with `σ* = σ / 3.405 Å`, `ε* = (ε/k_B) / 119.8 K`, `m* = m / 39.948 u` and must lie in `σ* ∈ 0.5..8`, `ε* ∈ 0.1..10`, `m* ∈ 0.05..100`. The first entry is the default for new balls and source spheres. The units stay Ar's whether or not the list contains `Ar`. A save carries the list in its `config`, so it loads with the species it was saved with.

Before #234 the potential acted on the edge distance (centre distance minus both radii) with `σ = 1` for every ball. With `σ_i = 2·R_i·2^(-1/6) + 1` and arithmetic mixing, every pair of former size classes keeps its old equilibrium centre distance `R_i + R_j + 2^(1/6)`, so saves migrate (format v2). The shape of the potential still changes: the larger size classes are softer and reach further (cutoff `4 σ_ij`), and they stay `σ_i / 2` from walls instead of `R_i` (e.g. `M`: 1.03 instead of 0.6, in N₂ units).

### From N₂ to Ar units (#405)

Saves of format v5 and older are in N₂ units; loading rescales them (format v6) with `L = σ_N2/σ_Ar = 1.0861`, `E = ε_N2/ε_Ar = 0.7934`, `M = m_N2/m_Ar = 0.7012` and `T = τ_N2/τ_Ar = 1.0210`: lengths × L, speeds × L/T, accelerations (gravity) × L/T², energies and temperatures × E, times (thermostat `tau`) × T, spring constants × E/L², the force cap `max_force` × E/L and `core_stiffness` × E/L². The defaults of the force cap are the old ones converted the same way (100 → 73.05, 200 → 134.53), and the Verlet skin stays a share of the `N2` cutoff, so default runs keep their physics. Numerical settings keep their numbers: the time step (0.005, now about 2 % shorter in physical time), the cutoff and skin factors, and the world size of a new world (112 × 62 in reduced units, 8 % smaller physically).

## Reduced units

All internal quantities are reduced (marked `*`). The Ar parameters are single-site Lennard-Jones values from second virial coefficients: Hirschfelder, Curtiss and Bird, *Molecular Theory of Gases and Liquids* (Wiley, 1954), table I-A.

| Reduced | Quantity | SI value (Ar) |
|---|---|---|
| σ* = 1 | length | σ = 3.405 Å = 3.405E-10 m |
| ε* = 1 | energy | ε = 119.8 k_B = 1.654E-21 J |
| m* = 1 | mass | m = 39.948 u = 6.634E-26 kg |
| k_B* = 1 | temperature | T = ε/k_B = 119.8 K |

## Conversion to SI

`X*` is the reduced value. The real-units display shows time in ps, the time step in fs, temperature in K, energies in J and speeds in m/s (`thermo_ui` `energy_plot/status_text.rs`, from the snapshot's `StatusNumbers`).

| Quantity | Formula |
|---|---|
| length `L` [m] | `L* σ` |
| energy `E` [J] | `E* ε` |
| temperature `T` [K] | `T* ε/k_B` |
| mass `M` [kg] | `M* m` |
| time `t` [s] | `t* τ`, with `τ = σ √(m/ε) = 2.156E-12 s` |
| velocity `v` [m/s] | `v* σ/τ` (`σ/τ` = 157.9 m/s) |
| acceleration `a` [m/s²] | `a* σ/τ²` (gravity: `a* = g τ²/σ`) |

Gravity (key `G`) is 1 g = 9.81 m/s², `a* = 1.3396E-13` in Ar units, or 5 g. In N₂ units (#236 to #405) 1 g was `a* = 1.2859E-13`; loading such a save converts it to 1.3396E-13. Before #236 the units were also argon's, with other constants, and 1 g was `a* = 1.3157E-13`; `G` still treats such a value as 1 g (`MainContent::toggle_gravity`).

## Temperature (2D)

With `k_B* = 1`: `T* = 1/N Σᵢ E_kin,i*`, where `E_kin* = ½ m* v*²` (2 degrees of freedom per ball).

## Equation of motion

There are no free constants in reduced units:

`m* d²x*/dt*² = F*`, with `F* = −dU*/dx*`.

Derivation: `F = −dU/dx = (ε/σ) F*` and `m a = m (σ/τ²) m* a*`. With `τ² = m σ²/ε` this gives `F* = m* a*`.

## Other species in Ar units

For another species, convert its single-site LJ parameters with `σ* = σ/3.405 Å`, `ε* = (ε/k_B)/119.8 K` and `m* = m/39.948 u` (`UnitSystem::reduced_lennard_jones`). The values below other than N₂ and Ar are typical literature numbers, rounded, for scale; the built-in species and their source are listed above. Check other values against a cited source before relying on them.

| Species | σ [Å] | ε/k_B [K] | m [u] | σ* | ε* | m* | τ [ps] |
|---|---|---|---|---|---|---|---|
| Ar | 3.405 | 119.8 | 39.948 | 1.000 | 1.000 | 1.000 | 2.16 |
| N₂ | 3.698 | 95.05 | 28.0134 | 1.086 | 0.793 | 0.701 | 2.20 |
| H₂ | 2.915 | 38.0 | 2.016 | 0.856 | 0.317 | 0.050 | 0.74 |
| H₂O (a) | 3.166 | 78.2 | 18.015 | 0.930 | 0.653 | 0.451 | 1.67 |
| Au (b) | 2.629 | 2662 | 196.967 | 0.772 | 22.22 | 4.931 | 0.78 |

The `τ` column is `σ √(m/ε)` of that species. In Ar units the same timescale ratio is `σ* √(m*/ε*)` (N₂ 1.02, H₂ 0.34, H₂O 0.77, Au 0.36), so light or stiff species need a smaller `dt`.

- (a) O-site of SPC/E water with the whole-molecule mass. LJ alone is not water: it needs charges (hydrogen bonds) and a rigid-molecule geometry.
- (b) Heinz et al. 12-6 fit (ε = 5.29 kcal/mol, `r_m` = 2.951 Å, `σ = r_m / 2^(1/6)`). Metals are many-body (EAM-like); LJ only gives a rough cohesive/lattice model.
- H₂: quantum effects matter below about 100 K; classical LJ is approximate.
