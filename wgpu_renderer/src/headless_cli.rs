// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Headless mode CLI argument parsing and configuration.
///
/// This module handles parsing command-line arguments for headless execution mode
/// and storing the parsed configuration for use by the headless simulation loop.
use crate::cli::OptionHelp;
use rust_thermo_core::application::valve_span_problem;
use rust_thermo_core::objects2d::line_segment::{DEFAULT_THICKNESS, MAX_THICKNESS, MIN_THICKNESS};
use std::path::PathBuf;

/// `--help` entries for every flag `parse_headless_args` takes. Keep in sync:
/// `every_parsed_flag_is_documented` checks it.
pub const HEADLESS_OPTIONS: &[OptionHelp] = &[
    OptionHelp {
        names: &["--headless"],
        value: None,
        help: &["Run without a window; enables the options below"],
    },
    OptionHelp {
        names: &["--steps"],
        value: Some("<N>"),
        help: &["Physics steps to run (default 1)"],
    },
    OptionHelp {
        names: &["--snapshot-every"],
        value: Some("<N>"),
        help: &["Write a frame every N steps (default 0: first and last only)"],
    },
    OptionHelp {
        names: &["--output"],
        value: Some("<PATH>"),
        help: &["Write JSONL records to this file"],
    },
    OptionHelp {
        names: &["--format"],
        value: Some("<jsonl>"),
        help: &["Output format (default jsonl)"],
    },
    OptionHelp {
        names: &["--detail"],
        value: Some("<LEVEL>"),
        help: &["summary (default), balls, thermo or balls+thermo"],
    },
    OptionHelp {
        names: &["--stdout"],
        value: None,
        help: &["Print a summary line per frame (default)"],
    },
    OptionHelp {
        names: &["--no-stdout"],
        value: None,
        help: &["No console output; needs --output"],
    },
    OptionHelp {
        names: &["--rng-seed"],
        value: Some("<N>"),
        help: &["Seed for reproducible runs"],
    },
    OptionHelp {
        names: &["--seed-balls"],
        value: Some("<N>"),
        help: &["Place up to N random balls before the first step"],
    },
    OptionHelp {
        names: &["--seed-speed"],
        value: Some("<V>"),
        help: &["Seeded balls get speeds in 0..V (default 0: at rest)"],
    },
    OptionHelp {
        names: &["--dt"],
        value: Some("<SECONDS>"),
        help: &["Time step (default: the simulation default)"],
    },
    OptionHelp {
        names: &["--world-scale"],
        value: Some("<F>"),
        help: &["Scale the default world's width and height by F"],
    },
    OptionHelp {
        names: &["--line-segment"],
        value: Some("<x1,y1,x2,y2[,t]>"),
        help: &[
            "Add a wall in world coordinates; thickness t 1..20 (default 1)",
            "Repeatable; walls are placed before --seed-balls",
        ],
    },
    OptionHelp {
        names: &["--valve"],
        value: Some("<wall,t0,t1[,open|closed]>"),
        help: &[
            "Cut a valve into --line-segment number `wall` (0-based) over",
            "fractions t0..t1 of its length; one per wall, default open",
        ],
    },
    OptionHelp {
        names: &["--save-setup"],
        value: None,
        help: &["Save the prepared world to --save-dir before the first step"],
    },
    OptionHelp {
        names: &["--timings"],
        value: Some("<PATH>"),
        help: &["Write ms/step per phase as JSON (see docs/perf-baseline.md)"],
    },
    OptionHelp {
        names: &["--timing-warmup"],
        value: Some("<N>"),
        help: &["Steps before --timings starts measuring (default 0)"],
    },
];

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
    /// Time step override; `None` keeps the simulation default.
    pub dt: Option<f32>,
    /// Factor applied to the default world's width and height (around its
    /// centre); `None` keeps the default world.
    pub world_scale: Option<f32>,
    /// Walls placed before any balls are seeded, in world coordinates.
    pub line_segments: Vec<LineSegmentSpec>,
    /// Valves cut into those walls, at most one per wall.
    pub valves: Vec<ValveSpec>,
    /// Write the per-phase step timings as JSON to this file (#221).
    pub timings: Option<PathBuf>,
    /// Steps run before the timings start; they are not counted.
    pub timing_warmup: u32,
    /// Save the world to the save directory once it is set up (walls,
    /// valves, seeded balls), before the first step. Perf runs seed a large
    /// world once and `--load` it for every run.
    pub save_setup: bool,
}

/// One `--valve wall,t_start,t_end[,open|closed]` valve: `wall` is the index
/// of a `--line-segment` (0-based, in order given), `t_start`/`t_end` are
/// fractions along it. Valves default to open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValveSpec {
    pub wall: usize,
    pub t_start: f32,
    pub t_end: f32,
    pub is_closed: bool,
}

impl ValveSpec {
    fn parse(value: &str) -> Result<Self, CliError> {
        const USAGE: &str = "--valve value must be wall,t_start,t_end[,open|closed]";
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        if !(3..=4).contains(&parts.len()) {
            return Err(CliError::new(USAGE));
        }
        let wall = parts[0]
            .parse::<usize>()
            .map_err(|_| CliError::new(USAGE))?;
        let fraction = |part: &str| {
            part.parse::<f32>()
                .ok()
                .filter(|t| (0.0..=1.0).contains(t))
                .ok_or_else(|| CliError::new("--valve t_start and t_end must be in 0..1"))
        };
        let (t_start, t_end) = (fraction(parts[1])?, fraction(parts[2])?);
        if t_start >= t_end {
            return Err(CliError::new("--valve t_start must be less than t_end"));
        }
        if let Some(problem) = valve_span_problem(t_start, t_end) {
            return Err(CliError::new(format!("--valve: {problem}")));
        }
        let is_closed = match parts.get(3).copied() {
            None | Some("open") => false,
            Some("closed") => true,
            Some(_) => return Err(CliError::new(USAGE)),
        };
        Ok(Self {
            wall,
            t_start,
            t_end,
            is_closed,
        })
    }
}

/// One `--line-segment x1,y1,x2,y2[,thickness]` wall. The thickness range and
/// default come from core (`objects2d::line_segment`), shared with the UI keys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineSegmentSpec {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub thickness: f32,
}

impl LineSegmentSpec {
    fn parse(value: &str) -> Result<Self, CliError> {
        const USAGE: &str = "--line-segment value must be x1,y1,x2,y2[,thickness]";
        let parts = value
            .split(',')
            .map(|part| part.trim().parse::<f32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| CliError::new(USAGE))?;
        if !(4..=5).contains(&parts.len()) || parts.iter().any(|v| !v.is_finite()) {
            return Err(CliError::new(USAGE));
        }
        let spec = Self {
            start: (parts[0], parts[1]),
            end: (parts[2], parts[3]),
            thickness: parts.get(4).copied().unwrap_or(DEFAULT_THICKNESS),
        };
        if spec.start == spec.end {
            return Err(CliError::new("--line-segment start and end must differ"));
        }
        if !(MIN_THICKNESS..=MAX_THICKNESS).contains(&spec.thickness) {
            return Err(CliError::new(format!(
                "--line-segment thickness must be between {MIN_THICKNESS} and {MAX_THICKNESS}"
            )));
        }
        Ok(spec)
    }
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
            dt: None,
            world_scale: None,
            line_segments: Vec::new(),
            valves: Vec::new(),
            timings: None,
            timing_warmup: 0,
            save_setup: false,
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
            "--dt" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--dt requires a value"));
                }
                let dt = args[i]
                    .parse::<f32>()
                    .ok()
                    .filter(|dt| dt.is_finite() && *dt > 0.0)
                    .ok_or_else(|| CliError::new("--dt value must be a positive number"))?;
                config.dt = Some(dt);
            }
            "--world-scale" => {
                i += 1;
                if i >= args.len() {
                    return Err(CliError::new("--world-scale requires a value"));
                }
                let scale = args[i]
                    .parse::<f32>()
                    .ok()
                    .filter(|scale| scale.is_finite() && *scale > 0.0)
                    .ok_or_else(|| {
                        CliError::new("--world-scale value must be a positive number")
                    })?;
                config.world_scale = Some(scale);
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

#[cfg(test)]
mod tests;
