// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Core must not write to stdout: it breaks `--no-stdout` headless runs and
// cannot be filtered. Use `log` (see `logging`); tests may print.
#![cfg_attr(not(test), warn(clippy::print_stdout))]

pub mod application;
pub mod graphics;
pub mod logging;

// The physics moved to `thermo_physics` (#308). These re-exports keep the
// old paths working for one release; new code uses `thermo_physics`.
pub use thermo_physics::{error, objects2d};
