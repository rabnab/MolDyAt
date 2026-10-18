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
| `migrate.rs` | `parse()` (version first, fast path for current files), the `MIGRATIONS` chain and `take_field` for writing migrations |
| `convert.rs` | live world ⇄ file (`WorldFile::capture(world, time_step)`, `TryFrom<&WorldFile> for SimulationWorld`, `From` impls for each `*Data` struct) |
| `store.rs` | save directory, file names, atomic write (temp file, then rename), `latest()` |
| `mod.rs` | `MainContent::load_world`: resolve, read, validate, build, then swap in |
| `tests/` | `format.rs`, `roundtrip.rs`, `store.rs`, `reference_saves.rs` (one fixture per format version in `tests/fixtures/`) |

## Rules
- **The file has its own types.** Balls, source spheres and thermostats are written through dedicated `*Data` structs, not their runtime types. Keep it that way for new state: runtime-only fields (caches, neighbour lists, chain scratch) must not leak into the file.
- **Built-in species are part of the format.** Since v2 a ball stores only its species name (`species`, `ball_species`); mass, `σ` and `ε` come from `SpeciesTable::builtin()` at load time. Renaming a built-in species makes every save that uses it fail to load; changing its mass, `σ` or `ε` silently loads old saves with different physics. Either is a breaking change: bump `WORLD_FILE_VERSION` and add a migration, or add a new species instead of changing an existing one. `builtin_table_is_pinned` (`objects2d/ball_impl/species.rs`) pins the whole table; update it only together with one of those.
- **Shared types are part of the format.** `SimLineSegment`, `ValveSegment`, `WallChain` (and its `Joint`, tagged `kind`), `Rect` and `Vec2D` are serialized directly. Renaming, adding or removing one of their fields changes the save format, even if the change looks unrelated. Treat such an edit as a format change, or move the type behind a `*Data` struct first.
- **Validate before touching the world.** `from_json` runs `validate()`; `load_world` builds the new `SimulationWorld` completely before replacing the running one, so a bad file leaves the simulation untouched (`invalid_file_leaves_the_running_world_untouched`). Every new field gets a check in `validate()`: finite numbers, ranges matching the runtime limits (reuse the `MIN_*` / `MAX_*` constants), unique ids below `MAX_ID`, references that resolve (valve → wall, chain → pieces).
- **Ids continue.** After a load, operator id counters (`OperatorIdCounter::after`) and the ball factory continue after the highest restored id.
- **Index contract.** The loaded world rebuilds the cell grid and neighbour lists from scratch (`collect_neighbours_by_cell`); never restore stored indices.

## Changing the format
Loading (`migrate::parse`, #272) reads the version first: a current-version file is deserialized directly (the `Value` path is many times slower for large saves), an older one is parsed to `serde_json::Value`, upgraded through the `MIGRATIONS` chain, logged, then deserialized. Both are validated. Files without a version, version 0 or newer than the build are rejected.
1. Decide whether the change is compatible (#271).
   - **Additive, keep the version, no migration:** a new field gets `#[serde(default)]` or `#[serde(default = "fn_name")]` with a default that reproduces the behaviour of older builds (the value that used to be hard-coded); a new top-level list, such as a new operator kind, is `#[serde(default)] Vec<…>`. `validate()` must accept the default. This applies to every saved type, including the shared ones below.
   - **Breaking, bump the version and add a migration:** renaming or removing a field, changing its type, unit or meaning, or changing a default that older files rely on. `#[serde(default)]` would load such files silently with wrong values.
   - `old_saves_load_into_a_file_type_with_new_fields` (`tests/reference_saves.rs`) shows the additive mechanism on every reference save; a real new field gets its own check that an older fixture loads with the default.
2. For an incompatible change: bump `WORLD_FILE_VERSION` to `N + 1` and add `migrate_vN_to_vN+1(&mut Value)` to `MIGRATIONS` in `persistence/migrate.rs` (the chain test checks that entry `i` upgrades version `i + 1`). A migration must not panic on unexpected input: use `take_field` and return `invalid(..)` for missing or mistyped fields, never `unwrap` or index. Remove the `expect(dead_code)` on `take_field` with the first real migration. Saving always writes the newest version.
3. Add the new version's reference save to `persistence/tests/fixtures/` (written by the new build) with its checks in `FIXTURES` (`tests/reference_saves.rs`). Existing fixtures are never edited or regenerated; they must keep loading through the migration.
4. Example: v1 → v2 (#234, `migrate_v1_to_v2`) turned each ball's `size_idx` and each source sphere's `ball_size_idx` into the name of a built-in species (`species`, `ball_species`); the size class species keep the old equilibrium distances.

## Tests and checks
- Round trip: save, load, compare (`world_survives_a_save_and_load`); add the new state to `busy_world()` in `tests/roundtrip.rs` so it is covered.
- Validation: one test per rejected case (non-finite, out of range, duplicate id, dangling reference).
- Every reference save loads: `cargo test -p moldyat_core reference_saves` (#269).
- `cargo test -p moldyat_core persistence`, then `cargo fmt` and `cargo clippy --workspace -- -D warnings`.
- `scripts/headless_perf.sh` caches worlds saved with `--save-setup` in `target/perf-worlds/`; after a format change, delete that cache.
- Update `README.md` (save/load section) if user-visible behaviour changes.
