// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Options shared by the windowed and the headless run, parsed once in `main`
//! before the headless options.

use crate::headless_cli::{CliError, HEADLESS_OPTIONS};
use std::path::PathBuf;
use thermo_physics::application::SimConfig;

mod usage;
pub use usage::{OptionGroup, OptionHelp};

/// Name shown in `--help` and `--version`: the binary name (`moldyat`), not the
/// package name. `CARGO_BIN_NAME` exists only while compiling a `[[bin]]` target.
pub const PROGRAM: &str = env!("CARGO_BIN_NAME");

/// Package version and the git commit the binary was built from (see
/// `build.rs`), e.g. `1.3.0 (60c70fd)` or `1.3.0 (60c70fd-dirty)`. Shown by
/// `--version`, in the window title and in the startup log.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_COMMIT"), ")");

/// Whether `args` (program name first) ask for the version (`--version` or
/// `-V`); checked before any other parsing, in both modes.
pub fn wants_version(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
}

/// Whether `args` (program name first) ask for the usage (`--help` or `-h`);
/// checked before everything else, `--version` included.
pub fn wants_help(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .any(|arg| arg == "--help" || arg == "-h")
}

/// Flags handled in `main` before any parsing.
pub const GENERAL_OPTIONS: &[OptionHelp] = &[
    OptionHelp {
        names: &["-h", "--help"],
        value: None,
        help: &["Show this help and exit"],
    },
    OptionHelp {
        names: &["-V", "--version"],
        value: None,
        help: &["Show the version and the git commit of the build, and exit"],
    },
];

/// Flags parsed by [`split_common_args`], valid in both modes.
pub const COMMON_OPTIONS: &[OptionHelp] = &[
    OptionHelp {
        names: &["--load"],
        value: Some("<PATH>"),
        help: &["Start from this saved world"],
    },
    OptionHelp {
        names: &["--config"],
        value: Some("<PATH>"),
        help: &[
            "Simulation config file (TOML, or JSON if it ends in .json); flags such as --dt override it",
            "Not with --load: a save brings its own config",
        ],
    },
    OptionHelp {
        names: &["--save-dir"],
        value: Some("<DIR>"),
        help: &[
            "Directory for saves (Ctrl+S / Ctrl+O, --save-setup)",
            "Default: saves",
        ],
    },
];

/// The window takes only the shared options: anything left in `args` (program
/// name first, shared options already taken out) is an error, so a headless
/// flag without `--headless` is not silently ignored.
pub fn reject_window_args(args: &[String]) -> Result<(), CliError> {
    match args.get(1) {
        None => Ok(()),
        Some(arg) => Err(CliError::new(format!(
            "Unknown argument: {arg} (headless options need --headless)"
        ))),
    }
}

/// The `--help` text: every option table, in the order the flags are parsed.
pub fn usage_text() -> String {
    usage::usage(
        PROGRAM,
        VERSION,
        &[
            OptionGroup {
                title: "General",
                options: GENERAL_OPTIONS,
            },
            OptionGroup {
                title: "Both modes",
                options: COMMON_OPTIONS,
            },
            OptionGroup {
                title: "Headless (need --headless)",
                options: HEADLESS_OPTIONS,
            },
        ],
    )
}

/// Where saves live unless `--save-dir` says otherwise (relative to the
/// working directory).
pub const DEFAULT_SAVE_DIR: &str = "saves";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonArgs {
    /// `--load <path>`: start from this save.
    pub load: Option<PathBuf>,
    /// `--save-dir <dir>`: directory for Ctrl+S / Ctrl+O saves.
    pub save_dir: PathBuf,
    /// `--config <path>`: simulation config file (TOML, or JSON for `.json`).
    pub config: Option<PathBuf>,
}

impl Default for CommonArgs {
    fn default() -> Self {
        Self {
            load: None,
            save_dir: PathBuf::from(DEFAULT_SAVE_DIR),
            config: None,
        }
    }
}

/// Take the shared options out of `args` (program name first) and return them
/// together with the remaining arguments, which keep their order.
pub fn split_common_args(args: &[String]) -> Result<(CommonArgs, Vec<String>), CliError> {
    let mut common = CommonArgs::default();
    let mut rest = Vec::with_capacity(args.len());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--load" => common.load = Some(PathBuf::from(value_of(&mut iter, "--load")?)),
            "--save-dir" => common.save_dir = PathBuf::from(value_of(&mut iter, "--save-dir")?),
            "--config" => common.config = Some(PathBuf::from(value_of(&mut iter, "--config")?)),
            _ => rest.push(arg.clone()),
        }
    }
    // A save restores the config it ran with; a second source would leave
    // it unclear which one wins.
    if common.config.is_some() && common.load.is_some() {
        return Err(CliError::new(
            "--config cannot be combined with --load: a save brings its own config \
             (edit its `config` section instead)",
        ));
    }
    Ok((common, rest))
}

impl CommonArgs {
    /// The simulation config: the `--config` file, or the defaults without one.
    pub fn sim_config(&self) -> Result<SimConfig, CliError> {
        match &self.config {
            None => Ok(SimConfig::default()),
            Some(path) => SimConfig::load(path)
                .map_err(|error| CliError::new(format!("--config {}: {error:?}", path.display()))),
        }
    }
}

fn value_of<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
) -> Result<&'a String, CliError> {
    iter.next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| CliError::new(format!("{flag} requires a path")))
}

#[cfg(test)]
mod tests;
