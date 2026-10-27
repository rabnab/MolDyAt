// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the renderer needs of a window (#372), without naming the windowing
//! library: the raw handles wgpu creates a surface from, its size, and the
//! two calls around presenting. The shell implements it for
//! its windows.

use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};

/// A window the renderer can draw into.
pub(crate) trait RenderWindow: HasWindowHandle + HasDisplayHandle + Send + Sync {
    /// Inner size in physical pixels.
    fn inner_size(&self) -> (u32, u32);
    /// Ask for another redraw (a frame that could not be drawn now).
    fn request_redraw(&self);
    /// Called right before a frame is presented (some platforms pace frames
    /// with it).
    fn pre_present_notify(&self);
}
