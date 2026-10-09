// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::frame_renderer::{FrameRenderer, SurfaceErrorAction};

#[test]
fn out_of_memory_surface_errors_exit() {
    let action = FrameRenderer::surface_error_action(&wgpu::SurfaceError::OutOfMemory);

    assert_eq!(action, SurfaceErrorAction::Exit);
}

#[test]
fn lost_and_outdated_surface_errors_reconfigure() {
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::SurfaceError::Lost),
        SurfaceErrorAction::Reconfigure
    );
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::SurfaceError::Outdated),
        SurfaceErrorAction::Reconfigure
    );
}

#[test]
fn transient_surface_errors_retry_later() {
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::SurfaceError::Timeout),
        SurfaceErrorAction::RetryLater
    );
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::SurfaceError::Other),
        SurfaceErrorAction::RetryLater
    );
}
