use super::*;

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
