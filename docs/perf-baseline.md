# Performance baseline

Reproducible ms/step numbers, so a refactor can show its numbers before and
after (#221, plan item G1 in [`refactoring-plan.md`](plan/refactoring-plan.md)). The raw report is in
[`perf-baseline.json`](perf-baseline.json).

## One command

```bash
cargo build --profile perf -p moldyat_wgpu
scripts/headless_perf.sh > perf.json        # all scenarios, JSON on stdout
```

The `perf` profile (workspace `Cargo.toml`) inherits `release`, including
`target-cpu=native`, but uses thin LTO and 16 codegen units, so it builds
much faster than `release` (fat LTO, one codegen unit). Its absolute numbers
differ slightly from `release`; compare only binaries of the same profile.

Progress goes to stderr. The script's header lists the settings
(`SCENARIOS`, `STEPS`, `WARMUP`, `RUNS`, `THREADS`, `OUT`, ...). It needs
bash 4+, coreutils and awk: on Linux or WSL, not PowerShell.

## Scenarios

All scenarios are seeded with `--rng-seed 1`. The default world is 112 x 62,
and `--world-scale` multiplies both sides.

| Scenario | Balls | World scale | Seed speed | Covers |
|---|---|---|---|---|
| `dilute` | 10 000 | 16 | 6 | hot sparse gas, many neighbour-list rebuilds |
| `dense` | 50 000 | 10 | 0 (at rest) | many neighbours per ball, force kernel |
| `walls_valves` | 20 000 | 8 | 4 | 4 walls (thickness 2), 1 closed and 2 open valves; wall sweep and overlap passes |
| `box_walls` | 6 000 | 4 | 8 | hot gas in a small box: about 3x the world-wall contacts per step of `dilute`; wall contacts and wall momentum bookkeeping (in `positions`) |
| `large` | 200 000 | 28 | 2 | ball data far beyond the cache, memory-bound |

How a run works:

- **Seeding once.** Each scenario's world is seeded once with
  `--save-setup` and cached in `target/perf-worlds/<scenario>-<key>/`. Every
  run then starts from it with `--load`. All runs and all binaries start from
  the same state. Seeding checks each candidate only against the balls in
  its neighbouring grid cells (#245): about 0.4 s for `dense` and 2 s for
  `large` on the machine below (before #245 it was O(n²): 18 s and about
  5 minutes). The cache key is a hash of the seeding arguments, including
  the time step. Delete the cache when the save format or seeding changes.
- **Time step.** Seeding and every run use `--dt $DT` (default 0.01, the
  step the committed baseline was measured at), not the binary's default
  (0.005 since #301). The step sets how far balls move per step and so the
  neighbour rebuilds per step; pinning it keeps binaries with different
  defaults comparable. Recapture the baseline before changing `DT`.
- **Timing.** Each run does `STEPS=300` steps. The first `WARMUP=50` steps
  are not counted, because the at-rest `dense` world relaxes in them. The
  binary times itself (`--timings`), so process start and loading are left
  out. There is a snapshot every 10 steps (`--detail summary`, written to
  `/dev/null`).
- **Rounds.** There are `RUNS=5` rounds. In each round every binary runs
  once, alternating, and the report gives the median, min and max for each
  phase, plus `spread_pct = (max - min) / median`.

## Phases

The phases are timed in `crates/thermo_physics/src/application/phase_timings.rs`, and the
runner adds `snapshot`, `output` and `total`:

| Phase | What it times |
|---|---|
| `neighbours` | Verlet list rebuilds and neighbour statistics |
| `positions` | position update with wall contacts, kernel point copy |
| `forces` | pair forces (SIMD kernel) |
| `apply` | velocity update and stats, wall overlaps, thermostats |
| `cells` | source spheres, cell-grid moves, removal of invalid balls |
| `thermo` | global temperature and the per-cell T/P fields |
| `snapshot` | `create_snapshot` |
| `output` | writing the snapshot record |
| `other` | `total` minus all of the above (old-position copy, loop overhead) |
| `total` | wall clock of the measured steps / steps |

All values are ms per measured step. Snapshot costs are spread over all steps.

## Baseline

Captured from commit `6b50e9e` (the binary reports it; see
[`perf-baseline.json`](perf-baseline.json), schema `headless-perf.v2`), on
2026-10-08:

- **CPU:** Intel Xeon @ 2.80 GHz, 4 vCPUs (cloud container, shared host)
- **Threads:** 2 rayon workers (default: cores - 2)
- **Profile:** `release` (`target-cpu=native`, fat LTO), captured before the
  `perf` profile existed; new runs use `perf`, so their absolute numbers are
  not directly comparable with this baseline
- **OS:** Linux 6.18

Median ms/step:

| Scenario | Balls | total | neighbours | positions | forces | apply | cells | thermo | snapshot | other |
|---|---|---|---|---|---|---|---|---|---|---|
| dilute | 10 000 | 3.24 | 1.22 | 0.70 | 0.39 | 0.27 | 0.26 | 0.33 | 0.04 | 0.04 |
| dense | 50 000 | 18.96 | 2.89 | 3.03 | 6.36 | 2.51 | 1.96 | 1.69 | 0.23 | 0.28 |
| walls_valves | 20 000 | 8.28 | 2.36 | 1.57 | 2.06 | 1.11 | 0.50 | 0.46 | 0.08 | 0.10 |
| box_walls | 6 000 | 2.64 | 1.20 | 0.43 | 0.49 | 0.18 | 0.18 | 0.10 | 0.02 | 0.02 |
| large | 200 000 | 113.93 | 25.42 | 14.20 | 34.12 | 11.62 | 11.13 | 13.60 | 1.33 | 2.20 |

`output` is about 0.003 ms in every scenario.

`box_walls` replaces the former `test_wall_momentum_performance_impact`,
which timed 1000 balls but asserted nothing. All scenarios except `large`
had a `total` `spread_pct` of 14–19 % in this capture; see Variance.

Do not compare absolute numbers across machines, build profiles (`release`
uses `target-cpu=native`, `dist` uses x86-64-v2) or thread counts. Recapture
the baseline on the machine you compare on.

### Variance

All five captures ran on the machine above with code whose timed paths are
the same as in `6b50e9e` (#245 changed only seeding, which is not timed).
Runs 1 to 3 are from earlier versions of #221 before its history was
consolidated: runs 1 and 2 are two full invocations one after the other
(2026-10-06), run 3 is from the morning of 2026-10-07. Run 4 is the previous
baseline (`4b41546`, the evening of 2026-10-07) and run 5 the baseline above
(2026-10-08). `box_walls` exists only in runs 4 and 5:

| Scenario | total, run 1 | total, run 2 | total, run 3 | total, run 4 | total, run 5 | Run 1 vs 2 | spread_pct within a run |
|---|---|---|---|---|---|---|---|
| dilute | 3.92 | 3.70 | 3.52 | 3.21 | 3.24 | 5.7 % | 8–31 % |
| dense | 21.49 | 21.17 | 19.18 | 19.95 | 18.96 | 1.5 % | 6–19 % |
| walls_valves | 10.23 | 9.35 | 8.03 | 8.75 | 8.28 | 8.6 % | 4–20 % |
| box_walls | – | – | – | 2.84 | 2.64 | – | 19–32 % |
| large | 117.36 | 115.54 | 107.88 | 113.87 | 113.93 | 1.5 % | 4–9 % |

Back to back, the 5 % target holds for `dense` and `large`. It does not hold
for the short scenarios (`dilute`, `box_walls`: each run is only about 1 s)
or for `walls_valves` in run 1, which saw interference from the shared host.
Across runs on different days and times, the whole host shifts by up to
about 10 %. On this kind of machine, treat differences below
about 10 % as noise, and never compare runs from different days. Expect less variance on an idle
desktop with a fixed CPU frequency. To reduce it:

- Compare binaries **in one invocation** (see below). They alternate within
  each round, so host load affects both of them.
- Raise `RUNS` (e.g. 9) and `STEPS` (e.g. 1000 for `dilute`, using
  `SCENARIOS=dilute STEPS=1000`).
- Pin the thread count (`THREADS=2`) and keep the machine idle.

### Pre-#221 reference

Binaries built before #221 lack `--timings`, `--load` and `--save-setup`, so
this script cannot measure them. For a rough reference, master before #221
(`62c7e7c`) and this branch (as of 2026-10-06, before its history was
consolidated) were timed with the old outside
method. That method times two whole runs, 1 step and 300 steps, and takes
the difference. Both runs include seeding, which was O(n²) at the time
(before #245) and took about 20 s at 50k balls against about 9 s of
stepping. Seeding noise therefore
does not cancel, and it dominates the `dense` numbers. The method has no
warmup, so the at-rest relaxation of `dense` is included.

Same machine as above, `release`, median (min–max) ms/step:

| Scenario (old method) | Rounds | `62c7e7c` (pre-#221) | this branch |
|---|---|---|---|
| dilute: 10k, scale 16, speed 6 | 5 | 4.97 (4.34–5.07) | 4.71 (4.41–5.25) |
| dense: 50k, scale 10, at rest | 5 | 28.79 (26.89–29.87) | 30.77 (28.39–33.07) |
| dense, binary order swapped | 9 | 30.63 (25.10–32.25) | 32.20 (24.09–37.33) |

How to read this:

- **Resolution.** On this host the old method cannot resolve changes below
  about 10 %.
- **Dense direction.** `dense` is 5–7 % slower on the branch in both binary
  orders. That direction is consistent, but this method cannot confirm or
  rule it out.
- **Scope.** The branch changes more than the timers: valve and line
  segment API, accessors and the constructor.
- **Threads.** Both binaries ran with 2 threads only because this host has 4
  cores. `62c7e7c` ignores `RAYON_NUM_THREADS` and always uses cores − 2.
- **Not comparable** with the tables above, which use a warmup and loaded
  worlds.

The cost of the timers themselves is measured directly in the next section.

### Cost of the phase timers

One commit of this branch (before its history was consolidated; the timed
code paths are the same as in `6b50e9e`) was built twice, once as it is and once with the phase
timers in `simulation_engine.rs` compiled out (`Instant` replaced by a no-op
stub, not committed). Both binaries ran in one invocation with alternating
rounds, `RUNS=5`, same machine. Total median (min–max) ms/step:

| Scenario | Timers on | Timers off | Difference |
|---|---|---|---|
| dilute | 3.57 (3.35–3.82) | 3.67 (2.89–3.83) | −2.5 % |
| dense | 19.80 (19.65–21.27) | 19.96 (19.06–20.58) | −0.8 % |
| walls_valves | 8.85 (8.06–8.97) | 9.03 (7.88–10.10) | −2.0 % |
| large | 112.19 (107.59–115.26) | 106.15 (105.39–109.41) | +5.7 % |

The sign changes between scenarios, and every pair of ranges overlaps. The
timers are about ten `Instant::now()` calls per step, well under 1 µs,
against steps of 3.5 to 110 ms. The timer cost is therefore below what this
host can resolve. The `large` difference is too big to come from the calls
themselves; it is noise or a code-layout effect.

## Comparing a branch against the baseline

Build both binaries for the same target and profile, then run them in one
invocation. Both must include #221: the base is master after #221 was merged.
Older binaries fail at seeding (`Unknown argument: --save-setup`); for those,
see [Pre-#221 reference](#pre-221-reference).

```bash
git switch master && cargo build --profile perf -p moldyat_wgpu
cp target/perf/moldyat /tmp/thermo-base
git switch my-branch && cargo build --profile perf -p moldyat_wgpu
scripts/headless_perf.sh /tmp/thermo-base target/perf/moldyat > perf.json
```

The report then has two `results` entries per scenario, one for each
binary. The first binary seeds the cached worlds, so both start from the
same state. Ratio of medians per scenario and phase (needs `jq`):

```bash
jq -r '.results | group_by(.scenario)[] | .[0] as $a | .[1] as $b
  | $a.ms_per_step | keys_unsorted[] as $p
  | "\($a.scenario)\t\($p)\t\($a.ms_per_step[$p].median)\t\($b.ms_per_step[$p].median)\t\(($b.ms_per_step[$p].median / ($a.ms_per_step[$p].median + 1e-12) * 100 - 100) | floor)%"' perf.json
```

In a PR, report the `total` and changed-phase medians of both binaries and
the machine line from the report. A change is real when it exceeds the
`spread_pct` of both binaries.

To update the committed baseline after an intentional change, rerun
`OUT=docs/perf-baseline.json scripts/headless_perf.sh` on the machine above
or note the new machine, and update the tables here.

Comparing against `docs/perf-baseline.json` directly only makes sense on the
same machine. Otherwise run the base commit's binary next to yours as shown
above.
