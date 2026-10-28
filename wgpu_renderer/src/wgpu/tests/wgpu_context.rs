// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::render::wgpu_context::{
    PendingSurfaceSize, choose_present_mode, clamp_surface_size, default_present_mode,
    parse_frame_latency, parse_present_mode,
};
use std::time::Instant;
use wgpu::PresentMode;

/// A GPU limit no test size reaches, except where a test says so.
const MAX: u32 = 8192;

fn take_size(pending: &mut PendingSurfaceSize) -> Option<(u32, u32)> {
    pending.take().map(|resize| resize.size)
}

#[test]
fn nothing_is_pending_before_the_first_request() {
    let mut pending = PendingSurfaceSize::default();

    assert_eq!(take_size(&mut pending), None);
}

#[test]
fn a_burst_of_requests_leaves_only_the_newest_size() {
    let mut pending = PendingSurfaceSize::default();

    pending.request(800, 600, MAX);
    pending.request(810, 604, MAX);
    pending.request(820, 610, MAX);

    let resize = pending.take().expect("a resize is pending");
    assert_eq!(resize.size, (820, 610));
    assert_eq!(resize.requests, 3);
}

#[test]
fn a_merged_resize_keeps_the_time_of_its_first_request() {
    let mut pending = PendingSurfaceSize::default();
    let before_first = Instant::now();
    pending.request(800, 600, MAX);
    let before_second = Instant::now();
    pending.request(810, 604, MAX);

    let resize = pending.take().expect("a resize is pending");
    assert!(before_first <= resize.since);
    assert!(
        resize.since <= before_second,
        "the second request does not move it"
    );
}

#[test]
fn a_size_is_taken_only_once() {
    let mut pending = PendingSurfaceSize::default();
    pending.request(800, 600, MAX);

    assert_eq!(take_size(&mut pending), Some((800, 600)));
    assert_eq!(take_size(&mut pending), None);
}

#[test]
fn the_same_size_requested_again_is_pending_again() {
    // A lost or outdated surface is reconfigured at an unchanged size.
    let mut pending = PendingSurfaceSize::default();
    pending.request(800, 600, MAX);
    pending.take();

    pending.request(800, 600, MAX);

    assert_eq!(take_size(&mut pending), Some((800, 600)));
}

#[test]
fn an_empty_size_is_ignored() {
    let mut pending = PendingSurfaceSize::default();
    pending.request(0, 600, MAX);
    pending.request(800, 0, MAX);
    assert_eq!(take_size(&mut pending), None);

    pending.request(800, 600, MAX);
    pending.request(0, 0, MAX);
    let resize = pending.take().expect("a resize is pending");
    assert_eq!(resize.size, (800, 600));
    assert_eq!(resize.requests, 1);
}

#[test]
fn an_oversized_request_is_clamped_to_the_gpu_limit() {
    // Wayland grants a 12032x6275 window at once (#363).
    let mut pending = PendingSurfaceSize::default();
    pending.request(12032, 6275, MAX);
    assert_eq!(take_size(&mut pending), Some((8192, 6275)));
}

#[test]
fn surface_sizes_are_clamped_per_side() {
    assert_eq!(clamp_surface_size(800, 600, MAX), (800, 600));
    assert_eq!(clamp_surface_size(9000, 9000, MAX), (MAX, MAX));
    assert_eq!(clamp_surface_size(0, 600, MAX), (1, 600));
}

#[test]
fn present_modes_parse_by_name() {
    assert_eq!(parse_present_mode("fifo"), Some(PresentMode::Fifo));
    assert_eq!(parse_present_mode("Mailbox"), Some(PresentMode::Mailbox));
    assert_eq!(
        parse_present_mode(" auto-no-vsync "),
        Some(PresentMode::AutoNoVsync)
    );
    assert_eq!(
        parse_present_mode("fifo_relaxed"),
        Some(PresentMode::FifoRelaxed)
    );
    assert_eq!(parse_present_mode("vsync"), None);
}

const ALL_MODES: &[PresentMode] = &[
    PresentMode::Mailbox,
    PresentMode::Fifo,
    PresentMode::Immediate,
];
const FIFO_ONLY: &[PresentMode] = &[PresentMode::Fifo];

#[test]
fn mailbox_is_the_default_where_supported() {
    assert_eq!(default_present_mode(ALL_MODES), PresentMode::Mailbox);
    assert_eq!(choose_present_mode(None, ALL_MODES), PresentMode::Mailbox);
}

#[test]
fn without_mailbox_the_default_is_auto_vsync() {
    assert_eq!(default_present_mode(FIFO_ONLY), PresentMode::AutoVsync);
    assert_eq!(choose_present_mode(None, FIFO_ONLY), PresentMode::AutoVsync);
}

#[test]
fn a_supported_requested_mode_wins_over_the_default() {
    assert_eq!(
        choose_present_mode(Some("fifo"), ALL_MODES),
        PresentMode::Fifo
    );
    assert_eq!(
        choose_present_mode(Some("immediate"), ALL_MODES),
        PresentMode::Immediate
    );
    // Auto modes resolve themselves on any surface.
    assert_eq!(
        choose_present_mode(Some("auto_vsync"), ALL_MODES),
        PresentMode::AutoVsync
    );
}

#[test]
fn an_unusable_requested_mode_falls_back_to_the_default() {
    assert_eq!(
        choose_present_mode(Some("immediate"), FIFO_ONLY),
        PresentMode::AutoVsync
    );
    assert_eq!(
        choose_present_mode(Some("vsync"), ALL_MODES),
        PresentMode::Mailbox
    );
}

#[test]
fn frame_latency_is_one_to_three() {
    assert_eq!(parse_frame_latency("1"), Some(1));
    assert_eq!(parse_frame_latency(" 3 "), Some(3));
    assert_eq!(parse_frame_latency("0"), None);
    assert_eq!(parse_frame_latency("4"), None);
    assert_eq!(parse_frame_latency("two"), None);
}
