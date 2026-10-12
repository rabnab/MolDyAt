# Headless Renderer Plan

## Goal

Add a non-windowed renderer mode that can run the simulation without WGPU or a UI event loop and emit simulation snapshots to the console, a file, or both.

This mode should be useful for:

- debugging simulation state without opening a window
- collecting reproducible output for tests and CI
- exporting frame-by-frame state for later analysis or visualization

Issue coverage status:

- #179: complete (non-finite JSON policy defined and tested)
- #180: complete (contract, sequencing, detail modes, stdout/file split, and deterministic seeded output tests)

## Constraints

- Keep physics ownership in `moldyat_core::application::MainContent`
- Reuse existing snapshot generation via `MainContent::create_snapshot()`
- Avoid introducing renderer-specific dependencies into `core`
- Keep the default interactive startup path unchanged
- Preserve energy-conservation behavior by only observing simulation state, not changing physics logic
- Treat renderer snapshots as an internal observation source, not as the long-term exported file schema

## Proposed Shape

### Entry Point

Add a headless execution path in `wgpu_renderer` startup selection.

- Keep the current WGPU path as the default
- Add an explicit flag such as `--headless`
- Parse a small set of arguments with `std::env::args()` first; only introduce a CLI crate if the option surface grows substantially

### Runtime Model

Drive the simulation directly from `MainContent` rather than spinning up the window/event loop.

Proposed loop:

1. bootstrap a world rect with the same policy used by the interactive renderer
2. construct `MainContent`
3. optionally seed the scene with deterministic balls for non-empty output
4. run `simulation_step()` for a fixed number of steps
5. call `create_snapshot()` at the configured emission interval
6. forward each snapshot to one or more output sinks

This keeps the implementation close to the existing renderer data contract and avoids coupling headless mode to WGPU internals.

Emit a snapshot at step `0` before stepping begins unless explicitly disabled. This gives deterministic initial-state output for CI, regression fixtures, and energy-drift comparisons.

## Output Design

Use `RendererSnapshot` as the in-memory source, then map it to a dedicated headless output model owned by `wgpu_renderer`.

Do not treat `RendererSnapshot` itself as the exported contract. It is renderer-shaped and may evolve for rendering needs that should not break headless consumers.

Suggested first export model:

- `RunStart` record: one-time metadata
- `Frame` record: per-snapshot simulation data
- `RunEnd` record: optional completion summary

Add a top-level schema version from the start so later format extensions remain compatible.

### Console Sink

Print a compact per-snapshot summary intended for humans.

Recommended fields:

- simulation step
- simulated time
- ball count
- kinetic energy
- potential energy
- total energy
- total energy delta from initial snapshot
- operator counts

Example direction:

```text
step=120 time=1.20000 balls=32 kinetic=14.72 potential=-21.04 total=-6.32 dE=0.00031
```

Do not print `status_text` from `RendererSnapshot`. That field is UI-oriented and currently depends on renderer-style display concerns rather than a stable headless contract.

### File Sink

Write structured snapshots for tooling.

Preferred first format:

- JSON Lines (`.jsonl`)

Recommended record contents:

- schema version
- record type (`run_start`, `frame`, `run_end`)
- top-level simulation summary
- world metadata and run configuration
- operator counts
- optional full per-ball detail
- optional thermo data

Reasoning:

- append-friendly
- easy to inspect manually
- easy to consume from scripts or notebooks

Default to a summary-oriented frame record and make large payloads opt-in.

Suggested default `frame` fields:

- simulation step
- simulated time
- ball count
- kinetic energy
- potential energy
- total energy
- total energy delta from initial frame
- gravity preset
- operator counts

Suggested opt-in detail sections:

- `balls`: per-ball position, velocity, radius, size class, mass
- `thermo`: cell temperature/pressure grids and grid specs
- `balls+thermo`: both sections together
- `operators`: full operator geometry (planned follow-up; not in the initial v1 export)

Important details:

- Do not export `status_text` as a structured field
- Do not claim per-ball neighbor counts unless a dedicated, always-populated source is added; the current snapshot only populates `neighbors` for verbose visualization paths
- JSON non-finite policy (implemented): any non-finite `f32` (`NaN`, `+Inf`, `-Inf`) is encoded as `null` in all exported numeric positions, including thermo grids/specs and summary fields

### Implemented JSONL v1 Contract

The implementation emits versioned JSON Lines records (`schema_version = "headless.v1"`):

- `run_start`: configuration and initial metadata
- `frame`: periodic simulation snapshots
- `run_end`: completion metadata

`frame` emission sequence is guaranteed as:

1. initial frame at simulation step `0`
2. periodic frames at `snapshot_every` boundaries (if non-zero)
3. final frame at the last step, even when the interval does not divide total steps

This sequencing is covered by runner tests and should be treated as contract behavior for CI fixtures.

## Initial CLI Surface

Keep the first version narrow.

Suggested flags:

- `--headless`
- `--steps <n>`
- `--snapshot-every <n>`
- `--output <path>`
- `--format jsonl`
- `--detail <summary|balls|thermo|balls+thermo>`
- `--rng-seed <n>`
- `--stdout`
- `--no-stdout`
- `--seed-balls <n>`
- `--seed-speed <max>` (seeded balls get speeds uniform in `0..=max` in random directions; default 0 = at rest, which draws no extra random numbers; net momentum is not removed)
- `--world-scale <factor>` (multiplies the default world's width and height around its centre; positive; for large-world performance runs)
- `--dt <seconds>` (time step override; positive)
- `--line-segment x1,y1,x2,y2[,thickness]` (repeatable wall in world coordinates; thickness 1-20, default 1; placed before seeding)

Default behavior in headless mode:

- stdout enabled
- one step if `--steps` is omitted
- snapshot emitted at step `0`
- snapshot emitted on the final step even if the interval does not divide evenly
- summary-only JSON records unless a higher detail level is requested

## Seeding Strategy

Headless mode needs a simple way to produce non-empty output without UI interaction.

Recommended first version:

- deterministic grid seeding in world coordinates
- optional small initial velocities so output changes between frames
- seeding metadata included in `run_start`
- when RNG-backed behavior is introduced, require an explicit seed in headless mode to support reproducible CI fixtures

Avoid as a first step:

- random seeding hidden behind non-deterministic RNG
- exposing private core helpers only for headless mode

If deterministic grid seeding proves too limited, a later step can add:

- explicit scenario files
- public command application helpers
- seeded random initialization

Document determinism boundaries explicitly. For CI use, headless mode should be deterministic when run with the same RNG seeds and the same relevant simulation configuration.

## Module Boundaries

### `core`

Prefer no required changes for the first version if the mode only:

- constructs `MainContent`
- advances via `MainContent::simulation_step()`
- reads state via `create_snapshot()`
- seeds via already-public constructors/helpers

Only widen `core` visibility if an actual blocker appears.

If export needs data that `RendererSnapshot` does not expose reliably for headless use, prefer adding explicit observer-oriented accessors or a dedicated headless projection over overloading renderer-only fields.

### `wgpu_renderer`

Place the implementation in a dedicated module such as `wgpu_renderer/src/headless.rs`.

Headless mode should remain in `wgpu_renderer` so the same binary can provide both graphical and non-graphical execution paths.

Suggested responsibilities:

- parse headless args
- own output sink setup
- run the direct simulation loop
- define the versioned headless record schema
- map `RendererSnapshot` into console/file output formats

## Testing Plan

Minimum validation for the first implementation:

1. argument parsing tests
2. snapshot formatting tests for console output
3. structured record serialization tests for file output, including schema version and non-finite thermo handling
4. a smoke test that runs a few steps headlessly and emits an initial snapshot plus at least one stepped snapshot
5. an energy-delta output test that verifies the initial frame is the delta baseline
6. a determinism test that runs the same seeded headless configuration twice and verifies identical emitted records

Implemented coverage in `wgpu_renderer` now includes:

- CLI parsing tests for headless flags, defaults, and validation errors
- contract serialization tests for `run_start` / `frame` / `run_end`
- sequencing tests for step-0, interval, and final snapshot behavior
- detail mode tests for summary vs balls and thermo inclusion
- non-finite encoding tests for summary and thermo payloads
- stdout/file-path behavior split tests
- deterministic seeded run test asserting byte-identical JSONL output for same config and seed
- checked-in seeded regression fixture comparison test for CI drift detection

### CI Fixture Strategy (Implemented)

Fixture strategy uses checked-in golden JSONL files:

- free gas, summary records: `wgpu_renderer/src/headless_runner/fixtures/seeded_headless_regression.jsonl`
  (test gate `seeded_headless_regression_fixture_matches_checked_in_jsonl`)
- operators (walls, a valve, a thermostat, a source sphere), per-ball state:
  `wgpu_renderer/src/headless_runner/fixtures/seeded_headless_operators.jsonl`
  (test gate `seeded_headless_operators_fixture_matches_checked_in_jsonl`)
- CI: the `Run seeded headless regression fixture` step of the `test` job in `.github/workflows/ci.yml`
- thread-count reproducibility: `seeded_operators_run_does_not_depend_on_thread_count`
  runs the operator scenario with 1 and with 4 Rayon threads and compares the f32 bits

Failure behavior:

- CI fails when any record ordering, field value, or line count drifts from the fixture
- failure output identifies the first mismatching JSONL line (expected vs actual)

Intentional fixture update workflow:

1. Apply the intended headless contract change.
2. Refresh the fixtures:
   `UPDATE_HEADLESS_FIXTURE=1 cargo ci-test-headless-regression`
3. Re-run the same command without `UPDATE_HEADLESS_FIXTURE` to confirm stability.
4. Include the fixture diff in PR review as contract evidence.

If the implementation touches `core`, add focused tests there rather than only exercising the path from `wgpu_renderer`.

## Validation Plan

Canonical command policy lives in `.github/copilot-instructions.md#quick-build-test-and-lint-commands-explicit`.
When implementation starts, validate in this order:

1. `cargo test -p moldyat_wgpu`
2. `cargo build -p moldyat_wgpu`
3. `cargo fmt`
4. `cargo clippy --workspace -- -D warnings`

If implementation widens or changes `core`, also run the affected `moldyat_core` tests instead of validating only `wgpu_renderer`.

For physics-affecting changes, add an energy-drift check. A pure observer/export path should not require physics changes, but the headless output should still expose enough energy data to detect drift.

## Risks

- CLI selection logic could accidentally change default startup behavior
- snapshot files may become large quickly if full ball state is emitted every step
- tying the exported file contract directly to `RendererSnapshot` would make future renderer changes harder
- seeded determinism must be preserved for CI fixtures and regression tests
- thermo export can fail or become non-portable if non-finite values are written without a clear encoding policy
- deterministic seeding must avoid overlapping balls if it bypasses normal collision checks
- there may be pressure to grow the mode into a scenario runner; keep the first scope narrow

## Resolved Decisions

- Summary-only output is the default; full ball detail is opt-in.
- Headless mode stays inside `wgpu_renderer` so the same binary supports graphical and headless execution.
- The first version of detailed ball export should include velocities and masses.
- CI is expected to use headless mode for regression fixtures and snapshot-style tests.
- Determinism for CI users should be guaranteed when runs use the same RNG seeds and equivalent simulation configuration.

## Recommended Implementation Order

1. Add doc-approved CLI contract and startup selection
2. Add direct `MainContent` stepping loop
3. Add versioned headless record schema with `run_start`/`frame`/`run_end`
4. Add console summary sink
5. Add JSONL file sink with summary-only default records
6. Add deterministic seeding
7. Add optional detailed per-ball export
8. Add focused tests and final lint/format validation
