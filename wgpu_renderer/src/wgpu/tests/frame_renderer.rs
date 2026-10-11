// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::frame_renderer::{FrameRenderer, SurfaceErrorAction};

#[test]
fn lost_and_outdated_surface_statuses_reconfigure() {
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::CurrentSurfaceTexture::Lost),
        SurfaceErrorAction::Reconfigure
    );
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::CurrentSurfaceTexture::Outdated),
        SurfaceErrorAction::Reconfigure
    );
}

#[test]
fn transient_surface_statuses_retry_later() {
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::CurrentSurfaceTexture::Timeout),
        SurfaceErrorAction::RetryLater
    );
    assert_eq!(
        FrameRenderer::surface_error_action(&wgpu::CurrentSurfaceTexture::Occluded),
        SurfaceErrorAction::RetryLater
    );
}
