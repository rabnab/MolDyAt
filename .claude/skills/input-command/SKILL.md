---
name: input-command
description: Checklist for adding, changing or rebinding a keyboard command or a non-operator DomainCommand (pause, gravity, time step, ball commands, save/load, view toggles). Use before editing input_handler.rs, action_processor.rs, domain_command_mapping.rs, domain_command.rs, the physics command handler, or the help text. Temporary - delete this skill once plan item C2 (command registry) lands.
---

# Input Command Checklist

A key command currently touches up to eight places. Plan item C2 (`docs/plan/refactoring-plan.md`) will generate them from one registry; until then, follow this list. Operator tool input (mouse on walls, valves, thermostats, sources) is different: see the `operator-framework` skill.

## Flow
```text
KeyCode → InputHandler::handle_key_down → InputAction
        → process_action_common → DomainCommand::try_from(action)
              ├─ Ok(command)  → PhysicsCoordinator::send_command → DomainCommandHandler::handle_<group>
              └─ Err(action)  → UI-only handling (viewport, renderer config, help, energy window)
```

## Places to change
All under `core/src/` unless noted.

| # | Where | What |
|---|---|---|
| 1 | `graphics/renderer/event_loop/input_handler.rs` | Add the `InputAction` variant. Bind the key in `handle_key_down` (Ctrl chords are matched first, at the top). Check the key is free: `KeyCode::...` arms in the same function. |
| 2 | `application/domain_command.rs` | If it changes the simulation: add a variant to the right group, `SimControl`, `BallCommand`, `WorldCommand` or `ViewCommand` (no physics effect). Do not add top-level `DomainCommand` variants. |
| 3 | `graphics/renderer/event_loop/domain_command_mapping.rs` | Map the action to the command in `TryFrom<InputAction> for DomainCommand`. |
| 4 | `application/physics_thread/command_handler.rs` | Handle the variant in `handle_sim`, `handle_balls`, `handle_world` or `handle_view`. The logic itself goes into `MainContent` / `SimulationWorld` methods, not into the handler. |
| 5 | `graphics/renderer/event_loop/action_processor.rs` | `process_action_common` matches exhaustively: add the action either to a UI-only arm (if it never becomes a `DomainCommand`) or to the list of actions handled elsewhere / mapped to commands. |
| 6 | `wgpu_renderer/src/wgpu/wgpu_input.rs` | Only if the action targets a render pipeline (like `ToggleCutoffRings`). |
| 7 | `application/log_util.rs` | Add a line to `print_help_message` (key `H`). |
| 8 | `README.md` | Add the key to the keyboard table. |

Commands that report a result to the UI (like save/load) send a `PhysicsEvent` back; add a variant in `application/physics_event.rs` and handle it where the renderer polls `try_recv_event`.

## Rules
- The renderer never mutates simulation state: anything that changes the world goes through a `DomainCommand`.
- Ctrl chords act on key press and must ignore auto-repeat (`is_repeat`); see the Ctrl+S / Ctrl+O handling.
- A view-only toggle that changes which snapshot data is needed sends `ViewCommand::SetSnapshotLayers` (see `ToggleRendering`, `CycleThermoDisplay`).
- Keep commands deterministic: a command must not depend on wall-clock time or thread timing, so headless and seeded runs stay reproducible.

## Tests
In `graphics/renderer/event_loop/tests/`:
- `key_actions.rs`: key → `InputAction` (including modifiers and repeat).
- `domain_command.rs`: action → `DomainCommand` mapping, and that UI-only actions are not sent.
- A test for the command's effect on `MainContent` in `application/core/tests/` or `application/tests/`.

Then `cargo test -p moldyat_core`, `cargo fmt`, `cargo clippy --workspace -- -D warnings`.
