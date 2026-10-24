// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! MolDyAt user interface without a GPU backend: the event loop and input
//! handling, the keyboard command registry, operator tools, the viewport,
//! the sidebar model and the energy plot window (`graphics`), plus the
//! logger setup and the UI settings file (`ui_settings`). The simulation is in `thermo_physics` and `thermo_runtime`;
//! the wgpu backend and the binary are in `moldyat_wgpu` (see
//! `docs/architecture.md`).

// The UI must not write to stdout: it breaks `--no-stdout` headless runs and
// cannot be filtered. Use `log` (see `logging`); tests may print.
#![cfg_attr(not(test), warn(clippy::print_stdout))]

pub mod graphics;
pub mod logging;
pub mod ui_settings;
