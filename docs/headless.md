# Headless Mode

`moldyat --headless` runs the simulation without a window, GPU or event
loop and writes its state as JSON Lines. It is used for reproducible
regression fixtures in CI, for performance measurements
(`scripts/headless_perf.sh`, see [perf-baseline.md](perf-baseline.md)) and
for exporting runs for analysis.

The design history is in
[headless-renderer-plan.md](done/headless-renderer-plan.md).

## How a run works

The headless code lives in `crates/thermo_runtime/src/` (`headless_cli.rs`,
`headless_runner.rs`, `headless_output.rs`, `headless_timings.rs`), a crate
without a windowing or GPU stack. The same `moldyat` binary runs both
modes: it parses the command line and the shared options (`--load`,
`--config`, `--world-scale`, `--save-dir`) and, for `--headless`, hands the
effective `SimConfig` to the runner, together with
the git commit it was built from (the `commit` of the timings report).

1. `split_common_args` (binary) takes the shared options and
   `parse_headless_args` the headless ones below; without `--headless` the
   window starts as usual.
2. The runner builds a `MainContent` in the config's `world.rect()`, the
   same default world as the window (scaled by `world.scale` or
   `--world-scale`), or loads one with `--load`,
   sets the time step, adds walls and valves, then seeds balls, so seeded
   balls never start inside a wall.
3. It steps the simulation directly on the main thread with
   `MainContent::simulation_step()`; there is no physics thread.
4. Snapshots come from `CreateSnapshot::create_snapshot()` and are mapped
   to the export records in `headless_output.rs`. The renderer snapshot
   itself is not the export format, so it can change without breaking
   consumers.

Frames are written at step 0, at every `--snapshot-every` boundary, and at
the last step even when the interval does not divide the step count.

## Command line

| Flag                                  | Meaning                                                                                                                                                                                                                                                                                                                                  |
|---------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `--headless`                          | Run without a window; enables the options below                                                                                                                                                                                                                                                                                          |
| `--steps <N>`                         | Physics steps to run (default 1)                                                                                                                                                                                                                                                                                                         |
| `--snapshot-every <N>`                | Write a frame every N steps (default 0: first and last only)                                                                                                                                                                                                                                                                             |
| `--output <PATH>`                     | Write JSONL records to this file                                                                                                                                                                                                                                                                                                         |
| `--format jsonl`                      | Output format (only `jsonl`)                                                                                                                                                                                                                                                                                                             |
| `--detail <LEVEL>`                    | `summary` (default), `balls`, `thermo` or `balls+thermo`                                                                                                                                                                                                                                                                                 |
| `--stdout` / `--no-stdout`            | Print a summary line per frame (default) / no console output (needs `--output`)                                                                                                                                                                                                                                                          |
| `--rng-seed <N>`                      | Seed for reproducible runs                                                                                                                                                                                                                                                                                                               |
| `--seed-balls <N>`                    | Place up to N random balls before the first step                                                                                                                                                                                                                                                                                         |
| `--seed-speed <V>`                    | Seeded balls get speeds uniform in `0..V` in random directions (default 0: at rest); net momentum is not removed                                                                                                                                                                                                                         |
| `--seed-species <NAME[:W],...>`       | Seed a mixture of species of the run's table (the `--config` file's `[[species]]`, or the built-in library) with integer weights `W` (default 1), e.g. `N2:3,Ar:1`; needs `--seed-balls`. Species are assigned in a fixed interleaved order (each ball gets the species furthest behind its share), so no extra random numbers are drawn |
| `--dt <SECONDS>`                      | Time step, `0.0003125..=0.08` (the range the `+` / `-` keys keep to; default: the simulation default, or the `--config` file's); also overrides a loaded save's                                                                                                                                                                          |
| `--line-segment <x1,y1,x2,y2[,t]>`    | Add a wall in world coordinates, thickness 1..20 (default 1); repeatable                                                                                                                                                                                                                                                                 |
| `--valve <wall,t0,t1[,open\|closed]>` | Cut a valve into `--line-segment` number `wall` (0-based) over fractions `t0..t1` of its length; one per wall, default open                                                                                                                                                                                                              |
| `--save-setup`                        | Save the prepared world to `--save-dir` before the first step                                                                                                                                                                                                                                                                            |
| `--timings <PATH>`                    | Write ms/step per phase as JSON (see [perf-baseline.md](perf-baseline.md))                                                                                                                                                                                                                                                               |
| `--timing-warmup <N>`                 | Steps before `--timings` starts measuring (default 0)                                                                                                                                                                                                                                                                                    |
| `--export-dir <DIR>`                  | After the last step, write `series.<ext>` (the metrics series of the whole run, one row per snapshot) and `snapshot.<ext>` (state of every ball at the final snapshot) to `DIR`; see [Data export](#data-export)                                                                                                                         |
| `--export-format <csv\|jsonl>`        | Format of the export files (default `csv`); needs `--export-dir`                                                                                                                                                                                                                                                                         |
| `--export-series <A,B,..>`            | Export only these series (default: all); an unknown name fails the run; needs `--export-dir`                                                                                                                                                                                                                                             |
| `--export-stream`                     | Also append the sample of every frame (all series, not thinned) to `stream.<ext>` while the run goes; see [Sample stream](#sample-stream); needs `--export-dir`                                                                                                                                                                          |
| `--load <PATH>`                       | Start from a saved world (shared with the window mode)                                                                                                                                                                                                                                                                                   |
| `--save-dir <DIR>`                    | Directory for saves (shared with the window mode)                                                                                                                                                                                                                                                                                        |
| `--world-scale <F>`                   | Scale the default world's width and height by F around its centre, 0.05..=100; overrides `world.scale` of `--config`; not with `--load` (shared with the window mode, parsed by the binary)                                                                                                                                              |
| `--config <PATH>`                     | Simulation config file, TOML, or JSON if the name ends in `.json` (shared with the window mode; see the README, "Simulation config"); not with `--load`                                                                                                                                                                                  |

`moldyat --help` prints the same list from the option tables in
`headless_cli.rs` and `cli.rs`.

## Console output

With `--stdout` each frame prints one line:

```text
step=120 time=1.20000 balls=32 kinetic=14.72000 potential=-21.04000 total=-6.32000 dE=0.00031 operators=0/0/0/0
```

`dE` is the total energy change since the step-0 frame; `operators` counts
line segments, valves, source spheres and thermostats.

## JSONL records (`headless.v2`)

Every line is one JSON object with `schema_version: "headless.v2"` and a
`record_kind`. Consumers should ignore unknown record kinds and unknown
fields. v2 (#234) replaced the per-ball `size_idx`, `size_class` and
`radius` of v1 by `species` and `contact_radius`; everything else is
unchanged.

| `record_kind` | When                              | Contents                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
|---------------|-----------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `run_start`   | once, first                       | `config` (steps, snapshot interval, format, detail level, RNG seed, seeding, `world_scale` (the run's `world.scale`, `null` for the default world), dt, walls and valves; `seed_species` as `[{species, weight}]` only with `--seed-species`; `sim`, the effective `SimConfig` after `--config`, `--world-scale`, `--dt` and a loaded save, in the layout of a `--config` file: `world` when not the default, `integration`, `potential`, `neighbours`, `display`, and `species` when the run has a `[[species]]` list; written to a `.json` file it is a valid `--config`, e.g. `head -1 run.jsonl \| jq .config.sim > run.json`) and `initial` (step, time, ball count, gravity, operator counts, world rectangle) |
| `frame`       | step 0, every interval, last step | step, time, kinetic and potential energy (with gravity on, the potential energy includes `m·g·h`, `h` the height above the bottom wall, #334), `delta_energy` from the step-0 frame (drift only while gravity and the ball count stay fixed: changing gravity or adding balls shifts it), ball count, gravity, physics FPS, operator counts, `metrics` (latest value of each `MetricsRegistry` series, sorted by name: `E_kin`, `E_pot`, `E_tot`, `wall_momentum`, `<species>:count`; series without a sample are left out), plus the detail sections                                                                                                                                                                |
| `run_end`     | once, last                        | final step and time, number of frames written, final ball count, operator counts                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

Detail sections in `frame` records, chosen with `--detail`:

- `balls`: per ball, sorted by id: id, `species` (name), position,
  velocity, `contact_radius` (`σ / 2`), mass.
- `thermo`: the per-cell temperature and pressure grids and the grid spec.
  The fields are sampled every `THERMO_SAMPLE_STRIDE` steps (see
  [simulation.md](simulation.md#thermodynamic-grid-space)).

Non-finite numbers (`NaN`, `±Inf`) are written as `null` in every numeric
position, including the thermo grids.

## Determinism and fixtures

A run with the same `--rng-seed`, flags and build (same target and profile)
produces byte-identical output. Three tests in
`crates/thermo_runtime/src/headless_runner/tests.rs` rely on this:

| Test                                                               | Scenario                                                         | Fixture                                     |
|--------------------------------------------------------------------|------------------------------------------------------------------|---------------------------------------------|
| `test_seeded_headless_regression_fixture_matches_checked_in_jsonl` | free gas, summary records                                        | `fixtures/seeded_headless_regression.jsonl` |
| `test_seeded_headless_operators_fixture_matches_checked_in_jsonl`  | walls, a valve, a thermostat and a source sphere, per-ball state | `fixtures/seeded_headless_operators.jsonl`  |
| `test_seeded_operators_run_does_not_depend_on_thread_count`        | the operator scenario with 1 and 4 Rayon threads                 | compares the f32 bits of both runs          |

The two fixture tests are `#[ignore = "suite:headless-regression"]` and run
with `cargo ci-test-headless-regression`, in the `test` job of
`.github/workflows/ci.yml`. A mismatch fails with the first differing line
(expected and actual).

When a change is meant to alter the output, the fixtures are refreshed with
`UPDATE_HEADLESS_FIXTURE=1 cargo ci-test-headless-regression`, checked by a
second run without the variable, and the fixture diff goes into the PR as
evidence.

## Data export

`--export-dir` (headless) and the commands `world.export_series_csv` (`Ctrl + E`),
`world.export_series_jsonl` (`Ctrl + J`), `world.export_snapshot_csv` (`Alt + E`) and
`world.export_snapshot_jsonl` (`Alt + J`) write the metrics series and the per-ball
state for analysis outside the app (#337). The commands write timestamped files
`export_YYYYMMDD_HHMMSS_<series|snapshot>.<csv|jsonl>` to the save directory
(`--save-dir`) and never overwrite one; a headless run writes `series.<ext>` and
`snapshot.<ext>`, overwriting earlier ones, so scripts find them.

Both formats follow the headless contract: a non-finite `f32` (`NaN`, `±Inf`) and a
missing value are `null` in JSONL and an empty cell in CSV, floats are written in
their shortest round-trip form, and the same seed, flags and build give
byte-identical files. JSONL records carry `schema_version: "export.v1"` and a
`record_kind`; consumers ignore unknown kinds and fields.

| File     | CSV                                                                                                                                                                                        | JSONL                                                                                                                                                 |
|----------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------|
| series   | Header `step,<name>[ [<unit>]],…` with the series sorted by name (`E_kin [J]`, `N2:count`), then one row per sampled step. A series registered later has empty cells for the earlier steps | `series_header` (`rows`, `series`: name, unit, description, per_species), then one `series_row` per step (`step`, `values` object)                    |
| snapshot | `id,species,x,y,vx,vy,mass,contact_radius`, one row per ball sorted by id                                                                                                                  | `snapshot_header` (`step`, `time`, `ball_count`), then one `ball` record per ball (`id`, `species`, `position`, `velocity`, `contact_radius`, `mass`) |

The series are those of the `MetricsRegistry` (`E_kin`, `E_pot`, `E_tot`,
`wall_momentum`, `<species>:count`), sampled at snapshot cadence. The registry keeps
the last 1000 samples, so:

- **Headless runs** export the whole run. The run length is known up front, so the
  export samples only every k-th frame (`k = ceil(frames / 999)`) and always the
  last one, whenever the frames of `--snapshot-every` would not fit; the first step
  is always in. The frame interval and the `metrics` of the frames are unchanged.
  The effective distance in steps (`k * snapshot_every`) is recorded as
  `sample_interval_steps`: in the JSONL `series_header` (`null` when every frame
  was sampled) and, only for a thinned run, as a first CSV line
  `# sample_interval_steps=N` before the header row (readers should skip comment lines, e.g. `pd.read_csv(path, comment="#")`; the `step` column gives the spacing in any case).
- **Interactive exports** (`Ctrl + E` / `Ctrl + J`) hold the last 1000 samples. When
  older ones were evicted, the physics thread logs a warning and the
  `DataExported` event carries a note.

An unknown `--export-series` name (names are trimmed, none may be empty) fails the
run before the first step. An export error is returned after `run_end` and the
timings report have been written.

### Sample stream

The series export holds what the registry holds. To keep every sample of a long
run, a sample stream appends each sample to a file as it is taken (#366):

- **Interactive:** `world.toggle_sample_stream_csv` (`Ctrl + R`) or
  `world.toggle_sample_stream_jsonl` (`Alt + R`) starts a stream to
  `export_YYYYMMDD_HHMMSS_stream.<csv|jsonl>` in the save directory, from the next
  sample on; either key stops it. It is off at start. The physics thread writes it
  right after each sample (snapshot cadence) and reports `StreamEnabled`,
  `StreamFile` (a file was started), `StreamStopped` and `StreamFailed` events,
  which the window logs. Quitting stops the stream with its rows on disk.
- **Headless:** `--export-stream` (needs `--export-dir`, uses `--export-format`)
  writes `stream.<ext>` with the sample of every frame (`--snapshot-every`),
  overwriting an earlier one. It does not change the JSONL records, and
  `--export-series` does not apply to it. The path is listed with the other
  export files.

Rows go through a buffered writer that is flushed within about a second
(`FLUSH_INTERVAL`) and when the stream stops, so a crash loses at most the last
second. A new world (a loaded save) or another set of series (another species
table) starts a new file (`_01`, `_02`, … suffix), so one file never mixes
worlds. A write error stops the stream and is reported once; the simulation goes
on. In a headless run the error is returned after `run_end`, the timings report
and the other export files have been written.

| Format | Contents                                                                                                                                                                                   |
|--------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| CSV    | Header `step,time,<name>[ [<unit>]],…` (series sorted by name), then one row per sample; non-finite values are empty cells                                                                 |
| JSONL  | `stream_header` (`series`: name, unit, description, per_species), then one `series_row` per sample (`step`, `time`, `values` object); non-finite values are `null`, `export.v1` throughout |
