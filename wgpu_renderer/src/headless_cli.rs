/// Headless mode CLI argument parsing and configuration.
///
/// This module handles parsing command-line arguments for headless execution mode
/// and storing the parsed configuration for use by the headless simulation loop.
use std::env;
use std::path::PathBuf;

/// Parsed headless CLI arguments and their default values.
#[derive(Debug, Clone)]
pub struct HeadlessConfig {
    /// Total number of simulation steps to run
    pub steps: u32,
    /// Emit snapshot every N steps (0 means only at step 0 and final)
    pub snapshot_every: u32,
    /// Output file path for snapshot records (None means no file output)
    pub output: Option<PathBuf>,
    /// Output format (currently only "jsonl" supported)
    pub format: OutputFormat,
    /// Detail level for snapshot records
    pub detail: DetailLevel,
    /// RNG seed for deterministic seeding
    pub rng_seed: Option<u64>,
    /// Enable stdout output
    pub stdout: bool,
    /// Number of balls to seed deterministically at startup
    pub seed_balls: Option<u32>,
}

/// Output format for headless snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// JSON Lines format (.jsonl)
    Jsonl,
}

/// Detail level for snapshot records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailLevel {
    /// Summary-only output (default)
    Summary,
    /// Include per-ball details (position, velocity, radius, size class, mass)
    Balls,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            steps: 1,
            snapshot_every: 0,
            output: None,
            format: OutputFormat::Jsonl,
            detail: DetailLevel::Summary,
            rng_seed: None,
            stdout: true,
            seed_balls: None,
        }
    }
}

/// Error type for CLI argument parsing.
#[derive(Debug, Clone)]
pub struct CliError {
    pub message: String,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CLI Error: {}", self.message)
    }
}

impl std::error::Error for CliError {}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Parse command-line arguments into a headless configuration.
///
/// Returns:
/// - `Ok(None)` if `--headless` is not present (graphical mode should run)
/// - `Ok(Some(config))` if `--headless` is present and arguments are valid
/// - `Err(CliError)` if argument validation fails
pub fn parse_headless_args() -> Result<Option<HeadlessConfig>, CliError> {
    let args: Vec<String> = env::args().collect();
    parse_headless_args_with_args(&args)
}

fn parse_headless_args_with_args(args: &[String]) -> Result<Option<HeadlessConfig>, CliError> {
    // Check if --headless is present
    if !args.iter().any(|arg| arg == "--headless") {
        return Ok(None);
    }

    let mut config = HeadlessConfig::default();
    let mut i = 1; // Skip program name

    while i < args.len() {
        let arg = &args[i];

        match arg.as_str() {
            "--headless" => {
                // Already checked above, just continue
            }
            "--steps" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--steps requires a value"));
                }
                config.steps = args[i]
                    .parse::<u32>()
                    .map_err(|_| CliError::new("--steps value must be a positive integer"))?;
                if config.steps == 0 {
                    return Err(CliError::new("--steps value must be greater than 0"));
                }
            }
            "--snapshot-every" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--snapshot-every requires a value"));
                }
                config.snapshot_every = args[i].parse::<u32>().map_err(|_| {
                    CliError::new("--snapshot-every value must be a non-negative integer")
                })?;
            }
            "--output" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--output requires a value"));
                }
                config.output = Some(PathBuf::from(&args[i]));
            }
            "--format" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--format requires a value"));
                }
                config.format = match args[i].as_str() {
                    "jsonl" => OutputFormat::Jsonl,
                    other => return Err(CliError::new(format!("Unknown format: {}", other))),
                };
            }
            "--detail" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--detail requires a value"));
                }
                config.detail = match args[i].as_str() {
                    "summary" => DetailLevel::Summary,
                    "balls" => DetailLevel::Balls,
                    other => return Err(CliError::new(format!("Unknown detail level: {}", other))),
                };
            }
            "--rng-seed" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--rng-seed requires a value"));
                }
                let seed = args[i].parse::<u64>().map_err(|_| {
                    CliError::new("--rng-seed value must be a non-negative integer")
                })?;
                config.rng_seed = Some(seed);
            }
            "--stdout" => {
                config.stdout = true;
            }
            "--no-stdout" => {
                config.stdout = false;
            }
            "--seed-balls" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--seed-balls requires a value"));
                }
                let count = args[i]
                    .parse::<u32>()
                    .map_err(|_| CliError::new("--seed-balls value must be a positive integer"))?;
                if count == 0 {
                    return Err(CliError::new("--seed-balls value must be greater than 0"));
                }
                config.seed_balls = Some(count);
            }
            other => {
                return Err(CliError::new(format!("Unknown argument: {}", other)));
            }
        }

        i += 1;
    }

    // Validate flag combinations
    if !config.stdout && config.output.is_none() {
        return Err(CliError::new(
            "At least one of --stdout or --output must be enabled",
        ));
    }

    Ok(Some(config))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod legacy_tests {
    use super::*;

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
        assert_eq!(
            config.output.unwrap().to_string_lossy(),
            "/tmp/output.jsonl"
        );
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
        assert_eq!(config.output.unwrap().to_string_lossy(), "/tmp/out.jsonl");
        assert_eq!(config.format, OutputFormat::Jsonl);
        assert_eq!(config.detail, DetailLevel::Balls);
        assert_eq!(config.rng_seed, Some(123));
        assert!(config.stdout);
        assert_eq!(config.seed_balls, Some(16));
    }
}
