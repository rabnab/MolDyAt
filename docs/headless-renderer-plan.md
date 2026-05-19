# Headless Renderer Plan

## Goal

Add a non-windowed renderer mode that can run the simulation without WGPU or a UI event loop and emit simulation snapshots to the console, a file, or both.

This mode should be useful for:

- debugging simulation state without opening a window
- collecting reproducible output for tests and CI
- exporting frame-by-frame state for later analysis or visualization

## Constraints

- Keep physics ownership in `rust_thermo_core::application::MainContent`
- Reuse existing snapshot generation via `MainContent::create_snapshot()`
- Avoid introducing renderer-specific dependencies into `core`
- Keep the default interactive startup path unchanged
- Preserve energy-conservation behavior by only observing simulation state, not changing physics logic

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

## Output Design

### Console Sink

Print a compact per-snapshot summary intended for humans.

Recommended fields:

- simulation step
- simulated time
- ball count
- kinetic energy
- potential energy
- total energy
- operator counts

Example direction:

```text
step=120 time=1.20000 balls=32 kinetic=14.72 potential=-21.04 total=-6.32
```

### File Sink

Write structured snapshots for tooling.

Preferred first format:

- JSON Lines (`.jsonl`)

Recommended record contents:

- top-level simulation summary
- operator counts
- full per-ball positions/radii/neighbor counts
- optional status text

Reasoning:

- append-friendly
- easy to inspect manually
- easy to consume from scripts or notebooks

## Initial CLI Surface

Keep the first version narrow.

Suggested flags:

- `--headless`
- `--steps <n>`
- `--snapshot-every <n>`
- `--output <path>`
- `--stdout`
- `--no-stdout`
- `--seed-balls <n>`

Default behavior in headless mode:

- stdout enabled
- one step if `--steps` is omitted
- snapshot emitted on the final step even if the interval does not divide evenly

## Seeding Strategy

Headless mode needs a simple way to produce non-empty output without UI interaction.

Recommended first version:

- deterministic grid seeding in world coordinates
- optional small initial velocities so output changes between frames

Avoid as a first step:

- random seeding hidden behind non-deterministic RNG
- exposing private core helpers only for headless mode

If deterministic grid seeding proves too limited, a later step can add:

- explicit scenario files
- public command application helpers
- seeded random initialization

## Module Boundaries

### `core`

No required changes expected for the first version if the mode only:

- constructs `MainContent`
- advances via `GameLogic::simulation_step()`
- reads state via `create_snapshot()`
- seeds via already-public constructors/helpers

Only widen `core` visibility if an actual blocker appears.

### `wgpu_renderer`

Place the implementation in a dedicated module such as `wgpu_renderer/src/headless.rs`.

Suggested responsibilities:

- parse headless args
- own output sink setup
- run the direct simulation loop
- map `RendererSnapshot` into console/file output formats

## Testing Plan

Minimum validation for the first implementation:

1. argument parsing tests
2. snapshot formatting tests for console output
3. structured record serialization tests for file output
4. a smoke test that runs a few steps headlessly and emits at least one snapshot

If the implementation touches `core`, add focused tests there rather than only exercising the path from `wgpu_renderer`.

## Validation Plan

When implementation starts, validate in this order:

1. `cargo +nightly test -p rust_thermo_wgpu`
2. `cargo +nightly build -p rust_thermo_wgpu`
3. `cargo +nightly fmt`
4. `cargo +nightly clippy -- -D warnings`

For physics-affecting changes, add an energy-drift check. A pure observer/export path should not require physics changes.

## Risks

- CLI selection logic could accidentally change default startup behavior
- snapshot files may become large quickly if full ball state is emitted every step
- deterministic seeding must avoid overlapping balls if it bypasses normal collision checks
- there may be pressure to grow the mode into a scenario runner; keep the first scope narrow

## Open Questions

- Should the file sink contain full ball detail on every emitted frame or support a summary-only mode?
- Should headless mode live inside `wgpu_renderer`, or should a separate binary crate own non-graphical execution later?
- Do we want a stable machine-readable schema version in the JSON output from the start?
- Should CI eventually use headless mode for regression fixtures or snapshot-style tests?

## Recommended Implementation Order

1. Add doc-approved CLI contract and startup selection
2. Add direct `MainContent` stepping loop
3. Add console summary sink
4. Add JSONL file sink
5. Add deterministic seeding
6. Add focused tests and final lint/format validation
