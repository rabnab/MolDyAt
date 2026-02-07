//! ggez-specific [`Batchable`] implementations for line rendering.
//!
//! Provides [`LineSegment`] (neighbour lines with per-segment colour)
//! and [`TailSegment`] (particle trails, fixed yellow), plus helper
//! functions that flush a [`BatchRenderer`] into a ggez `Mesh`.

use crate::application::AppResult;
use crate::graphics::renderer::batch_renderer::{BatchRenderer, Batchable};
use ggez::Context;
use ggez::graphics::{Canvas, Color, DrawParam, Mesh, MeshBuilder};
use ggez::mint::Point2;

// ── LineSegment ─────────────────────────────────────────────────────

/// A coloured line segment suitable for neighbour-line rendering.
#[derive(Clone, Debug)]
pub struct LineSegment {
    /// Start and end points in pixel coordinates.
    pub points: [[f32; 2]; 2],
    /// Stroke width in pixels.
    pub width: f32,
    /// RGBA colour as `(r, g, b, a)` bytes.
    pub color: (u8, u8, u8, u8),
}

impl Batchable for LineSegment {
    /// Lines with the same colour and width can share a mesh.
    ///
    /// Key layout: `(packed_rgba, width_bits)`.
    type BatchKey = (u32, u32);

    fn batch_key(&self) -> Self::BatchKey {
        let (r, g, b, a) = self.color;
        let packed = (r as u32) << 24 | (g as u32) << 16 | (b as u32) << 8 | a as u32;
        (packed, self.width.to_bits())
    }
}

// ── TailSegment ─────────────────────────────────────────────────────

/// A particle-trail line segment (always rendered in yellow).
#[derive(Clone, Debug)]
pub struct TailSegment {
    /// Start and end points in pixel coordinates.
    pub points: [[f32; 2]; 2],
    /// Stroke width in pixels.
    pub width: f32,
}

impl Batchable for TailSegment {
    /// All tail segments share a single batch (uniform colour).
    type BatchKey = ();

    fn batch_key(&self) -> Self::BatchKey {}
}

// ── Flush helpers ───────────────────────────────────────────────────

/// Build a ggez [`Mesh`] from all pending [`LineSegment`]s and draw it.
///
/// Drains the batch so it is empty for the next frame.
pub fn flush_line_batch(
    batch: &mut BatchRenderer<LineSegment>,
    ctx: &mut Context,
    canvas: &mut Canvas,
) -> AppResult {
    if batch.is_empty() {
        return Ok(());
    }

    let mut builder = MeshBuilder::new();
    for seg in batch.drain() {
        let pts: [Point2<f32>; 2] = [
            Point2 {
                x: seg.points[0][0],
                y: seg.points[0][1],
            },
            Point2 {
                x: seg.points[1][0],
                y: seg.points[1][1],
            },
        ];
        let color = Color::from(seg.color);
        builder.line(&pts, seg.width, color)?;
    }
    let mesh = Mesh::from_data(ctx, builder.build());
    canvas.draw(&mesh, DrawParam::default());
    Ok(())
}

/// Build a ggez [`Mesh`] from all pending [`TailSegment`]s and draw it.
///
/// All tails are rendered in [`Color::YELLOW`].  Drains the batch.
pub fn flush_tail_batch(
    batch: &mut BatchRenderer<TailSegment>,
    ctx: &mut Context,
    canvas: &mut Canvas,
) -> AppResult {
    if batch.is_empty() {
        return Ok(());
    }

    let mut builder = MeshBuilder::new();
    for seg in batch.drain() {
        let pts: [Point2<f32>; 2] = [
            Point2 {
                x: seg.points[0][0],
                y: seg.points[0][1],
            },
            Point2 {
                x: seg.points[1][0],
                y: seg.points[1][1],
            },
        ];
        builder.line(&pts, seg.width, Color::YELLOW)?;
    }
    let mesh = Mesh::from_data(ctx, builder.build());
    canvas.draw(&mesh, DrawParam::default());
    Ok(())
}
