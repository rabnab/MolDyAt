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
                let seed = args[i]
                    .parse::<u64>()
                    .map_err(|_| CliError::new("--rng-seed value must be a non-negative integer"))?;
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
mod tests {
    use super::*;

    #[test]
    fn test_no_headless_flag_returns_none() {
        // This test is context-dependent on env::args(), so we verify the behavior
        // by calling parse_headless_args when --headless is not in args.
        // We'll use a simpler approach: test with --headless explicitly.
        let result = parse_headless_args();
        // If --headless is not in env::args(), this should return Ok(None)
        match result {
            Ok(None) => {} // Expected when --headless is not present
            Ok(Some(_)) => {
                panic!("Expected Ok(None) when --headless is not in env::args()")
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[test]
    fn test_default_config() {
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
    fn test_steps_requires_value() {
        // Note: This is a placeholder test structure. Real tests would need
        // to mock env::args() or refactor parse_headless_args to accept args.
        // For now, we document the expected behavior.
    }

    #[test]
    fn test_stdout_no_output_requires_file() {
        // Placeholder: In real tests, we'd verify that having both
        // --no-stdout and no --output returns an error.
    }
}
