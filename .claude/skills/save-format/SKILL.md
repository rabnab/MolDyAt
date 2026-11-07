---
name: save-format
description: Contract for changing what is saved and loaded - the WorldFile JSON format, its validation and version, conversion between the live world and the file, and migrating old saves. Use before editing crates/thermo_physics/src/application/persistence/, adding persistent state to SimulationWorld or an operator, or changing the fields of a file type (`*Data` in persistence/format.rs) or of the two value types written as they are (Rect, Vec2D).
---

# Save Format Contract

Saves are pretty-printed JSON written by `SaveStore` (`crates/thermo_physics/src/application/persistence/`; Ctrl+S, Ctrl+O, `--load`, `--save-setup`). Plan decision: **old saves keep loading** (`docs/plan/refactoring-plan.md` §6).

## Files
| File | Role |
|---|---|
| `format.rs` | `WorldFile` (with `units` and `config`), its data structs (`BallData`, and in the `operators` list `OperatorData`: `WallChainData` with `ChainPieceData` / `JointData`, `SourceSphereData`, `ThermostatData`), `WORLD_FILE_VERSION`, `validate()`, `to_json` / `from_json` |
| `migrate.rs` | `parse()` (version and units first, fast path for current files in `UnitSystem::REDUCED`), the `MIGRATIONS` chain, `rescale` and `take_field` for writing migrations |
| `convert.rs` | live world ⇄ file (`WorldFile::capture(&MainContent)`, `TryFrom<&WorldFile> for SimulationWorld`, `From` impls for each `*Data` struct) |
| `store.rs` | save directory, file names, atomic write (temp file, then rename), `latest()` |
| `mod.rs` | `MainContent::load_world`: resolve, read, validate, build, then swap in |
| `tests/` | `format.rs`, `migrate.rs`, `newer_files.rs`, `roundtrip.rs`, `store.rs`, `reference_saves.rs` (one fixture per format version in `tests/fixtures/`) |

## Rules
- **Save only what a load cannot rebuild.** State derivable from saved state is recomputed on load, not written; new state that fits an existing `*Data` field extends it instead of adding one.
- **The file has its own types.** Every saved object is written through a dedicated `*Data` struct, not its runtime type (balls, walls, valves, wall chains, source spheres, thermostats; #270).
- **One operator list (#274, v3).** `WorldFile::operators` holds every operator as `{"<kind>": {...}}` (`OperatorData`, exactly one key per entry). A wall chain owns its pieces and valve joints; segments and valves have no lists of their own, so only piece, valve and chain ids need to be unique. An entry of an unknown kind is skipped and reported as `operators[i].<kind>`. The simulation types (`SimLineSegment`, `ValveSegment`, `WallChain`, `Joint`, ...) do not derive serde; their conversion lives in `convert.rs`. Keep it that way for new state: runtime-only fields (caches, neighbour lists, chain scratch) must not leak into the file.
- **A save holds its species table (#426).** A ball stores only its species name (`species`, `ball_species`); mass, `σ`, `ε` and the model come from `config.species`, which every save lists in full (`capture` writes out the built-in table if the run had no list; `validate()` rejects an empty one). Conversion (`convert.rs`) looks names up in the world's table (`SimulationWorld::species()`), never in `builtin()`, and `load_world` hands the loaded table to the ball factory (`BallFactory::use_species`). The built-in library only creates new worlds, so changing it needs no migration; `builtin_table_names_are_pinned` pins only its names and order. A new species model needs `SpeciesConfig` keys (`resolve`, and `From<&Species>`, which resolves back bit for bit: `builtin_species_survive_the_literature_form`). v2 to v8 files get the frozen table in `persistence/v8_species.json` (never edit it).
- **A save states its units (#426).** `WorldFile::units` (`UnitSystem`) is the unit system of every reduced value; `migrate::parse` converts a file in other units with `rescale` (the time step and factors keep their numbers; `[[species]]` is in Å, K, u). A new reduced field gets its factor in `rescale` / `scale_config`. Constants and defaults with a unit are defined in physical units and converted with `UnitSystem::REDUCED`, so a default a save leaves out means the same physics in any units.
- **The simulation config is part of the format.** `WorldFile::config` (`SimConfig`, required since v9) is written by every save and restored on load; files of v1 to v8 without one get it in the v8 → v9 migration (the defaults with hard walls). So its key names, meanings and defaults are format: changing a default is a breaking change (bump the version and write the old value into those files in a migration). A new key gets a default equal to today's behaviour and a range (`MIN_*` / `MAX_*` in `sim_config/mod.rs`). `SimConfig` rejects unknown keys (`deny_unknown_fields`), so older builds of the same format version cannot read saves with a new key; that is intended (#273), since a dropped key would change the physics. An optional list key that is `skip_serializing_if = "Vec::is_empty"` (like `potential.pair_override`) keeps saves that do not use it readable. The top-level `time_step` must lie in `MIN_TIME_STEP..=MAX_TIME_STEP` and equal `config.integration.time_step` (`validate()`).
- **Three value types are part of the format.** `Vec2D` (`{x, y}`), `Rect` (`{x, y, w, h}`) and `UnitSystem` (`{sigma_angstrom, epsilon_kelvin, mass_u}`) are written as they are: their layout is stable and every file type uses them. Renaming, adding or removing one of their fields changes the save format, even if the change looks unrelated; treat it as a format change. `wall_layout_is_unchanged_by_the_file_types` (`tests/reference_saves.rs`) checks that the walls' JSON in the current reference save survives a load and a save unchanged.
- **Validate before touching the world.** `from_json` runs `validate()`; `load_world` builds the new `SimulationWorld` completely before replacing the running one, so a bad file leaves the simulation untouched (`invalid_file_leaves_the_running_world_untouched`). Every new field gets a check in `validate()`: finite numbers, ranges matching the runtime limits (reuse the `MIN_*` / `MAX_*` constants), unique ids below `MAX_ID`, references that resolve (valve → wall, chain → pieces).
- **Ids continue.** After a load, operator id counters (`OperatorIdCounter::after`) and the ball factory continue after the highest restored id.
- **Index contract.** The loaded world rebuilds the cell grid and neighbour lists from scratch (`collect_neighbours_by_cell`); never restore stored indices.

## Changing the format
Loading (`migrate::parse`, #272) reads the version first: a current-version file is deserialized directly (the `Value` path is many times slower for large saves), an older one is parsed to `serde_json::Value`, upgraded through the `MIGRATIONS` chain, logged, then deserialized. Both are validated. Files without a version, version 0 or newer than the build are rejected.

**Newer files (#273).** A file from a newer build:
- with a newer version: rejected ("version N is newer than this build supports (up to M)");
- with unknown keys in `config`: rejected, with the "probably written by a newer MolDyAt build" hint;
- with unknown fields anywhere else: loaded. `migrate::parse` collects the skipped paths through `serde_ignored`; `load_world` logs them as one warning and returns them in `LoadedWorld::ignored_fields`. Saving that world again drops them.

So **an addition that changes results bumps the version**, even when it is additive for older files (a new operator kind, a new physics field or config key): add the `#[serde(default)]` as in step 1 *and* a migration that only updates the number, so older builds refuse the file clearly instead of running it with different physics. Only additions that change no results (display, metadata) keep the version. `serde_ignored` cannot see into serde's internally tagged or flattened types, which buffer their fields: a file type must not use them (`JointData` reads its `kind` through a plain `RawJoint` struct, and `OperatorData` is keyed by its kind for this); `unknown_fields_outside_config_are_skipped_and_named` (`tests/newer_files.rs`) covers every nesting kind.
1. Decide whether the change is compatible (#271).
   - **Additive, no migration (and keep the version unless it changes results, see above):** a new field gets `#[serde(default)]` or `#[serde(default = "fn_name")]` with a default that reproduces the behaviour of older builds (the value that used to be hard-coded); a new top-level list is `#[serde(default)] Vec<…>`; a new operator kind is a new `OperatorData` variant (older files simply have none of it). `validate()` must accept the default. This applies to every saved type, including the shared ones below.
   - **Breaking, bump the version and add a migration:** renaming or removing a field, changing its type, unit or meaning, or changing a default that older files rely on. `#[serde(default)]` would load such files silently with wrong values.
   - `old_saves_load_into_a_file_type_with_new_fields` (`tests/reference_saves.rs`) shows the additive mechanism on every reference save; a real new field gets its own check that an older fixture loads with the default.
2. For an incompatible change: bump `WORLD_FILE_VERSION` to `N + 1` and add `migrate_vN_to_vN+1(&mut Value)` to `MIGRATIONS` in `persistence/migrate.rs` (the chain test checks that entry `i` upgrades version `i + 1`). A migration must not panic on unexpected input: use `take_field` and return `invalid(..)` for missing or mistyped fields, never `unwrap` or index. Remove the `expect(dead_code)` on `take_field` with the first real migration. Saving always writes the newest version.
3. Add the new version's reference save to `persistence/tests/fixtures/` (written by the new build) with its checks in `FIXTURES` (`tests/reference_saves.rs`). Existing fixtures are never edited or regenerated; they must keep loading through the migration.
4. Examples: `migrate_v1_to_v2` (#234) and `migrate_v2_to_v3` (#274) in `migrate.rs`.

## Tests and checks
- Round trip: save, load, compare (`world_survives_a_save_and_load`); add the new state to `busy_world()` in `tests/roundtrip.rs` so it is covered.
- Validation: one test per rejected case (non-finite, out of range, duplicate id, dangling reference).
- Every reference save loads: `cargo test -p thermo_physics reference_saves` (#269).
- `cargo test -p thermo_physics persistence`.
- `scripts/headless_perf.sh` caches worlds saved with `--save-setup` in `target/perf-worlds/`; after a format change, delete that cache.
- Update `README.md` (save/load section) if user-visible behaviour changes.
