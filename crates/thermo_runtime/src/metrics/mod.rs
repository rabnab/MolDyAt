// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Metrics registry for time-series data.
//!
//! This module provides a centralized store for all time-series metrics
//! used by UI plots and headless runs. Metrics are stored in ring buffers
//! and sampled at snapshot cadence.

mod registry;
mod series;

pub use registry::{MetricsRegistry, SharedMetricsRegistry};
pub use series::{RingBuffer, SeriesMetadata, SeriesName};
