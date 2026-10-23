// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Centralised logging infrastructure.
//!
//! Log targets and domain-specific log helpers. The logger itself is
//! installed by the binary (`moldyat_core::logging::init`).

/// Log target of the per-step output of balls in console verbose mode
/// (key `V`, mode `c`), at `debug` level: `RUST_LOG=moldyat::verbose=debug`.
pub const VERBOSE_TARGET: &str = "moldyat::verbose";

/// Log target of the step timings shown while the timing display (key
/// `F4`) is on, at `info` level.
pub const TIMING_TARGET: &str = "moldyat::timing";

/// True if the verbose-ball output is shown with the current log filter.
pub fn verbose_enabled() -> bool {
    log::log_enabled!(target: VERBOSE_TARGET, log::Level::Debug)
}

// ── Domain-specific log helpers (moved from application::log_util) ──

use crate::objects2d::ball_impl::{Ball2D, Named};

pub fn log_update_hood_start(ball: &Ball2D, skin: f32) {
    log::debug!("update_hood: {:?} skin: {}", ball, skin);
}

pub fn log_neighbour_add(ball: &Ball2D, cur_ref: &Ball2D, dist_sq: f32, dist_limit_sq: f32) {
    if ball.verbose.console {
        log::debug!(
            target: VERBOSE_TARGET,
            "\tneighbours {} at ({},{}) dist {}",
            cur_ref.get_name(),
            cur_ref.get_position().x,
            cur_ref.get_position().y,
            f32::sqrt(dist_sq)
        );
    } else if cur_ref.verbose.console {
        log::debug!(
            target: VERBOSE_TARGET,
            "\t {} is neighbour of {} since its dist is {} (dist_limit {})",
            cur_ref.get_name(),
            ball.get_name(),
            f32::sqrt(dist_sq),
            f32::sqrt(dist_limit_sq)
        );
    }
}
