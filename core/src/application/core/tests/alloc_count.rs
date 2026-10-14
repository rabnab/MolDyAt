// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Counts heap allocations per thread, so tests can check that a code path
//! allocates nothing (#228). Installed as the global allocator of core's
//! test binary only; it forwards everything to the system allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // `const` and without a destructor, so touching it never allocates.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

struct CountingAllocator;

fn count() {
    // `try_with` fails only while the thread is being torn down.
    let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
}

// SAFETY: every method forwards to `System` with the caller's arguments
// unchanged, so `System`'s guarantees hold; counting touches only a
// thread-local integer and never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: same contract as `GlobalAlloc::alloc`, passed through.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: same contract as `GlobalAlloc::alloc_zeroed`, passed through.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: same contract as `GlobalAlloc::realloc`, passed through.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as `GlobalAlloc::dealloc`, passed through.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Allocations (including reallocations) made so far on the calling thread.
pub(super) fn allocations_on_this_thread() -> u64 {
    ALLOCATIONS.with(Cell::get)
}
