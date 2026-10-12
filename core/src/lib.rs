// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![feature(portable_simd)]
#![allow(dead_code)]
// Core must not write to stdout: it breaks `--no-stdout` headless runs and
// cannot be filtered. Use `log` (see `logging`); tests may print.
#![cfg_attr(not(test), warn(clippy::print_stdout))]

pub mod application;
pub mod error;
pub mod graphics;
pub mod logging;
pub mod objects2d;
pub mod timing_monad;
