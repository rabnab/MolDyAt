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

// ── Batchable trait ─────────────────────────────────────────────────

/// Data that can be collected into a draw batch.
///
/// Implementors define a [`BatchKey`](Batchable::BatchKey) that
/// identifies which items can be merged into a single draw call (e.g.
/// same colour, same line width).  The key is informational for now —
/// future optimisations may group items by key before flushing.
pub trait Batchable {
    /// Grouping criterion (colour, width, material, …).
    ///
    /// Items with the same key *could* be merged into one draw call by
    /// an optimising flush implementation.
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
    fn batch_key_grouping() {
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
