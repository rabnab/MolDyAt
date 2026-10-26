// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! MolDyAt runtime without a window: the physics thread and its snapshot
//! publisher, the commands it handles (`DomainCommand`, `OperatorCommand`),
//! the `RendererSnapshot` it publishes, and headless runs (CLI options,
//! runner, JSONL output, timings). Builds without a windowing or GPU stack;
//! the physics is in `thermo_physics` (see `docs/architecture.md`).

#![cfg_attr(not(test), warn(clippy::print_stdout))]

pub mod application;
pub mod headless_cli;
pub mod headless_output;
pub mod headless_runner;
pub mod headless_timings;
pub mod metrics;

// The physics modules, under the paths the moved code uses. Private: users
// of this crate take them from `thermo_physics`.
use thermo_physics::objects2d;
