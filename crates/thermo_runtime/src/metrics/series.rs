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
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a new value, discarding the oldest if at capacity.
    pub fn push(&mut self, value: T) {
        if self.data.len() >= self.capacity {
            self.data.pop_front();
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
