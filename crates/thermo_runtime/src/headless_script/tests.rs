// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

fn error_of(text: &str) -> ScriptError {
    Script::parse(text).expect_err("script should not parse")
}

#[test]
fn test_parse_reads_steps_commands_comments_and_semicolons() {
    let script = Script::parse(
        "# setup\n\
         operator.place_thermostat -8 0.5\n\
         \n\
         step 10   # first part\n\
         balls.add_random 500; sim.cycle_gravity\n\
         step 5\n",
    )
    .unwrap();
    let texts: Vec<&str> = script.items.iter().map(|item| item.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "operator.place_thermostat -8 0.5",
            "step 10",
            "balls.add_random 500",
            "sim.cycle_gravity",
            "step 5"
        ]
    );
    let lines: Vec<usize> = script.items.iter().map(|item| item.line).collect();
    assert_eq!(lines, [2, 4, 5, 5, 6]);
    assert_eq!(script.total_steps(), 15);
    assert_eq!(
        script.items[2].action,
        ScriptAction::Domain {
            id: "balls.add_random",
            arg: Some(500)
        }
    );
    assert_eq!(
        script.items[0].action,
        ScriptAction::Operator {
            id: "operator.place_thermostat",
            action: OperatorAction::PlaceThermostat { x: -8.0, y: 0.5 }
        }
    );
}

#[test]
fn test_schedule_puts_each_command_after_the_steps_before_it() {
    let script = Script::parse("sim.cycle_gravity\nstep 3\nstep 4\nworld.save\nstep 1").unwrap();
    let scheduled = script.schedule();
    let schedule: Vec<(u32, &str)> = scheduled
        .iter()
        .map(|scheduled| (scheduled.at_step, scheduled.item.text.as_str()))
        .collect();
    assert_eq!(schedule, [(0, "sim.cycle_gravity"), (7, "world.save")]);
    assert_eq!(script.total_steps(), 8);
}

#[test]
fn test_parse_rejects_unknown_and_non_scriptable_ids_with_their_line() {
    let error = error_of("step 1\nballs.explode");
    assert_eq!(error.line, 2);
    assert!(
        error.message.contains("unknown command `balls.explode`"),
        "{error}"
    );

    for id in [
        "sim.toggle_pause",
        "sim.toggle_stepwise",
        "sim.decrease_wait_time",
        "sim.increase_wait_time",
        "world.load_latest",
        "world.toggle_sample_stream_csv",
        "view.toggle_real_data",
        "debug.toggle_timings",
    ] {
        let error = error_of(id);
        assert_eq!(error.line, 1);
        assert!(
            error.message.contains("cannot run in a script"),
            "{id}: {error}"
        );
    }
}

#[test]
fn test_parse_rejects_bad_arguments() {
    for (text, expected) in [
        ("step", "`step` takes one number"),
        ("step -5", "whole number"),
        ("step 1 2", "`step` takes one number"),
        ("balls.add_random lots", "must be a whole number"),
        ("balls.add_random 2000000", "at most 1000000"),
        ("balls.add_random 1 2", "takes one count"),
        ("sim.cycle_gravity 3", "takes no arguments"),
        ("operator.place_thermostat 1", "wrong number of arguments"),
        ("operator.place_thermostat 1 nan", "not a finite"),
        ("operator.toggle_thermostat -1", "not a valid index"),
        (
            "operator.adjust_source_sphere_period 0 1.5",
            "not a valid whole number",
        ),
    ] {
        let error = error_of(text);
        assert!(error.message.contains(expected), "{text}: {error}");
    }
}

#[test]
fn test_parse_rejects_steps_that_overflow() {
    let error = error_of("step 4294967295\nstep 1");
    assert_eq!(error.line, 2);
}

#[test]
fn test_error_display_names_the_line() {
    assert_eq!(
        error_of("\n\nnope").to_string(),
        "line 3: unknown command `nope`: use `step N`, a registry id such as `balls.add_random` or an `operator.*` command"
    );
}

#[test]
fn test_command_record_splits_id_and_args() {
    let script = Script::parse("operator.place_source_sphere 1 -2").unwrap();
    let record = CommandRecord::of(&script.items[0], 7);
    assert_eq!(record.step, 7);
    assert_eq!(record.line, 1);
    assert_eq!(record.id, "operator.place_source_sphere");
    assert_eq!(record.args, ["1", "-2"]);
}
