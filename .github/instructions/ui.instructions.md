---
applyTo: "crates/thermo_ui/**,wgpu_renderer/**"
---

# UI and rendering rules

Rules for code in `thermo_ui` and `wgpu_renderer`. Procedures are in the `operator-framework` and `render-pipeline` skills.

- Coordinates: world space is the canonical simulation space (see `docs/simulation.md`). The renderer flips Y for pixel space; use the `ViewportTransform` helpers.
- Keyboard commands: one `CommandDescriptor` per command in `COMMANDS` (`graphics/renderer/event_loop/command_registry.rs`; the domain half is `DOMAIN_COMMANDS` in `thermo_runtime`'s `application::command_table`) holds its default bindings and its UI-vs-domain routing, and the key dispatch and the help text (`H`) are generated from them; add an entry plus its handler (a `UiAction` arm or a `DomainCommand` variant), never a key `match`. Bind every key that types a character (letters, digits, `+`, `-`, the numpad) by that character (`BindKey::Symbol`, `symbol('s', …)`), so shortcuts sit where they are printed on every layout; only keys that type none (SPACE, ESC, TAB, DEL, BACKSPACE, Shift, F-keys) by physical `KeyCode` (#327). The bindings in effect are a `KeyMap`: the defaults plus the `[key_bindings]` of a UI settings file (`ui.toml` / `--ui-config`, #335), a UI setting that never goes into `SimConfig` or saves. Ids (`sim.toggle_pause`) are stable. A test keeps the README key table in sync. See `docs/architecture.md#keyboard-commands`.
- Operator tools (line segment, valve, thermostat, source sphere): one `TOOLS` list (`crates/thermo_ui/src/graphics/sidebar.rs`) sets the icon order and drives the hit-testing and the wgpu icon atlas. See the `operator-framework` skill before adding, renaming or reordering tools.
