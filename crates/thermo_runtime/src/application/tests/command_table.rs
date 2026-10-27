// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The domain command table (#340).

use crate::application::{BallCommand, DOMAIN_COMMANDS, DomainCommand, domain_command_spec};

#[test]
fn ids_are_unique_and_namespaced() {
    let mut ids: Vec<&str> = DOMAIN_COMMANDS.iter().map(|spec| spec.id).collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "duplicate id");
    for id in ids {
        let (category, name) = id.split_once('.').expect("<category>.<name>");
        assert!(!category.is_empty() && !name.is_empty(), "{id}");
    }
}

#[test]
fn scripts_get_simulation_balls_save_and_export_only() {
    let scripted: Vec<&str> = DOMAIN_COMMANDS
        .iter()
        .filter(|spec| spec.in_scripts)
        .map(|spec| spec.id)
        .collect();
    for id in &scripted {
        assert!(
            id.starts_with("sim.")
                || id.starts_with("balls.")
                || *id == "world.save"
                || id.starts_with("world.export_"),
            "{id} must not be scriptable"
        );
    }
    for id in [
        "world.load_latest",
        "world.toggle_sample_stream_csv",
        "view.toggle_real_data",
    ] {
        assert!(!domain_command_spec(id).unwrap().in_scripts, "{id}");
    }
    assert!(scripted.contains(&"balls.add_random"));
}

#[test]
fn a_parameter_falls_back_to_its_default() {
    let spec = domain_command_spec("balls.add_random").unwrap();
    assert!(matches!(
        spec.build(None),
        DomainCommand::Balls(BallCommand::AddRandom(100))
    ));
    assert!(matches!(
        spec.build(Some(7)),
        DomainCommand::Balls(BallCommand::AddRandom(7))
    ));
    assert!(
        domain_command_spec("sim.cycle_gravity")
            .unwrap()
            .param
            .is_none()
    );
    assert!(domain_command_spec("nope").is_none());
}
