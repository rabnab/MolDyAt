// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::render::frame::{SurfaceErrorAction, surface_error_action};

#[test]
fn lost_and_outdated_surface_statuses_reconfigure() {
    assert_eq!(
        surface_error_action(&wgpu::CurrentSurfaceTexture::Lost),
        SurfaceErrorAction::Reconfigure
    );
    assert_eq!(
        surface_error_action(&wgpu::CurrentSurfaceTexture::Outdated),
        SurfaceErrorAction::Reconfigure
    );
}

#[test]
fn transient_surface_statuses_retry_later() {
    assert_eq!(
        surface_error_action(&wgpu::CurrentSurfaceTexture::Timeout),
        SurfaceErrorAction::RetryLater
    );
}

#[test]
fn validation_surface_status_exits() {
    assert_eq!(
        surface_error_action(&wgpu::CurrentSurfaceTexture::Validation),
        SurfaceErrorAction::Exit
    );
}

#[test]
fn occluded_surface_status_skips_the_frame() {
    assert_eq!(
        surface_error_action(&wgpu::CurrentSurfaceTexture::Occluded),
        SurfaceErrorAction::Skip
    );
}
