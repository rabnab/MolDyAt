// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! MolDyAt simulation physics: particles and species, pair potentials,
//! walls and operators, the simulation world and its stepping, run
//! settings and saves. Builds without a windowing or GPU stack; the
//! physics thread, the snapshots and headless runs live in `thermo_runtime`,
//! the UI in `thermo_ui` and the wgpu backend in `moldyat_wgpu` (see
//! `docs/architecture.md`).

#![feature(portable_simd)]
#![cfg_attr(not(test), warn(clippy::print_stdout))]

pub mod application;
pub mod error;
pub(crate) mod logging;
pub mod objects2d;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
