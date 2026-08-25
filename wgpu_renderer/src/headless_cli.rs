/// Headless mode CLI argument parsing and configuration.
///
/// This module handles parsing command-line arguments for headless execution mode
/// and storing the parsed configuration for use by the headless simulation loop.
use std::env;
use std::path::PathBuf;

/// Parsed headless CLI arguments and their default values.
#[derive(Debug, Clone)]
pub struct HeadlessConfig {
    pub steps: u32,
    pub snapshot_every: u32,
    pub output: Option<PathBuf>,
    pub format: OutputFormat,
    pub detail: DetailLevel,
    pub rng_seed: Option<u64>,
    pub stdout: bool,
    pub seed_balls: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailLevel {
    Summary,
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

pub fn parse_headless_args() -> Result<Option<HeadlessConfig>, CliError> {
    let args: Vec<String> = env::args().collect();
    parse_headless_args_with_args(&args)
}

fn parse_headless_args_with_args(args: &[String]) -> Result<Option<HeadlessConfig>, CliError> {
    if !args.iter().any(|arg| arg == "--headless") {
        return Ok(None);
    }

    let mut config = HeadlessConfig::default();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--headless" => {}
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
                config.rng_seed = Some(args[i].parse::<u64>().map_err(|_| {
                    CliError::new("--rng-seed value must be a non-negative integer")
                })?);
            }
            "--stdout" => config.stdout = true,
            "--no-stdout" => config.stdout = false,
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
            other => return Err(CliError::new(format!("Unknown argument: {}", other))),
        }
        i += 1;
    }

    let has_stdout = args.iter().any(|arg| arg == "--stdout");
    let has_no_stdout = args.iter().any(|arg| arg == "--no-stdout");
    if has_stdout && has_no_stdout {
        return Err(CliError::new("Cannot pass both --stdout and --no-stdout"));
    }

    if !config.stdout && config.output.is_none() {
        return Err(CliError::new("At least one of --stdout or --output must be enabled"));
    }

    Ok(Some(config))
}

#[cfg(test)]
mod tests;
