// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Series types for the MetricsRegistry.

use std::collections::VecDeque;

/// Name of a metric series.
pub type SeriesName = String;

/// Metadata for a metric series.
#[derive(Debug, Clone)]
pub struct SeriesMetadata {
    /// Unit of measurement, if applicable (e.g., "J", "K", "Pa").
    pub unit: Option<&'static str>,
    /// Human-readable description of the series.
    pub description: &'static str,
    /// Whether this is a per-species series.
    pub is_per_species: bool,
}

/// A fixed-capacity ring buffer for time-series data.
///
/// Stores the most recent `capacity` values, discarding oldest when full.
#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    data: VecDeque<T>,
    capacity: usize,
    /// Values pushed since the buffer was made, evicted ones included.
    pushed: u64,
    /// How often the buffer was cleared.
    epoch: u64,
    /// Values pushed out to make room since the buffer was last cleared.
    evicted: u64,
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
            pushed: 0,
            epoch: 0,
            evicted: 0,
        }
    }

    /// Push a new value, discarding the oldest if at capacity. A buffer of
    /// capacity 0 keeps nothing.
    pub fn push(&mut self, value: T) {
        if self.capacity == 0 {
            return;
        }
        self.pushed += 1;
        if self.data.len() >= self.capacity {
            self.data.pop_front();
            self.evicted += 1;
        }
        self.data.push_back(value);
    }

    /// Get a reference to the underlying data.
    pub fn get(&self) -> &VecDeque<T> {
        &self.data
    }

    /// Get the most recent value, if any.
    pub fn latest(&self) -> Option<&T> {
        self.data.back()
    }

    /// Clear all stored values.
    pub fn clear(&mut self) {
        self.data.clear();
        self.epoch += 1;
        self.evicted = 0;
    }

    /// Values pushed so far, evicted ones included. With [`Self::epoch`] it
    /// lets a reader that copied the buffer earlier fetch only what is new:
    /// within one epoch, the last `total_pushed() - seen` values are new.
    pub fn total_pushed(&self) -> u64 {
        self.pushed
    }

    /// Values pushed out since the last [`Self::clear`]: how many of the
    /// samples taken since then are no longer held.
    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    /// How often the buffer was cleared; a reader's copy older than the
    /// current epoch is stale as a whole.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Number of stored values.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Capacity of the buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<T> Default for RingBuffer<T> {
    fn default() -> Self {
        Self::new(1000)
    }
}
