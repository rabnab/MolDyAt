//! Options shared by the windowed and the headless run, parsed once in `main`
//! before the headless options.

use crate::headless_cli::CliError;
use std::path::PathBuf;

/// Where saves live unless `--save-dir` says otherwise (relative to the
/// working directory).
pub const DEFAULT_SAVE_DIR: &str = "saves";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonArgs {
    /// `--load <path>`: start from this save.
    pub load: Option<PathBuf>,
    /// `--save-dir <dir>`: directory for Ctrl+S / Ctrl+O saves.
    pub save_dir: PathBuf,
}

impl Default for CommonArgs {
    fn default() -> Self {
        Self {
            load: None,
            save_dir: PathBuf::from(DEFAULT_SAVE_DIR),
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
            _ => rest.push(arg.clone()),
        }
    }
    Ok((common, rest))
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
