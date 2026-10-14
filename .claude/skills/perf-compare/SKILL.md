---
name: perf-compare
description: Procedure for measuring and reporting simulation performance - comparing a branch against master with scripts/headless_perf.sh, judging noise, reporting numbers in a PR, and updating docs/perf-baseline.json. Use for performance work (refactoring-plan P items), any change to hot paths (force kernel, neighbour rebuilds, apply pass, cell grid, snapshot), or any claim that something is faster or slower.
---

# Performance Comparison

Background, scenarios and the committed baseline are in `docs/perf-baseline.md`. This skill is the procedure.

## Rules
- Compare two binaries **in one invocation** of `scripts/headless_perf.sh`; they alternate within each round, so host load hits both. Never compare runs from different days or machines, and never compare your numbers with `docs/perf-baseline.json` unless you are on the machine recorded there.
- Both binaries must be built for the same target and profile (`--release`, which uses `target-cpu=native`) and include #221 (they need `--timings`, `--load`, `--save-setup`).
- A change is real only when it exceeds the `spread_pct` of **both** binaries. On shared or cloud hosts, treat differences below about 10 % as noise.
- Performance work must not change results: the drift suite and headless fixtures stay green (see the `physics-change` skill if results move).

## Procedure
```bash
git switch master && cargo build --release -p moldyat_wgpu
cp target/release/moldyat "$TMPDIR/moldyat-base"
git switch <branch> && cargo build --release -p moldyat_wgpu
scripts/headless_perf.sh "$TMPDIR/moldyat-base" target/release/moldyat > perf.json
```
- Progress goes to stderr; the JSON report (`headless-perf.v2`) to stdout or `OUT`.
- Narrow or tighten a run with environment variables: `SCENARIOS` (`dilute dense walls_valves box_walls large`), `STEPS` (300), `WARMUP` (50), `RUNS` (5), `THREADS` (default cores - 2), `SNAPSHOT_EVERY` (10). For short scenarios use e.g. `SCENARIOS=dilute STEPS=1000 RUNS=9`. Pin `THREADS` when comparing on a busy machine.
- Seeded worlds are cached in `target/perf-worlds/`; delete the cache when the save format or the seeding changes.

Ratio of medians per scenario and phase:
```bash
jq -r '.results | group_by(.scenario)[] | .[0] as $a | .[1] as $b
  | $a.ms_per_step | keys_unsorted[] as $p
  | "\($a.scenario)\t\($p)\t\($a.ms_per_step[$p].median)\t\($b.ms_per_step[$p].median)\t\(($b.ms_per_step[$p].median / ($a.ms_per_step[$p].median + 1e-12) * 100 - 100) | floor)%"' perf.json
```

## Reading the phases
| Phase | Times |
|---|---|
| `neighbours` | Verlet list rebuilds |
| `positions` | position update and wall contacts |
| `forces` | SIMD pair forces |
| `apply` | velocity update, stats, wall overlaps, thermostats |
| `cells` | source spheres, cell-grid moves, invalid-ball removal |
| `thermo` | global temperature, per-cell T/P fields |
| `snapshot` / `output` | snapshot creation / record writing |
| `other` | `total` minus the above |

A change should move the phase it targets. If `total` moves but the phase does not, suspect noise or code layout.

## Reporting in the PR
Put this under "Testing & Validation" or "Technical Details":
- the report's `machine` entry (CPU, cores, OS) and, per binary, `commit`, `profile` and `threads`;
- per scenario: median `total` of both binaries, the changed phases, the ratio, and both `spread_pct` values;
- a sentence on whether the change exceeds the noise, and on any scenario that got slower.

## Updating the committed baseline
Only after an intended, merged performance change, on the machine recorded in `docs/perf-baseline.md` (or note the new machine):
```bash
OUT=docs/perf-baseline.json scripts/headless_perf.sh
```
Then update the Baseline table (commit, date, machine, medians) in `docs/perf-baseline.md`. Changing a scenario's arguments in the script requires a new baseline.
