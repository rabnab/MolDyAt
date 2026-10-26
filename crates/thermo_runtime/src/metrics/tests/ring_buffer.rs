// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

#[test]
fn test_ring_buffer_new() {
    let buf: RingBuffer<f32> = RingBuffer::new(100);
    assert_eq!(buf.capacity(), 100);
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
}

#[test]
fn test_ring_buffer_push_and_len() {
    let mut buf: RingBuffer<f32> = RingBuffer::new(3);
    buf.push(1.0);
    buf.push(2.0);
    assert_eq!(buf.len(), 2);
    assert_eq!(buf.latest(), Some(&2.0));
}

#[test]
fn test_ring_buffer_evicts_oldest() {
    let mut buf: RingBuffer<f32> = RingBuffer::new(3);
    for v in [1.0, 2.0, 3.0, 4.0] {
        buf.push(v);
    }
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.latest(), Some(&4.0));
    assert_eq!(buf.get()[0], 2.0);
    assert_eq!(buf.get()[2], 4.0);
}

#[test]
fn test_ring_buffer_latest_empty() {
    let buf: RingBuffer<f32> = RingBuffer::new(10);
    assert_eq!(buf.latest(), None);
}

#[test]
fn test_ring_buffer_clear() {
    let mut buf: RingBuffer<f32> = RingBuffer::new(5);
    buf.push(1.0);
    buf.push(2.0);
    buf.clear();
    assert!(buf.is_empty());
}

#[test]
fn test_ring_buffer_default() {
    let buf: RingBuffer<i32> = RingBuffer::default();
    assert_eq!(buf.capacity(), 1000);
}
