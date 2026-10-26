// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::export::ExportFormat;
use std::path::PathBuf;
use thermo_physics::application::sim_config::{MAX_TIME_STEP, MIN_TIME_STEP, SimConfig};

mod options;
mod specs;

/// One option in the `--help` text (see `HEADLESS_OPTIONS`; the binary
/// builds the whole text from such tables).
pub struct OptionHelp {
    /// Spellings, e.g. `&["-h", "--help"]`.
    pub names: &'static [&'static str],
    /// Placeholder for the value, e.g. `Some("<N>")`; `None` for a switch.
    pub value: Option<&'static str>,
    /// One line per entry; later lines are indented under the first.
    pub help: &'static [&'static str],
}

pub use options::HEADLESS_OPTIONS;
pub use specs::{LineSegmentSpec, ValveSpec};

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
    /// Seeded balls get speeds uniform in `0..=seed_speed`; `None` seeds them at rest.
    /// Meant for performance runs: the net momentum is not removed, so the gas
    /// drifts as a whole.
    pub seed_speed: Option<f32>,
    /// `--seed-species`: species names and weights of the seeded mixture;
    /// empty seeds the default species. Checked against the run's species
    /// table when the world is prepared.
    pub seed_species: Vec<(String, u32)>,
    /// Time step override; `None` keeps the simulation default.
    pub dt: Option<f32>,
    /// Walls placed before any balls are seeded, in world coordinates.
    pub line_segments: Vec<LineSegmentSpec>,
    /// Valves cut into those walls, at most one per wall.
    pub valves: Vec<ValveSpec>,
    /// Write the per-phase step timings as JSON to this file (#221).
    pub timings: Option<PathBuf>,
    /// Steps run before the timings start; they are not counted.
    pub timing_warmup: u32,
    /// Write the series and the final per-ball snapshot to this directory as
    /// `series.<ext>` and `snapshot.<ext>` once the run ends (#337).
    pub export_dir: Option<PathBuf>,
    /// Format of the export files (`--export-format`, default CSV).
    pub export_format: ExportFormat,
    /// Series to export (`--export-series`); `None` exports all.
    pub export_series: Option<Vec<String>>,
    /// Save the world to the save directory once it is set up (walls,
    /// valves, seeded balls), before the first step. Perf runs seed a large
    /// world once and `--load` it for every run.
    pub save_setup: bool,
    /// The effective run settings (`--config` with `--world-scale`, then
    /// `--dt`, or a loaded save's config), set once the application is prepared and recorded in
    /// `run_start` (#295). `None` until then.
    pub sim: Option<SimConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailLevel {
    Summary,
    Balls,
    Thermo,
    BallsAndThermo,
}

impl DetailLevel {
    pub fn includes_balls(self) -> bool {
        matches!(self, Self::Balls | Self::BallsAndThermo)
    }

    pub fn includes_thermo(self) -> bool {
        matches!(self, Self::Thermo | Self::BallsAndThermo)
    }
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
            seed_speed: None,
            seed_species: Vec::new(),
            dt: None,
            line_segments: Vec::new(),
            valves: Vec::new(),
            timings: None,
            timing_warmup: 0,
            export_dir: None,
            export_format: ExportFormat::default(),
            export_series: None,
            save_setup: false,
            sim: None,
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

/// Parse the headless options from `args` (program name first), after the
/// shared options were taken out by `cli::split_common_args`.
pub fn parse_headless_args(args: &[String]) -> Result<Option<HeadlessConfig>, CliError> {
    parse_headless_args_with_args(args)
}

fn parse_headless_args_with_args(args: &[String]) -> Result<Option<HeadlessConfig>, CliError> {
    if !args.iter().any(|arg| arg == "--headless") {
        return Ok(None);
    }

    let mut config = HeadlessConfig::default();
    // `--export-format` or `--export-series` seen; both need `--export-dir`.
    let mut export_options_given = false;
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
                    "thermo" => DetailLevel::Thermo,
                    "balls+thermo" | "balls-and-thermo" => DetailLevel::BallsAndThermo,
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
            "--save-setup" => config.save_setup = true,
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
            "--seed-speed" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--seed-speed requires a value"));
                }
                let speed = args[i]
                    .parse::<f32>()
                    .ok()
                    .filter(|speed| speed.is_finite() && *speed >= 0.0)
                    .ok_or_else(|| {
                        CliError::new("--seed-speed value must be a non-negative number")
                    })?;
                config.seed_speed = Some(speed);
            }
            "--seed-species" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--seed-species requires a value"));
                }
                config.seed_species = parse_seed_species(&args[i])?;
            }
            "--dt" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--dt requires a value"));
                }
                let dt = args[i]
                    .parse::<f32>()
                    .ok()
                    .filter(|dt| (MIN_TIME_STEP..=MAX_TIME_STEP).contains(dt))
                    .ok_or_else(|| {
                        CliError::new(format!(
                            "--dt value must be a number within {MIN_TIME_STEP}..={MAX_TIME_STEP}"
                        ))
                    })?;
                config.dt = Some(dt);
            }
            "--line-segment" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--line-segment requires a value"));
                }
                config.line_segments.push(LineSegmentSpec::parse(&args[i])?);
            }
            "--valve" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--valve requires a value"));
                }
                config.valves.push(ValveSpec::parse(&args[i])?);
            }
            "--timings" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--timings requires a value"));
                }
                config.timings = Some(PathBuf::from(&args[i]));
            }
            "--timing-warmup" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--timing-warmup requires a value"));
                }
                config.timing_warmup = args[i].parse::<u32>().map_err(|_| {
                    CliError::new("--timing-warmup value must be a non-negative integer")
                })?;
            }
            "--export-dir" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--export-dir requires a value"));
                }
                config.export_dir = Some(PathBuf::from(&args[i]));
            }
            "--export-format" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--export-format requires a value"));
                }
                config.export_format = ExportFormat::parse(&args[i])
                    .ok_or_else(|| CliError::new("--export-format value must be csv or jsonl"))?;
                export_options_given = true;
            }
            "--export-series" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--export-series requires a value"));
                }
                let names: Vec<String> = args[i].split(',').map(|n| n.trim().to_string()).collect();
                if names.iter().any(String::is_empty) {
                    return Err(CliError::new("--export-series names must not be empty"));
                }
                config.export_series = Some(names);
                export_options_given = true;
            }
            other => return Err(CliError::new(format!("Unknown argument: {}", other))),
        }
        i += 1;
    }

    if args
        .windows(2)
        .any(|pair| pair[0] == "--stdout" && pair[1] == "--no-stdout")
        || args
            .windows(2)
            .any(|pair| pair[0] == "--no-stdout" && pair[1] == "--stdout")
    {
        return Err(CliError::new("--stdout and --no-stdout cannot be combined"));
    }

    if config.export_dir.is_none() && export_options_given {
        return Err(CliError::new(
            "--export-format and --export-series need --export-dir",
        ));
    }

    for (index, valve) in config.valves.iter().enumerate() {
        if valve.wall >= config.line_segments.len() {
            return Err(CliError::new(format!(
                "--valve wall {} does not exist ({} --line-segment given)",
                valve.wall,
                config.line_segments.len()
            )));
        }
        // A valve splits its wall into pieces; a second one would need to
        // find the piece it falls on. Not needed for the perf scenarios.
        if config.valves[..index].iter().any(|v| v.wall == valve.wall) {
            return Err(CliError::new(format!(
                "--valve: wall {} has more than one valve",
                valve.wall
            )));
        }
    }

    if !config.seed_species.is_empty() && config.seed_balls.is_none() {
        return Err(CliError::new("--seed-species needs --seed-balls"));
    }

    if config.timing_warmup > 0 && config.timing_warmup >= config.steps {
        return Err(CliError::new("--timing-warmup must be less than --steps"));
    }

    if !config.stdout && config.output.is_none() {
        return Err(CliError::new(
            "At least one of --stdout or --output must be enabled",
        ));
    }

    Ok(Some(config))
}

/// `NAME[:WEIGHT],...`: unique names, weights positive integers (default 1).
fn parse_seed_species(value: &str) -> Result<Vec<(String, u32)>, CliError> {
    const USAGE: &str = "--seed-species value must be NAME[:WEIGHT],... with weights above 0";
    let mut mixture: Vec<(String, u32)> = Vec::new();
    for part in value.split(',').map(str::trim) {
        let (name, weight) = match part.split_once(':') {
            Some((name, weight)) => (
                name.trim(),
                weight
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| CliError::new(USAGE))?,
            ),
            None => (part, 1),
        };
        if name.is_empty() || weight == 0 {
            return Err(CliError::new(USAGE));
        }
        if mixture.iter().any(|(n, _)| n == name) {
            return Err(CliError::new(format!(
                "--seed-species lists {name} more than once"
            )));
        }
        mixture.push((name.to_string(), weight));
    }
    Ok(mixture)
}

#[cfg(test)]
mod tests;
