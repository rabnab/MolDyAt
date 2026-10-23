// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Where world files live: one directory, configured once (`--save-dir`) and
//! handed to whoever saves or loads.
//!
//! Saves are named `world_YYYYMMDD_HHMMSS.json` (UTC). The timestamp sorts
//! chronologically, so the newest save is the last name in sort order.

use super::LoadSource;
use super::format::WorldFile;
use crate::error::{AppError, AppResult};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_PREFIX: &str = "world_";
const FILE_EXTENSION: &str = "json";

/// A directory of world saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveStore {
    dir: PathBuf,
}

fn io_error(action: &str, path: &Path, error: impl std::fmt::Display) -> AppError {
    AppError::CustomError(format!("{action} {}: {error}", path.display()))
}

impl SaveStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Write `file` as a new, timestamped save (the directory is created if
    /// missing) and return its path. An existing save is never overwritten: a
    /// second save in the same second gets a `_01`, `_02`, … suffix, which
    /// still sorts after it.
    ///
    /// The data goes to a temporary file first and is renamed into place, so a
    /// crash cannot leave a truncated save behind.
    pub(crate) fn save(&self, file: &WorldFile) -> AppResult<PathBuf> {
        let json = file.to_json()?;
        fs::create_dir_all(&self.dir).map_err(|e| io_error("cannot create", &self.dir, e))?;

        let stamp = timestamp_utc(SystemTime::now());
        let mut path = self.file_path(&stamp);
        let mut counter = 0;
        while path.exists() {
            counter += 1;
            path = self.file_path(&format!("{stamp}_{counter:02}"));
        }

        let tmp = path.with_extension(format!("{FILE_EXTENSION}.tmp"));
        let mut out = fs::File::create(&tmp).map_err(|e| io_error("cannot create", &tmp, e))?;
        out.write_all(json.as_bytes())
            .and_then(|()| out.sync_all())
            .map_err(|e| io_error("cannot write", &tmp, e))?;
        drop(out);
        fs::rename(&tmp, &path).map_err(|e| io_error("cannot rename to", &path, e))?;
        Ok(path)
    }

    /// The newest save, or `None` when the directory is missing or holds none.
    ///
    /// Only files named like [`Self::save`] names them count, ordered by
    /// timestamp and then by suffix; hand-named files (`world_backup.json`)
    /// are never picked.
    pub(crate) fn latest(&self) -> Option<PathBuf> {
        fs::read_dir(&self.dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter_map(|path| {
                let (stamp, counter) = save_order(path.file_name()?.to_str()?)?;
                let order = (stamp.to_owned(), counter);
                Some((order, path))
            })
            .max_by(|(a, _), (b, _)| a.cmp(b))
            .map(|(_, path)| path)
    }

    /// Read and validate the file at `path`.
    #[cfg(test)]
    pub(crate) fn load(&self, path: &Path) -> AppResult<WorldFile> {
        self.load_reporting(path).map(|(file, _)| file)
    }

    /// [`Self::load`], also returning the fields this build skipped (see
    /// [`WorldFile::from_json_reporting`]).
    pub(crate) fn load_reporting(&self, path: &Path) -> AppResult<(WorldFile, Vec<String>)> {
        let json = fs::read_to_string(path).map_err(|e| io_error("cannot read", path, e))?;
        WorldFile::from_json_reporting(&json).map_err(|AppError::CustomError(message)| {
            AppError::CustomError(format!("{}: {message}", path.display()))
        })
    }

    /// The file `source` refers to.
    pub(crate) fn resolve(&self, source: &LoadSource) -> AppResult<PathBuf> {
        match source {
            LoadSource::Path(path) => Ok(path.clone()),
            LoadSource::Latest => self.latest().ok_or_else(|| {
                AppError::CustomError(format!("no saved world found in {}", self.dir.display()))
            }),
        }
    }

    fn file_path(&self, stamp: &str) -> PathBuf {
        self.dir
            .join(format!("{FILE_PREFIX}{stamp}.{FILE_EXTENSION}"))
    }
}

/// The sort key of a save named `world_YYYYMMDD_HHMMSS[_NN].json`: its
/// timestamp and suffix number (0 without one). `None` for any other name.
pub(crate) fn save_order(name: &str) -> Option<(&str, u32)> {
    let rest = name
        .strip_prefix(FILE_PREFIX)?
        .strip_suffix(FILE_EXTENSION)?
        .strip_suffix('.')?;
    let (stamp, suffix) = rest.split_at_checked(15)?;
    let stamp_ok = stamp
        .char_indices()
        .all(|(i, c)| if i == 8 { c == '_' } else { c.is_ascii_digit() });
    if !stamp_ok {
        return None;
    }
    let counter = match suffix {
        "" => 0,
        _ => {
            let digits = suffix.strip_prefix('_')?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.parse().ok()?
        }
    };
    Some((stamp, counter))
}

/// `YYYYMMDD_HHMMSS` in UTC.
pub(crate) fn timestamp_utc(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let in_day = secs.rem_euclid(86_400);
    format!(
        "{year:04}{month:02}{day:02}_{:02}{:02}{:02}",
        in_day / 3600,
        in_day % 3600 / 60,
        in_day % 60
    )
}

/// Civil date for a day count since 1970-01-01 (Howard Hinnant's algorithm).
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
