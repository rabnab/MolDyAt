// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Registry commands → `DomainCommand`, and UI commands that stay in the
//! renderer.

use super::super::command_registry::{COMMANDS, UiAction, command_by_id};
use thermo_physics::application::persistence::LoadSource;
use thermo_runtime::application::{
    BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand,
};

fn domain(id: &str) -> DomainCommand {
    command_by_id(id)
        .unwrap_or_else(|| panic!("{id} is registered"))
        .domain_command()
        .unwrap_or_else(|| panic!("{id} is a domain command"))
}

#[test]
fn test_every_domain_command_maps_as_before() {
    let expected = [
        ("sim.toggle_pause", "Sim(TogglePause)"),
        ("sim.toggle_stepwise", "Sim(ToggleStepwise)"),
        ("sim.cycle_gravity", "Sim(ToggleGravity)"),
        ("sim.double_time_step", "Sim(DoubleTimeStep)"),
        ("sim.halve_time_step", "Sim(HalveTimeStep)"),
        ("sim.increase_wait_time", "Sim(IncreaseWaitTime)"),
        ("sim.decrease_wait_time", "Sim(DecreaseWaitTime)"),
        ("balls.clear", "Balls(Clear)"),
        ("balls.stop_all", "Balls(StopAll)"),
        ("balls.add_random", "Balls(AddRandom(100))"),
        ("balls.next_species", "Balls(NextSpecies)"),
        ("balls.previous_species", "Balls(PreviousSpecies)"),
        ("balls.randomize_speeds", "Balls(RandomizeSpeeds)"),
        ("balls.enable_random_speed", "Balls(EnableRandomSpeed)"),
        ("world.save", "World(Save)"),
        ("world.load_latest", "World(Load(Latest))"),
        (
            "world.export_series_csv",
            "World(Export { kind: Series, format: Csv })",
        ),
        (
            "world.export_series_jsonl",
            "World(Export { kind: Series, format: Jsonl })",
        ),
        (
            "world.export_snapshot_csv",
            "World(Export { kind: Snapshot, format: Csv })",
        ),
        (
            "world.export_snapshot_jsonl",
            "World(Export { kind: Snapshot, format: Jsonl })",
        ),
        (
            "world.export_distributions_csv",
            "World(Export { kind: Distributions, format: Csv })",
        ),
        (
            "world.export_distributions_jsonl",
            "World(Export { kind: Distributions, format: Jsonl })",
        ),
        (
            "world.toggle_sample_stream_csv",
            "World(ToggleSampleStream { format: Csv })",
        ),
        (
            "world.toggle_sample_stream_jsonl",
            "World(ToggleSampleStream { format: Jsonl })",
        ),
        ("view.toggle_real_data", "View(ToggleRealData)"),
        ("view.toggle_ball_stats", "View(ToggleBallStats)"),
        ("debug.toggle_timings", "View(ToggleTimingDisplay)"),
        ("debug.ball_verbosity", "View(StartVerbosityInput)"),
    ];
    for (id, command) in expected {
        assert_eq!(format!("{:?}", domain(id)), command, "{id}");
    }
    let domain_count = COMMANDS
        .iter()
        .filter(|c| c.domain_command().is_some())
        .count();
    assert_eq!(
        domain_count,
        expected.len(),
        "a domain command lacks a test row"
    );
}

#[test]
fn test_toggle_pause_maps_to_sim_control() {
    assert!(matches!(
        domain("sim.toggle_pause"),
        DomainCommand::Sim(SimControl::TogglePause)
    ));
}

#[test]
fn test_add_random_adds_100_balls() {
    assert!(matches!(
        domain("balls.add_random"),
        DomainCommand::Balls(BallCommand::AddRandom(100))
    ));
}

#[test]
fn test_load_latest_maps_to_world_command_with_latest_save() {
    assert!(matches!(
        domain("world.load_latest"),
        DomainCommand::World(WorldCommand::Load(LoadSource::Latest))
    ));
}

#[test]
fn test_toggle_ball_stats_maps_to_view_command() {
    assert!(matches!(
        domain("view.toggle_ball_stats"),
        DomainCommand::View(ViewCommand::ToggleBallStats)
    ));
}

#[test]
fn test_ui_commands_stay_outside_domain_command() {
    let expected = [
        ("help.show", UiAction::PrintHelp),
        ("view.reset", UiAction::ResetView),
        ("view.toggle_rendering", UiAction::ToggleRendering),
        ("view.cycle_cutoff_rings", UiAction::CycleCutoffRings),
        ("view.cycle_thermo_overlay", UiAction::CycleThermoDisplay),
        ("view.cycle_plot", UiAction::CyclePlotContent),
        ("view.toggle_energy_window", UiAction::ToggleEnergyWindow),
        ("tools.delete_hovered", UiAction::DeleteHovered),
    ];
    for (id, action) in expected {
        let command = command_by_id(id).unwrap_or_else(|| panic!("{id} is registered"));
        assert!(command.domain_command().is_none(), "{id}");
        assert_eq!(command.ui_action(), Some(action), "{id}");
    }
}

/// The key registry and the domain command table (`thermo_runtime`, #340)
/// name the same commands: every key's domain id has a table entry, and
/// every table entry has a key command.
#[test]
fn test_key_registry_and_domain_table_name_the_same_commands() {
    use super::super::command_registry::CommandKind;
    use thermo_runtime::application::{DOMAIN_COMMANDS, domain_command_spec};

    let key_ids: Vec<&str> = COMMANDS
        .iter()
        .filter_map(|c| match c.kind {
            CommandKind::Domain(id) => {
                assert_eq!(id, c.id, "a key command names its own id");
                assert!(domain_command_spec(id).is_some(), "{id} not in the table");
                Some(id)
            }
            CommandKind::Ui(_) => None,
        })
        .collect();
    for spec in DOMAIN_COMMANDS {
        assert!(key_ids.contains(&spec.id), "{} has no key command", spec.id);
    }
}
