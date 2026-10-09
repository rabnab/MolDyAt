// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Centralised logging infrastructure.
//!
//! Re-exports the [`log`] crate macros and provides domain-specific
//! helpers that were previously in `application::log_util`.

/// Initialise the global logger via [`env_logger`].
///
/// Call once at program start.  The default filter level is `info`;
/// override with `RUST_LOG=debug` (or `trace`) for more detail.
pub fn init() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"),
    )
    .format_timestamp(None)
    .init();
}

// ── Domain-specific log helpers (moved from application::log_util) ──

use crate::objects2d::ball_impl::{Ball2D, Named};

pub fn log_update_hood_start(ball: &Ball2D, cutoff: f32) {
    log::debug!("update_hood: {:?} cutoff_and_skin: {}", ball, cutoff);
}

pub fn log_neighbour_add(ball: &Ball2D, cur_ref: &Ball2D, dist_sq: f32, dist_limit_sq: f32) {
    if ball.verbose.console {
        log::debug!(
            "\tneighbours {} at ({},{}) dist {}",
            cur_ref.get_name(),
            cur_ref.get_position().x,
            cur_ref.get_position().y,
            f32::sqrt(dist_sq)
        );
    } else if cur_ref.verbose.console {
        log::debug!(
            "\t {} is neighbour of {} since its dist is {} (dist_limit {})",
            cur_ref.get_name(),
            ball.get_name(),
            f32::sqrt(dist_sq),
            f32::sqrt(dist_limit_sq)
        );
    }
}
