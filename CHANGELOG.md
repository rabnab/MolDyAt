# Changelog

User-visible changes to MolDyAt, newest first. Binaries and full release notes are on the
[GitHub releases page](https://github.com/rabnab/MolDyAt/releases).

## Unreleased

- Normalise g(r) by the density around each ball so it levels off at 1 where the density varies
  (gravity, chambers): `Alt + M` in the distributions view, `--rdf-local` with `--distributions`
  (frames, `run_start` and exports then carry `normalisation: "local"`); the default is unchanged. (#384)
- Saves hold their whole species table (with water and molecule models) and state their units
  (save format 9), so later changes to the built-in species or units do not change how they load;
  `[[species]]` can define water (`water`) and rigid molecules (`sites`). (#426)
- New molecules are placed up to about 0.02 σ closer to soft walls: each site at its own wall
  minimum instead of the molecule's outer disk. (#443)
- Every species, also from a config, gets a time step limit derived from its potential (`dt limited
  by …`): water about 0.0007, H₂ 0.0016, NH₃ 0.0034; CO₂ is no longer capped at 0.0025, and a set
  step above about 0.008 is capped for every species. (#427)
- Fast balls no longer pass through a thin wall in a single step, and molecules meet every wall
  with each site at its own size, also with hard walls (before: line walls as a disk). (#424)
- Count the safety nets that bend the physics (speed-limit braking, force cap, wall backstop,
  overlap fix) in the status lines (`B`), headless frames and metrics. (#422)
- Keep the simulation time, energy totals, wall momentum and thermostats in f64, so long runs keep
  their time axis and an accurate energy drift. (#421)
- Add CO₂ as a rigid three-site molecule (`CO2_3S`); molecules bounce off the box walls with a
  spin. (#408)
- Add H₂ as a rigid two-site molecule (`H2_2S`). (#407)
- Add N₂ as a rigid two-site molecule (`N2_2S`). (#406)
- Add 2D water (`H2O`, Mercedes-Benz model) with hydrogen bonds. While water is present the time
  step is capped (`dt limited by …`), and `F1` also stops rotation. (#349)
- Species can rotate (`moment_of_inertia_u_angstrom2`): temperature, thermostats, new balls and saves
  include the rotation. (#348)
- `CH4` uses the TraPPE-UA parameters (the old one stays as `CH4_HCB`), and ammonia (`NH3`) is a new
  species. (#409)
- **Breaking:** reduced units are based on argon instead of N₂ (save format 6); older saves and
  configs are converted on load, and a new world is about 8 % smaller physically. (#405)
- Add external fields to the config (`[[fields]]`, e.g. a harmonic trap); gravity is one of them. (#347)
- Walls are a soft repulsive potential with a hard backstop by default (`[walls]` in the config);
  saves from before keep hard walls. A wall drawn over balls pushes them out gently. (#280, #424)

## v0.3.0 - 2026-11-02

- **Breaking:** balls interact by Lennard-Jones on the centre distance with parameters per species
  pair, and the default time step is halved to 0.005; trajectories differ from v0.2.0. (#234, #301)
- **Breaking:** save format 3 keeps all operators in one list. v0.2.0 saves are migrated on load;
  v0.3.0 saves do not load in v0.2.0. (#272, #274, #292)
- **Breaking:** headless output is now `headless.v2`. (#234)
- Scale the field overlay's colours to the world's averages; `Alt + T` locks the scale. (#402)
- Click a g(r) legend entry to hide or show that pair. (#400)
- Show a colour bar instead of numbers on the field overlay. (#398)
- Keep ball positions in f64, so energy is conserved as well far from the origin as near it. (#394)
- Faster steps in large worlds: a 16-byte force-kernel record, cold ball data stored apart, a flat
  cell grid, and source spheres that check new balls against the grid. (#390, #391, #392, #393)
- Fix the cursor crosshair growing or shrinking with zoom and world size. (#388)
- Fix dragging valves along their wall. (#383)
- Add the speed distribution with its Maxwell–Boltzmann curve and g(r) per species pair to the
  plot window (`M`) and to exports. (#381)
- Show temperature, pressure, density or flow speed per cell as a heatmap, or flow arrows (`T`),
  on a grid of its own. (#379, #380)
- Fix a crash when loading a world larger than the screen; the window is limited to the monitor
  and the GPU's maximum texture size. (#377)
- Fix the plot leaving its pause when a pan reaches the newest sample. (#376)
- Drive headless runs with command scripts (`--script FILE`). (#340)
- Plot any metric in up to four stacked plots; zoom and pan the time range with the mouse.
  (#367, #370)
- Stream every metrics sample to a CSV or JSONL file while the simulation runs. (#369)
- Export metrics series and per-ball state as CSV or JSONL, from the window and headless runs. (#337)
- `--world-scale` works for windowed runs too, not only headless.
- Change key bindings in `ui.toml`. (#335)
- Bind keys by the character they type, so shortcuts match every keyboard layout. (#327)
- Add gravitational potential energy to the stats, so the total is conserved with gravity on. (#334)
- Split the `moldyat_core` crate into `thermo_physics`, `thermo_runtime` and `thermo_ui`. (#308, #310)
- Apply forces in parallel; results stay identical for any thread count. (#322)
- Warn about unknown fields when loading a save instead of dropping them silently. (#273)
- Pick the species of new balls with `N` or a right click; the sidebar shows its name. (#234, #302)
- Add the species N₂, Ar, Ne, CH₄ and O₂, and `[[species]]` sections in the config. (#299)
- Show values in real units, with N₂ as the reference. (#236)
- Override σ and ε per species pair in the config. (#235)
- Load run settings from a `--config` TOML file; they are stored with every save. (#293)
- Fix the force cap so the core force matches the configured maximum. (#278)

## v0.2.0 - 2026-10-10

First public release; previously developed as ThermoSolver.

- **Breaking:** renamed to MolDyAt: the executable is `moldyat`, version numbers restart at 0.2.0.
  Licensed under MPL-2.0, with third-party notices. (#252, #253)
- Place new balls with a grid-based collision check. (#245)
- Show the build version and git commit in `--version`, the window title and the log; add `--help`.
  (#221)
- Add per-phase step timings, `--valve` and benchmark scenarios to headless runs. (#221)
- Let overlapping balls separate: the core force keeps rising below the cap distance. (#240)
- Draw operators and the mouse while paused. (#241)
- Save and load complete worlds as versioned JSON (`Ctrl+S`, `Ctrl+O`, `--load`, `--save-dir`). (#50)
- Improve window resize handling. (#217)
- Faster neighbour lists: sorted Verlet lists built from cells only, rebuilt in parallel. (#209)
- Better energy conservation: the LJ potential is truncated and shifted at the cutoff, wall contacts
  are resolved along the wall normal; the cell pressure uses the 2D virial factor.
- Add `--dt` and `--line-segment` to headless runs.
- Highlight hovered operators; `Del` deletes them.
- Fix valve collisions by splitting walls into chains around valves.
- Improve the look of balls, walls, valves, thermostats and the heatmap.
- Add headless mode: seeded, reproducible runs without a window, with a console summary and JSONL
  output (`headless.v1`). (#180, #185, #191)
- Fix DX12 shader portability and native backend selection. (#169)
- Cycle the thermo display with `T`: off, grid, temperature, pressure. (#168)
- Thermostats use a coupled 5-chain Nosé–Hoover integrator. (#136, #165)
- Toggle gravity with a key and show its strength.
- Place line segments, valves, thermostats and source spheres from a sidebar. (#131, #133–#136)
- Show a temperature and pressure heatmap overlay. (#122, #123)
- Render with wgpu and WGSL shaders. (#83)
- Zoom and pan the view.
- Find new neighbours without delay through Verlet lists with a skin.
- Run the physics on its own thread; control simulation speed and time step. (#49)
- Show a real-time energy plot in its own window. (#47)
- Fix Velocity Verlet with wall bounces and make force capping energy-conserving. (#38)
- Compute Lennard-Jones forces with SIMD and in parallel with Rayon. (#26, #29)
- Look up neighbours through a cell grid.
- Simulate a Lennard-Jones gas of balls in a box; add balls with the mouse.
