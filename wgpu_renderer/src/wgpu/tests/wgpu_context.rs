use super::super::wgpu_context::PendingSurfaceSize;

#[test]
fn nothing_is_pending_before_the_first_request() {
    let mut pending = PendingSurfaceSize::default();

    assert_eq!(pending.take(), None);
}

#[test]
fn a_burst_of_requests_leaves_only_the_newest_size() {
    let mut pending = PendingSurfaceSize::default();

    pending.request(800, 600);
    pending.request(810, 604);
    pending.request(820, 610);

    assert_eq!(pending.take(), Some((820, 610)));
}

#[test]
fn a_size_is_taken_only_once() {
    let mut pending = PendingSurfaceSize::default();
    pending.request(800, 600);

    assert_eq!(pending.take(), Some((800, 600)));
    assert_eq!(pending.take(), None);
}

#[test]
fn the_same_size_requested_again_is_pending_again() {
    // A lost or outdated surface is reconfigured at an unchanged size.
    let mut pending = PendingSurfaceSize::default();
    pending.request(800, 600);
    pending.take();

    pending.request(800, 600);

    assert_eq!(pending.take(), Some((800, 600)));
}

#[test]
fn an_empty_size_is_ignored() {
    let mut pending = PendingSurfaceSize::default();
    pending.request(0, 600);
    pending.request(800, 0);
    assert_eq!(pending.take(), None);

    pending.request(800, 600);
    pending.request(0, 0);
    assert_eq!(pending.take(), Some((800, 600)));
}
