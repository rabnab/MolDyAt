// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Generic batch-rendering abstraction.
//!
//! Provides [`BatchRenderer`] — a renderer-agnostic collector that
//! accumulates [`Batchable`] items for later flushing by a
//! backend-specific drawing routine.
//!
//! # Usage
//!
//! ```text
//! let mut batch = BatchRenderer::new();
//! batch.push(item);          // accumulate items
//! for item in batch.drain() { /* backend-specific flush */ }
//! ```
//!
//! The `BatchRenderer` intentionally knows nothing about ggez, wgpu, or
//! any other GPU API.  Backend modules implement their own flush logic
//! that consumes the collected items via [`drain`](BatchRenderer::drain).

use std::hash::Hash;

use thermo_runtime::application::renderer_snapshot::BallRenderData;

// ── Shared color constants ──────────────────────────────────────────

/// Cyan: used for main (strongest) neighbor lines.
const CYAN: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
/// Gray: used for secondary neighbor lines with non-zero force.
const GRAY: [f32; 4] = [0.5, 0.5, 0.5, 1.0];
/// Dark gray: used for neighbor lines with zero force.
const DARK_GRAY: [f32; 4] = [0.25, 0.25, 0.25, 1.0];

// ── LineSegment ─────────────────────────────────────────────────────

/// A coloured line segment in **world coordinates**.
///
/// Renderer-agnostic: both ggez and wgpu flush implementations consume
/// these, converting to their own vertex/mesh formats as needed.
#[derive(Clone, Debug)]
pub struct LineSegment {
    /// Start and end points in world coordinates.
    pub points: [[f32; 2]; 2],
    /// Stroke width hint (backends may ignore this, e.g. wgpu LineList).
    pub width: f32,
    /// RGBA colour as `[r, g, b, a]` in `0.0..=1.0`.
    pub color: [f32; 4],
}

impl Batchable for LineSegment {
    /// Lines with the same colour and width can share a draw call.
    ///
    /// Key layout: `(packed_rgba_bits, width_bits)`.
    type BatchKey = (u128, u32);

    fn batch_key(&self) -> Self::BatchKey {
        let [r, g, b, a] = self.color;
        let packed = (r.to_bits() as u128) << 96
            | (g.to_bits() as u128) << 64
            | (b.to_bits() as u128) << 32
            | a.to_bits() as u128;
        (packed, self.width.to_bits())
    }
}

// ── Shared neighbor-line collection ─────────────────────────────────

/// Collect neighbor-line segments for all visible balls into `batch`.
///
/// This is the single source of truth for the neighbor-line logic used
/// by **both** the ggez and wgpu renderers.  Positions are stored in
/// world coordinates; each backend applies its own projection during
/// the flush step.
pub fn collect_neighbor_lines(balls: &[BallRenderData], batch: &mut BatchRenderer<LineSegment>) {
    for ball in balls {
        if !ball.show_neighbor_lines {
            continue;
        }
        collect_lines_for_ball(ball, balls, batch);
    }
}

/// Collect neighbor lines for a single ball.
fn collect_lines_for_ball(
    ball: &BallRenderData,
    all_balls: &[BallRenderData],
    batch: &mut BatchRenderer<LineSegment>,
) {
    let start = [ball.position.x, ball.position.y];

    // Find max force to determine main neighbors
    let max_force = ball
        .neighbors
        .iter()
        .map(|(_, force)| *force)
        .reduce(f32::max)
        .unwrap_or(0.0);

    for (neighbor_idx, force_magnitude) in &ball.neighbors {
        let is_main = *force_magnitude > 0.5 * max_force;

        // Skip non-main neighbors if this ball only shows main neighbors
        if ball.show_main_neighbors_only && !is_main {
            continue;
        }

        // Determine line color based on force magnitude
        let color = if *force_magnitude > 0.0 {
            if is_main { CYAN } else { GRAY }
        } else {
            DARK_GRAY
        };

        if let Some(neighbor) = all_balls.get(*neighbor_idx) {
            let end = [neighbor.position.x, neighbor.position.y];
            batch.push(LineSegment {
                points: [start, end],
                width: 2.0,
                color,
            });
        }
    }
}

// ── Batchable trait ─────────────────────────────────────────────────

/// Data that can be collected into a draw batch.
///
/// Implementors define a [`BatchKey`](Batchable::BatchKey) that
/// identifies which items could be merged into a single draw call
/// (e.g. same colour, same line width).  The key is currently only
/// used for querying — no automatic grouping is performed.
pub trait Batchable {
    /// Grouping criterion (colour, width, material, …).
    ///
    /// Allows callers to distinguish item categories.  The
    /// [`BatchRenderer`] itself does **not** sort or group by key;
    /// backends may use the key in their flush logic if needed.
    type BatchKey: Hash + Eq + Clone;

    /// Return this item's batch key.
    fn batch_key(&self) -> Self::BatchKey;
}

// ── BatchConfig ─────────────────────────────────────────────────────

/// Optional configuration hints for a [`BatchRenderer`].
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Pre-allocation hint for the internal buffer.
    pub initial_capacity: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            initial_capacity: 256,
        }
    }
}

// ── BatchRenderer ───────────────────────────────────────────────────

/// Renderer-agnostic item collector.
///
/// Accumulates [`Batchable`] items and hands them to backend-specific
/// code for drawing.  The renderer itself performs no GPU work.
pub struct BatchRenderer<T: Batchable> {
    pending: Vec<T>,
}

impl<T: Batchable> BatchRenderer<T> {
    /// Create an empty batch with the default capacity hint (256).
    pub fn new() -> Self {
        Self {
            pending: Vec::with_capacity(BatchConfig::default().initial_capacity),
        }
    }

    /// Create an empty batch with a custom [`BatchConfig`].
    pub fn with_config(config: &BatchConfig) -> Self {
        Self {
            pending: Vec::with_capacity(config.initial_capacity),
        }
    }

    /// Append a single item to the batch.
    pub fn push(&mut self, item: T) {
        self.pending.push(item);
    }

    /// Drain all pending items, yielding ownership to the caller.
    ///
    /// After this call the batch is empty and ready for the next frame.
    pub fn drain(&mut self) -> std::vec::Drain<'_, T> {
        self.pending.drain(..)
    }

    /// Discard all pending items without processing them.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Number of items currently pending.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

impl<T: Batchable> Default for BatchRenderer<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal test item.
    #[derive(Clone, Debug, PartialEq)]
    struct TestItem {
        key: u32,
        value: f32,
    }

    impl Batchable for TestItem {
        type BatchKey = u32;
        fn batch_key(&self) -> u32 {
            self.key
        }
    }

    #[test]
    fn new_batch_is_empty() {
        let batch: BatchRenderer<TestItem> = BatchRenderer::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn push_increases_len() {
        let mut batch = BatchRenderer::new();
        batch.push(TestItem { key: 1, value: 1.0 });
        assert_eq!(batch.len(), 1);
        assert!(!batch.is_empty());

        batch.push(TestItem { key: 2, value: 2.0 });
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn clear_empties_batch() {
        let mut batch = BatchRenderer::new();
        batch.push(TestItem { key: 1, value: 1.0 });
        batch.push(TestItem { key: 2, value: 2.0 });
        batch.clear();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn drain_yields_all_items_and_empties() {
        let mut batch = BatchRenderer::new();
        batch.push(TestItem {
            key: 1,
            value: 10.0,
        });
        batch.push(TestItem {
            key: 2,
            value: 20.0,
        });

        let items: Vec<TestItem> = batch.drain().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].value, 10.0);
        assert_eq!(items[1].value, 20.0);
        assert!(batch.is_empty());
    }

    #[test]
    fn batch_key_is_queryable_on_items() {
        let mut batch = BatchRenderer::new();
        batch.push(TestItem { key: 1, value: 1.0 });
        batch.push(TestItem { key: 2, value: 2.0 });
        batch.push(TestItem { key: 1, value: 3.0 });

        let items: Vec<TestItem> = batch.drain().collect();
        let key1: Vec<_> = items.iter().filter(|i| i.batch_key() == 1).collect();
        let key2: Vec<_> = items.iter().filter(|i| i.batch_key() == 2).collect();
        assert_eq!(key1.len(), 2);
        assert_eq!(key2.len(), 1);
    }

    #[test]
    fn with_config_respects_capacity() {
        let config = BatchConfig {
            initial_capacity: 1024,
        };
        let batch: BatchRenderer<TestItem> = BatchRenderer::with_config(&config);
        assert!(batch.is_empty());
    }

    #[test]
    fn default_creates_empty_batch() {
        let batch: BatchRenderer<TestItem> = BatchRenderer::default();
        assert!(batch.is_empty());
    }

    #[test]
    fn reuse_after_drain() {
        let mut batch = BatchRenderer::new();
        batch.push(TestItem { key: 1, value: 1.0 });
        let _: Vec<_> = batch.drain().collect();

        // Reuse for next frame
        batch.push(TestItem { key: 2, value: 2.0 });
        assert_eq!(batch.len(), 1);
        let items: Vec<TestItem> = batch.drain().collect();
        assert_eq!(items[0].key, 2);
    }
}
