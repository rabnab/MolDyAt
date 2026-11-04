# Reduced Units and Scales

The simulation runs in reduced Lennard-Jones units with argon as the reference substance (#405): the built-in species `Ar` has `σ* = ε* = m* = 1`. N₂ was the reference from #236 to #405; `N2` stays the default species for new balls. `UnitSystem` (`crates/thermo_physics/src/application/units.rs`, `UnitSystem::AR`, also `UnitSystem::REDUCED`) holds the Ar parameters and converts both ways: reduced results into SI or common units for the real-units display (key `D`), and literature parameters of other species into reduced values. The parameters are fitted to 3D gases, so in this 2D simulation the SI values are qualitative.

## Pair potential

`U(r) = 4 ε_ij ((σ_ij/r)^12 − (σ_ij/r)^6)`, with `r` the centre distance. Each ball has a species with its own `σ`, `ε` and mass; a pair uses Lorentz–Berthelot mixing, `σ_ij = (σ_i + σ_j)/2` and `ε_ij = √(ε_i ε_j)` (`PairTable` in `pair_potential.rs`, #234).

- σ_ij is the zero crossing of `U`; the drawing radius of a ball (`display_radius`, default `σ/2`) has no effect on the physics.
- The minimum (equilibrium distance) is at `r_m = 2^(1/6) σ_ij ≈ 1.1225 σ_ij`.
- The code (`crates/thermo_physics/src/objects2d/ball_impl/pair_potential.rs`) separates the law (`LennardJones`, a `PairLaw`) from the force cap below `r_cap` and the cutoff at `4 σ_ij`, where `U` is shifted to zero (`CappedPotential`), one per species pair.
- Walls (#280, `wall_law.rs`): by default a soft wall potential, the Lennard-Jones law in the distance `z` of the centre from the wall surface, with the wall as a pseudo-species (`σ_w = 0`, `ε_w = 1` by default, `[walls]` in the config): `σ_iw = (σ_i + σ_w)/2`, `ε_iw = √(ε_i ε_w)`, capped like a pair, cut and shifted at its minimum `2^(1/6) σ_iw` (WCA, purely repulsive) unless `attractive`. With `σ_w = 0` its zero crossing is the hard contact distance `σ_i / 2`. A hard contact stays as a backstop at `0.3 σ_i`, where the default wall energy is ≈ 24 ε_N2 for `N2`. `[walls] model = "hard"` (and every save from before format v4) reflects at `σ_i / 2` instead, without a potential.
- A config can override `σ_ij` and/or `ε_ij` of named species pairs (`[[potential.pair_override]]` with `a`, `b`, `sigma`, `epsilon`; see the README, #235). The override replaces the mixed value of exactly that pair, in both orders; its cutoff (`cutoff_factor · σ_ij`), `r_cap` and `U(cutoff)` follow from the overridden law, and the cell grid widens if the pair reaches further than the others present. Placement and overlap checks measure from each pair's contact distance `σ_ij` (`BallPhysics::contact_distance`, which follows the override), and an overridden like pair `σ_ii` also moves the species' wall contact to `σ_ii / 2`; its `σ_ii` and `ε_ii` also enter the soft wall's mixed `σ_iw` and `ε_iw`. Only drawing and the headless `contact_radius` field keep the species' own `σ_i / 2`.

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
- (c) Svehla, *Estimated viscosities and thermal conductivities of gases at high temperatures*, NASA Technical Report R-132 (1962), from viscosity (#409). NH₃ is polar and forms hydrogen bonds; single-site Lennard-Jones ignores both, so it is only a rough, sticky sphere. With `ε* ≈ 4.7` and `m* ≈ 0.43` its timescale `σ* √(m*/ε*)` is 0.26 of Ar's: use about a quarter of the time step for NH₃-rich runs. The NH₃–NH₃ well is the stiffest pair of the library (curvature `72·2^(-1/3)·ε/σ²` ≈ 367, ω ≈ 41.5 for the pair): ω·dt ≈ 0.2 at the default dt 0.005, but Verlet goes unstable above dt ≈ 0.048 (ω·dt = 2), well below `MAX_TIME_STEP` 0.08. At T* = 1 bound pairs sit deep in the well (`ε_NH3 ≈ 4.7 k_B T`).
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

With `k_B* = 1`: `T* = 1/N Σᵢ E_kin,i*`, where `E_kin* = ½ m* v*²` (2 degrees of freedom per ball). The code measures it in the centre-of-mass frame: `T* = (Σ m |v − v_com|² + Σ I ω²) / (2 (N − 1) + N_rot)` (`LocalKineticStats`), where a ball of a species with a moment of inertia (#348) adds one rotational degree of freedom and its `½ I ω²` (equipartition). The thermostats use the same count and scale `ω` with the peculiar velocity.

A `[[species]]` entry can give `moment_of_inertia_u_angstrom2` (`I* = I / (m_Ar σ_Ar²)`, `m_Ar σ_Ar²` = 463.2 u·Å², `I* ∈ 0.001..100`); such a species has an orientation `θ` and an angular velocity `ω`, integrated by the rotational velocity-Verlet step from the torque of the force pass. The built-in species have none.

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

## MB water (#349)

The built-in species `H2O` is Mercedes-Benz water (Silverstein, Haymet and Dill, *J. Am. Chem. Soc.* 120, 3166 (1998); `ball_impl/water.rs`): a Lennard-Jones disk with three hydrogen-bond arms at 120°, turned by the ball's orientation (#348). Two water balls add `U = −ε_HB G(d − r_HB) Σ_k G(a_k·u − 1) Σ_l G(b_l·u + 1)`, `G(x) = exp(−x²/(2·0.085²))` (the distance width is `0.085 r_HB`); the force and the torque are its exact derivatives. Other species meet water through its Lennard-Jones part only (Lorentz–Berthelot), so N₂ or Ar in water are hydrophobic solutes; walls also see only that part.

| Parameter | MB units | Physical (owner decision) | Ar units |
|---|---|---|---|
| `r_HB` | 1 | 2.8 Å (O–O distance of a hydrogen bond) | 0.8223 |
| `ε_HB / k_B` | 1 | 1385 K, so the density maximum (`T ≈ 0.2 ε_HB/k_B`) lies near 277 K | 11.56 |
| `σ_LJ` | 0.7 | 1.96 Å | 0.5756 |
| `ε_LJ / k_B` | 0.1 | 138.5 K | 1.156 |
| mass | – | 18.015 u | 0.4510 |
| moment of inertia | – | 1.15 u·Å² (H₂O, in-plane axis) | 0.00248 |

With the real moment of inertia, water librates fast: runs with water need `dt ≤ 0.001` (`WATER_MAX_TIME_STEP`, about 2 fs, as in all-atom water MD with rigid molecules). 3400 H₂O in a world of scale 3 keep their total energy within 0.5 % over 2000 steps at 0.001 and heat up without bound (to > 30 000 K) at the default 0.005. A temperature in MB units is `T_MB = T / 11.56` in Ar units (`T_MB = 0.2` ≈ 277 K).

The 2D model reproduces water's density anomaly qualitatively: at fixed density `0.82 / r_HB²` the pressure peaks near `T_MB ≈ 0.135` and falls up to about 0.17 (`(∂P/∂T)_V < 0`, i.e. a negative expansion coefficient; wall pressure in Ar units: 0.11 → 2.08, 0.13 → 2.32, 0.14 → 2.33, 0.15 → 2.12, 0.16 → 2.22, 0.17 → 2.16, 0.19 → 2.36, single runs of 144 molecules, ±0.1). At 0.76 and 0.88 the pressure rises with temperature. The water config of `[[species]]` lists cannot define MB arms yet; only the built-in `H2O` has them.

## Rigid linear molecules (#406)

A species can be a rigid linear molecule (`ball_impl/molecule.rs`): Lennard-Jones sites at fixed offsets along the axis of the ball, which is the molecule's centre of mass and turns with its orientation (#348). Mass and moment of inertia are the sums over the sites. Two balls of which at least one is a molecule interact site by site, every site with every site (an atomic species counts as one site at its centre with its own `σ`, `ε`; pair overrides do not apply to such pairs), with Lorentz–Berthelot mixing per site pair, capped and cut off at `4 σ_ab` like a pair; the site forces give force and torque. The molecule's own `σ` is the envelope of its sites (placement, the cell grid) and its `ε` is 0, so the pair table gives it no centre interaction. Soft walls act on every site (force and torque). The hard backstop on the box walls depends on the orientation: the centre stays `half length · |cos θ|` (left/right) or `|sin θ|` (bottom/top) plus a site's backstop from the wall (#408), so no site crosses it and a molecule lying along a wall is not stopped before its sites feel the soft wall. Line walls use the full half length, which is conservative: a long molecule lying along a line wall can meet the backstop early (elastic, no energy error).

| Species | Sites | Bond | Mass | `I` | Source |
|---|---|---|---|---|---|
| N2_2S | 2 × N: σ = 3.31 Å, ε/k_B = 36.0 K | 1.10 Å | 28.014 u | 8.47 u·Å² (0.0183 in Ar units) | TraPPE, Potoff and Siepmann, *AIChE J.* 47, 1676 (2001), without the centre charges |
| CO2_3S | O–C–O: O σ = 3.05 Å, ε/k_B = 79.0 K; C σ = 2.80 Å, ε/k_B = 27.0 K | C–O 1.16 Å | 44.009 u | 43.07 u·Å² (0.0930) | TraPPE, Potoff and Siepmann (2001), without the partial charges (the quadrupole needs long-range electrostatics) |
| H2_2S | 2 × H: σ = 2.72 Å, ε/k_B = 10.0 K | 0.74 Å | 2.016 u | 0.276 u·Å² (0.000596) | Yang and Zhong, *J. Phys. Chem. B* 109, 11862 (2005); among the two best of eight LJ models in Barraco et al., *J. Phys. Chem. A* 127 (2023) |

H₂ is classical here: zero-point motion and the freezing of its rotations below about 100 K are not modelled. It is light and fast (`τ` 0.34 of Ar's, rotation ~20 times faster than N₂'s): runs with `H2_2S` need `dt ≤ 0.001` (`H2_MAX_TIME_STEP`; energy error 3.7e-4 of the kinetic energy at 0.001, 2.5e-3 at 0.0025, 3.5e-2 at the default 0.005). While a species with a `max_time_step` (`H2O` 0.001, `H2_2S` 0.001, `CO2_3S` 0.0025) is in the world, the steps run with at most the smallest of them (`MainContent::active_time_step`): the set time step is kept and applies again once those species are gone, the `+` key does not go above the limit, and the status lines (`B`) show `dt limited by <species>`. CO₂'s heavier, strongly bound sites need 0.0025: its energy error is 9.6e-3 of the kinetic energy at 0.005, 1.5e-3 at 0.0025 and 2.3e-4 at 0.00125 (a cool CO₂/Ar gas); equipartition `⟨E_rot⟩ / (⟨E_trans⟩ / 2)` = 1.00. In a dilute H₂/Ar gas the partial pressures add up (Dalton: `P_mix / (P_H2 + P_Ar)` = 1.06, each close to the ideal `N k_B T / A`).

`N2_2S` sits next to the single-site `N2`, which stays the default species (owner decision). A dumbbell has 2 translational and 1 rotational degree of freedom: in equilibrium `⟨E_rot⟩ = ⟨E_trans⟩ / 2` per molecule (measured 0.98). Its energy error at the default `dt = 0.005` is about 1.6e-3 of the kinetic energy over 20 time units (dt²: 0.4e-3 at dt/2).

