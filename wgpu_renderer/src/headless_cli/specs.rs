// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The values of `--valve` and `--line-segment`.

use super::*;
use moldyat_core::application::valve_span_problem;
use moldyat_core::objects2d::line_segment::{DEFAULT_THICKNESS, MAX_THICKNESS, MIN_THICKNESS};

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
    pub(super) fn parse(value: &str) -> Result<Self, CliError> {
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
    pub(super) fn parse(value: &str) -> Result<Self, CliError> {
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
