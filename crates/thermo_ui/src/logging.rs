// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The logger setup of the binary. The log targets (`moldyat::verbose`,
//! `moldyat::timing`) are used inside `thermo_physics`.

/// Initialise the global logger via [`env_logger`].
///
/// Call once at program start.  The default filter level is `info`;
/// override with `RUST_LOG=debug` (or `trace`) for more detail, or with
/// `RUST_LOG=info,moldyat::verbose=debug` for the verbose-ball output only.
pub fn init() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"),
    )
    .format_timestamp(None)
    .init();
}
