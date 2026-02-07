//! ggez-specific [`Batchable`] implementations and flush helpers.
//!
//! Provides [`TailSegment`] (particle trails, fixed yellow) and flush
//! functions that drain a [`BatchRenderer`] into a ggez `Mesh`.
//!
//! [`LineSegment`] lives in the shared [`batch_renderer`] module so
//! that both ggez and wgpu can use the same type.

use crate::application::AppResult;
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::batch_renderer::{BatchRenderer, Batchable, LineSegment};
use ggez::Context;
use ggez::graphics::{Canvas, Color, DrawParam, Mesh, MeshBuilder};
use ggez::mint::Point2;

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
/// Applies the viewport transform to convert world coordinates to
/// pixel coordinates.  Drains the batch so it is empty for the next frame.
pub fn flush_line_batch(
    batch: &mut BatchRenderer<LineSegment>,
    viewport: &ViewportTransform,
    ctx: &mut Context,
    canvas: &mut Canvas,
) -> AppResult {
    if batch.is_empty() {
        return Ok(());
    }

    let mut builder = MeshBuilder::new();
    for seg in batch.drain() {
        let start_px = viewport.world_to_px_arr(seg.points[0]);
        let end_px = viewport.world_to_px_arr(seg.points[1]);
        let pts: [Point2<f32>; 2] = [
            Point2 {
                x: start_px[0],
                y: start_px[1],
            },
            Point2 {
                x: end_px[0],
                y: end_px[1],
            },
        ];
        let [r, g, b, a] = seg.color;
        let color = Color::new(r, g, b, a);
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
