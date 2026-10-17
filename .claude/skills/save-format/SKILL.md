---
name: save-format
description: Contract for changing what is saved and loaded - the WorldFile JSON format, its validation and version, conversion between the live world and the file, and migrating old saves. Use before editing core/src/application/persistence/, adding persistent state to SimulationWorld or an operator, or changing the fields of any type that is serialized into the save file (SimLineSegment, ValveSegment, WallChain, Rect, Vec2D).
---

# Save Format Contract

Saves are pretty-printed JSON files written by `SaveStore` (`core/src/application/persistence/`): Ctrl+S writes `world_YYYYMMDD_HHMMSS.json` into the save directory (`--save-dir`, default `saves/`), Ctrl+O and `--load` read them, `--save-setup` writes the prepared world of a headless run. Plan decision: **old saves keep loading** (`docs/plan/refactoring-plan.md` §6).

## Files
| File | Role |
|---|---|
| `format.rs` | `WorldFile`, its data structs (`BallData`, `SourceSphereData`, `ThermostatData`), `WORLD_FILE_VERSION`, `validate()`, `to_json` / `from_json` |
| `convert.rs` | live world ⇄ file (`WorldFile::capture(world, time_step)`, `TryFrom<&WorldFile> for SimulationWorld`, `From` impls for each `*Data` struct) |
| `store.rs` | save directory, file names, atomic write (temp file, then rename), `latest()` |
| `mod.rs` | `MainContent::load_world`: resolve, read, validate, build, then swap in |
| `tests/` | `format.rs`, `roundtrip.rs`, `store.rs`, `reference_saves.rs` (one fixture per format version in `tests/fixtures/`) |

## Rules
- **The file has its own types.** Balls, source spheres and thermostats are written through dedicated `*Data` structs, not their runtime types. Keep it that way for new state: runtime-only fields (caches, neighbour lists, chain scratch) must not leak into the file.
- **Shared types are part of the format.** `SimLineSegment`, `ValveSegment`, `WallChain` (and its `Joint`, tagged `kind`), `Rect` and `Vec2D` are serialized directly. Renaming, adding or removing one of their fields changes the save format, even if the change looks unrelated. Treat such an edit as a format change, or move the type behind a `*Data` struct first.
- **Validate before touching the world.** `from_json` runs `validate()`; `load_world` builds the new `SimulationWorld` completely before replacing the running one, so a bad file leaves the simulation untouched (`invalid_file_leaves_the_running_world_untouched`). Every new field gets a check in `validate()`: finite numbers, ranges matching the runtime limits (reuse the `MIN_*` / `MAX_*` constants), unique ids below `MAX_ID`, references that resolve (valve → wall, chain → pieces).
- **Ids continue.** After a load, operator id counters (`OperatorIdCounter::after`) and the ball factory continue after the highest restored id.
- **Index contract.** The loaded world rebuilds the cell grid and neighbour lists from scratch (`collect_neighbours_by_cell`); never restore stored indices.

## Changing the format
Today the loader accepts only `version == WORLD_FILE_VERSION` (1) and has no migration path. The first incompatible change must add one:
1. Decide whether the change is compatible. Adding an optional field with `#[serde(default)]` whose default reproduces the old behaviour is compatible and keeps the version. Renaming, removing, changing type, unit or meaning is not.
2. For an incompatible change: bump `WORLD_FILE_VERSION`, and make loading accept every older version by upgrading it step by step (`v1 → v2 → …`) before `validate()`, e.g. by reading the version first and deserializing the matching old struct or patching the JSON value. Saving always writes the newest version.
3. Add the new version's reference save to `persistence/tests/fixtures/` (written by the new build) with its checks in `FIXTURES` (`tests/reference_saves.rs`). Existing fixtures are never edited or regenerated; they must keep loading through the migration.
4. Plan item I3 (species replace size classes) is the first planned migration: `size_idx` becomes a species, with σ chosen so the old equilibrium distance is kept.

## Tests and checks
- Round trip: save, load, compare (`world_survives_a_save_and_load`); add the new state to `busy_world()` in `tests/roundtrip.rs` so it is covered.
- Validation: one test per rejected case (non-finite, out of range, duplicate id, dangling reference).
- Every reference save loads: `cargo test -p moldyat_core reference_saves` (#269).
- `cargo test -p moldyat_core persistence`, then `cargo fmt` and `cargo clippy --workspace -- -D warnings`.
- `scripts/headless_perf.sh` caches worlds saved with `--save-setup` in `target/perf-worlds/`; after a format change, delete that cache.
- Update `README.md` (save/load section) if user-visible behaviour changes.
