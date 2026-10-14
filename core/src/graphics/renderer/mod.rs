// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod batch_renderer;
pub mod event_loop;
pub mod interpolation;
pub mod margins;
pub(crate) mod mat3;
pub mod state_layers;
pub mod viewport;

#[cfg(test)]
mod tests;

// ── Public API re-exports ───────────────────────────────────────────
// Short paths for types used across modules and by the wgpu renderer;
// everything else is imported from its submodule.
pub use event_loop::InputAction;
pub use margins::Margins;
pub use viewport::ViewportTransform;
