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

/// A frame skipped while the window is hidden asks for nothing; the window
/// comes back only through `Occluded(false)`, which must redraw it (#374
/// review).
#[test]
fn a_hidden_window_is_redrawn_when_shown_again() {
    use crate::wgpu::render::frame::FrameOutcome;
    use crate::wgpu::shell::event_router::EventRouter;
    use crate::wgpu::shell::frame_renderer::{AfterFrame, after_frame};
    use winit::event::WindowEvent;

    let skipped = surface_error_action(&wgpu::CurrentSurfaceTexture::Occluded);
    assert_eq!(skipped, SurfaceErrorAction::Skip);
    assert_eq!(
        after_frame(FrameOutcome::NotDrawn { retry: false }),
        AfterFrame::default(),
        "no busy redraw loop while hidden"
    );
    assert!(EventRouter::redraws_main_window(&WindowEvent::Occluded(
        false
    )));
    assert!(!EventRouter::redraws_main_window(&WindowEvent::Occluded(
        true
    )));
}

#[test]
fn the_window_acts_on_each_frame_outcome() {
    use crate::wgpu::render::frame::FrameOutcome;
    use crate::wgpu::shell::frame_renderer::{AfterFrame, after_frame};

    let none = AfterFrame::default();
    assert_eq!(after_frame(FrameOutcome::Done), none);
    assert_eq!(
        after_frame(FrameOutcome::Suboptimal),
        AfterFrame {
            fit_surface: true,
            ..none
        }
    );
    assert_eq!(
        after_frame(FrameOutcome::Reconfigure),
        AfterFrame {
            fit_surface: true,
            redraw: true,
            ..none
        }
    );
    assert_eq!(
        after_frame(FrameOutcome::NotDrawn { retry: true }),
        AfterFrame {
            redraw: true,
            ..none
        }
    );
    assert_eq!(
        after_frame(FrameOutcome::Exit),
        AfterFrame { exit: true, ..none }
    );
}
