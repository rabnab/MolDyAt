// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! winit's `Window` as a [`RenderWindow`] (#372), and the opaque id the
//! renderer keys its plot surface by.

use crate::wgpu::render::surface_window::RenderWindow;
use winit::window::{Window, WindowId};

impl RenderWindow for Window {
    fn inner_size(&self) -> (u32, u32) {
        let size = Window::inner_size(self);
        (size.width, size.height)
    }

    fn request_redraw(&self) {
        Window::request_redraw(self);
    }

    fn pre_present_notify(&self) {
        Window::pre_present_notify(self);
    }
}

/// The renderer's opaque id for a window: winit's id as a number.
pub(crate) fn surface_id(id: WindowId) -> u64 {
    u64::from(id)
}
