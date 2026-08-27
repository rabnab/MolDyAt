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
