---
name: headless-contract
description: Contract for changing headless mode - CLI flags, the JSONL headless.v1 records, console output, seeding, the timings report, or snapshot fields that headless exports - and for refreshing the seeded regression fixtures. Use before editing headless_cli.rs, headless_runner.rs, headless_output.rs, headless_timings.rs, cli.rs, the fixtures under headless_runner/fixtures/, or RendererSnapshot fields that headless output reads.
---

# Headless Contract

`docs/headless.md` describes headless mode as it is. Headless output is a contract: CI compares it byte for byte, scripts (`scripts/headless_perf.sh`) parse it, and users consume the JSONL. Changes must be deliberate and visible.

## Where things live
| Concern | File | Tests |
|---|---|---|
| Flags, defaults, validation, `--help` table (`HEADLESS_OPTIONS`) | `wgpu_renderer/src/headless_cli.rs` | `headless_cli/tests.rs` |
| Shared flags (`--load`, `--save-dir`), `--help` (`GENERAL_OPTIONS`) | `wgpu_renderer/src/cli.rs`, `cli/usage.rs` | `cli/tests.rs` |
| World setup order, step loop, frame cadence | `wgpu_renderer/src/headless_runner.rs` | `headless_runner/tests.rs` |
| JSONL records, console line, non-finite policy | `wgpu_renderer/src/headless_output.rs` | `headless_output/tests.rs` |
| `--timings` report (`headless-timings.v1`) | `wgpu_renderer/src/headless_timings.rs` | read by `scripts/headless_perf.sh` |

## Rules
- **Export model, not renderer model.** Records are built in `headless_output.rs` from `RendererSnapshot`. Do not serialize `RendererSnapshot` directly, and do not export UI fields such as `status_text`. If headless needs data the snapshot lacks, add an explicit field or accessor in core.
- **Versioning.** Every record carries `schema_version: "headless.v1"` and `record_kind`. Consumers ignore unknown kinds and fields, so adding a field or record kind keeps `v1`. Renaming or removing a field, changing its type, unit or meaning, or changing the record order is a breaking change: bump to `headless.v2` in all three records and document it in `docs/headless.md`.
- **Record sequence.** `run_start`, then a `frame` at step 0, at every `snapshot_every` boundary and at the last step (even if the interval does not divide it), then `run_end`. Runner tests treat this as contract.
- **Non-finite numbers.** Every exported `f32` goes through `f32_value`, so `NaN` and `±Inf` become `null`, including thermo grids and summary fields.
- **Payload size.** `summary` stays the default; per-ball and thermo data stay opt-in through `--detail`.
- **Flags.** Every flag that `parse_headless_args` accepts has an `OptionHelp` entry (`every_parsed_flag_is_documented` in `cli/tests.rs` checks it) and a row in `docs/headless.md`. Parse with `std::env::args()`; add a CLI crate only if the option surface grows a lot. The default (windowed) start must not change.
- **Setup order.** Time step, then walls and valves, then seeded balls, so seeded balls never start inside a wall. Seeding is deterministic for a given `--rng-seed`.
- **Determinism.** The same seed, flags and build give byte-identical output, independent of the Rayon thread count. Do not add wall-clock values or unordered collections to records (`physics_fps` is always 0 in headless runs, which have no physics thread).
- **Timings report.** `scripts/headless_perf.sh` parses `headless-timings.v1`. Changing its fields or phase names means updating the script (and its `headless-perf.v2` report) in the same change.

## Fixtures
Gates, all in the `test` job of `.github/workflows/ci.yml`:
- `cargo ci-test-headless-regression` (both fixture tests are `#[ignore = "suite:headless-regression"]`):
  `seeded_headless_regression.jsonl` (free gas, summary) and `seeded_headless_operators.jsonl` (walls, valve, thermostat, source sphere, per-ball state).
- `test_seeded_operators_run_does_not_depend_on_thread_count` (1 vs 4 threads, f32 bits).
- CI also runs the fixtures with `--no-default-features` (without `measure-time`); output must not depend on that feature.

A refactor must leave the fixtures untouched. If a fixture changes, the PR changes behaviour and says so:
1. Make the intended change and understand why the output moved (for physics, follow the `physics-change` skill first).
2. Refresh on x86_64 Linux with the dev profile (as CI): `UPDATE_HEADLESS_FIXTURE=1 cargo ci-test-headless-regression`. Never refresh on Windows; `sin`/`cos`/`exp` can differ there.
3. Rerun without `UPDATE_HEADLESS_FIXTURE` to confirm stability.
4. Put the fixture diff in the PR as evidence, with the reason.

## Checklist for a headless change
- Tests: CLI parsing (defaults, errors), record serialization (fields, schema version, non-finite values), sequencing, detail levels, stdout vs file.
- `cargo test -p moldyat_wgpu`, `cargo ci-test-headless-regression`, `cargo fmt`, `cargo clippy --workspace -- -D warnings`.
- Update `docs/headless.md` (flags table, record table, console line). If `scripts/headless_perf.sh` uses the changed flag, run it once.
