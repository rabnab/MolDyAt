// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use std::env;

#[test]
fn test_default_config_values() {
    let config = HeadlessConfig::default();
    assert_eq!(config.steps, 1);
    assert_eq!(config.snapshot_every, 0);
    assert_eq!(config.output, None);
    assert_eq!(config.format, OutputFormat::Jsonl);
    assert_eq!(config.detail, DetailLevel::Summary);
    assert_eq!(config.rng_seed, None);
    assert!(config.stdout);
    assert_eq!(config.seed_balls, None);
    assert_eq!(config.dt, None);
    assert!(config.line_segments.is_empty());
    assert_eq!(config.world_scale, None);
    assert_eq!(config.seed_speed, None);
}

#[test]
fn test_no_headless_flag() {
    let args = vec!["program".to_string()];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_headless_without_flags() {
    let args = vec!["program".to_string(), "--headless".to_string()];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.steps, 1);
    assert!(config.stdout);
}

#[test]
fn test_steps_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--steps".to_string(),
        "100".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.steps, 100);
}

#[test]
fn test_steps_zero_is_error() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--steps".to_string(),
        "0".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_steps_invalid_value() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--steps".to_string(),
        "not_a_number".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_steps_missing_value() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--steps".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_snapshot_every_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--snapshot-every".to_string(),
        "50".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.snapshot_every, 50);
}

#[test]
fn test_snapshot_every_zero() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--snapshot-every".to_string(),
        "0".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.snapshot_every, 0);
}

#[test]
fn test_output_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--output".to_string(),
        "/tmp/output.jsonl".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.output.unwrap(), env::temp_dir().join("output.jsonl"));
}

#[test]
fn test_output_missing_value() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--output".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_format_jsonl() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--format".to_string(),
        "jsonl".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.format, OutputFormat::Jsonl);
}

#[test]
fn test_format_invalid() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--format".to_string(),
        "csv".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_detail_summary() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--detail".to_string(),
        "summary".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.detail, DetailLevel::Summary);
}

#[test]
fn test_detail_balls() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--detail".to_string(),
        "balls".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.detail, DetailLevel::Balls);
}

#[test]
fn test_detail_thermo() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--detail".to_string(),
        "thermo".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.detail, DetailLevel::Thermo);
}

#[test]
fn test_detail_balls_and_thermo() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--detail".to_string(),
        "balls+thermo".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.detail, DetailLevel::BallsAndThermo);
}

#[test]
fn test_detail_invalid() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--detail".to_string(),
        "operators".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_rng_seed_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--rng-seed".to_string(),
        "42".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.rng_seed, Some(42));
}

#[test]
fn test_rng_seed_invalid() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--rng-seed".to_string(),
        "not_a_number".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_stdout_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--stdout".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert!(config.stdout);
}

#[test]
fn test_no_stdout_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--no-stdout".to_string(),
        "--output".to_string(),
        "/tmp/output.jsonl".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert!(!config.stdout);
}

#[test]
fn test_no_stdout_without_output_is_error() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--no-stdout".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_seed_balls_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--seed-balls".to_string(),
        "32".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.seed_balls, Some(32));
}

#[test]
fn test_seed_balls_zero_is_error() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--seed-balls".to_string(),
        "0".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_unknown_flag() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--unknown-flag".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

#[test]
fn test_multiple_flags_combined() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--steps".to_string(),
        "200".to_string(),
        "--snapshot-every".to_string(),
        "20".to_string(),
        "--output".to_string(),
        "/tmp/out.jsonl".to_string(),
        "--format".to_string(),
        "jsonl".to_string(),
        "--detail".to_string(),
        "balls".to_string(),
        "--rng-seed".to_string(),
        "123".to_string(),
        "--seed-balls".to_string(),
        "16".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_ok());
    let config = result.unwrap().unwrap();
    assert_eq!(config.steps, 200);
    assert_eq!(config.snapshot_every, 20);
    assert_eq!(config.output.unwrap(), env::temp_dir().join("out.jsonl"));
    assert_eq!(config.format, OutputFormat::Jsonl);
    assert_eq!(config.detail, DetailLevel::Balls);
    assert_eq!(config.rng_seed, Some(123));
    assert!(config.stdout);
    assert_eq!(config.seed_balls, Some(16));
}

#[test]
fn test_stdout_and_no_stdout_conflict_is_error() {
    let args = vec![
        "program".to_string(),
        "--headless".to_string(),
        "--stdout".to_string(),
        "--no-stdout".to_string(),
    ];
    let result = parse_headless_args_with_args(&args);
    assert!(result.is_err());
}

fn headless_args(extra: &[&str]) -> Vec<String> {
    ["program", "--headless"]
        .iter()
        .chain(extra)
        .map(|arg| arg.to_string())
        .collect()
}

#[test]
fn test_dt_flag() {
    let config = parse_headless_args_with_args(&headless_args(&["--dt", "0.005"]))
        .unwrap()
        .unwrap();
    assert_eq!(config.dt, Some(0.005));
}

#[test]
fn test_dt_flag_rejects_non_positive_and_invalid_values() {
    for value in ["0", "-0.01", "abc", "NaN", "inf"] {
        let result = parse_headless_args_with_args(&headless_args(&["--dt", value]));
        assert!(result.is_err(), "--dt {value} should be rejected");
    }
    assert!(parse_headless_args_with_args(&headless_args(&["--dt"])).is_err());
}

#[test]
fn test_line_segment_flag_is_repeatable_with_optional_thickness() {
    let config = parse_headless_args_with_args(&headless_args(&[
        "--line-segment",
        "-20,-10,20,-10",
        "--line-segment",
        "0, 5, 0, 25, 3.5",
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(
        config.line_segments,
        vec![
            LineSegmentSpec {
                start: (-20.0, -10.0),
                end: (20.0, -10.0),
                thickness: 1.0,
            },
            LineSegmentSpec {
                start: (0.0, 5.0),
                end: (0.0, 25.0),
                thickness: 3.5,
            },
        ]
    );
}

#[test]
fn test_line_segment_flag_rejects_malformed_values() {
    for value in [
        "1,2,3",
        "1,2,3,4,5,6",
        "1,2,x,4",
        "1,2,1,2",
        "0,0,1,1,0.5",
        "0,0,1,1,21",
        "0,0,inf,1",
    ] {
        let result = parse_headless_args_with_args(&headless_args(&["--line-segment", value]));
        assert!(result.is_err(), "--line-segment {value} should be rejected");
    }
    assert!(parse_headless_args_with_args(&headless_args(&["--line-segment"])).is_err());
}

#[test]
fn test_world_scale_flag() {
    let parse = |value: &str| {
        let args = ["program", "--headless", "--world-scale", value].map(String::from);
        parse_headless_args_with_args(&args)
    };
    assert_eq!(parse("5").unwrap().unwrap().world_scale, Some(5.0));
    for bad in ["0", "-2", "nan", "inf", "x"] {
        assert!(parse(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn test_seed_speed_flag() {
    let parse = |value: &str| {
        let args = ["program", "--headless", "--seed-speed", value].map(String::from);
        parse_headless_args_with_args(&args)
    };
    assert_eq!(parse("3.5").unwrap().unwrap().seed_speed, Some(3.5));
    assert_eq!(parse("0").unwrap().unwrap().seed_speed, Some(0.0));
    for bad in ["-1", "nan", "inf", "x"] {
        assert!(parse(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn test_valve_flag_refers_to_a_line_segment() {
    let config = parse_headless_args_with_args(&headless_args(&[
        "--line-segment",
        "-20,0,20,0",
        "--line-segment",
        "0,5,0,25",
        "--valve",
        "1,0.25,0.75,closed",
        "--valve",
        "0, 0.4, 0.6",
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(
        config.valves,
        vec![
            ValveSpec {
                wall: 1,
                t_start: 0.25,
                t_end: 0.75,
                is_closed: true,
            },
            ValveSpec {
                wall: 0,
                t_start: 0.4,
                t_end: 0.6,
                is_closed: false,
            },
        ]
    );
}

#[test]
fn test_valve_flag_rejects_malformed_values() {
    for value in [
        "0,0.2",
        "0,0.2,0.4,open,x",
        "x,0.2,0.4",
        "0,0.4,0.2",
        "0,-0.1,0.4",
        "0,0.2,1.5",
        "0,0.2,0.4,ajar",
        "1,0.2,0.4",
        // Placement would reject these: flush with only one wall end, too narrow.
        "0,0,0.5",
        "0,0.5,1",
        "0,0.2,0.20005",
    ] {
        let args = headless_args(&["--line-segment", "-20,0,20,0", "--valve", value]);
        assert!(
            parse_headless_args_with_args(&args).is_err(),
            "--valve {value} should be rejected"
        );
    }
    let twice = headless_args(&[
        "--line-segment",
        "-20,0,20,0",
        "--valve",
        "0,0.1,0.2",
        "--valve",
        "0,0.6,0.7",
    ]);
    assert!(parse_headless_args_with_args(&twice).is_err());
}

#[test]
fn test_timings_flags() {
    let config = parse_headless_args_with_args(&headless_args(&[
        "--steps",
        "50",
        "--timings",
        "/tmp/t.json",
        "--timing-warmup",
        "10",
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(config.timings, Some(PathBuf::from("/tmp/t.json")));
    assert_eq!(config.timing_warmup, 10);

    let warmup_too_long = headless_args(&["--steps", "10", "--timing-warmup", "10"]);
    assert!(parse_headless_args_with_args(&warmup_too_long).is_err());
    assert!(parse_headless_args_with_args(&headless_args(&["--timings"])).is_err());
}

#[test]
fn test_save_setup_flag() {
    assert!(!HeadlessConfig::default().save_setup);
    let config = parse_headless_args_with_args(&headless_args(&["--save-setup"]))
        .unwrap()
        .unwrap();
    assert!(config.save_setup);
}

#[test]
fn test_valve_flag_accepts_a_valve_spanning_the_whole_wall() {
    let args = headless_args(&["--line-segment", "-20,0,20,0", "--valve", "0,0,1"]);
    let config = parse_headless_args_with_args(&args).unwrap().unwrap();
    assert_eq!(config.valves[0].t_start, 0.0);
    assert_eq!(config.valves[0].t_end, 1.0);
}
