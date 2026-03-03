//! Lightweight per-frame metrics collection.
//!
//! Provides [`MetricsCollector`] for renderer-agnostic frame statistics,
//! [`FrameMetrics`] for per-frame data, and [`RollingAverage`] for
//! smoothed FPS display.
//!
//! # Usage
//!
//! ```text
//! begin_frame()           → resets per-frame counters, starts frame timer
//! begin_render()          → alias for begin_frame (split-phase callers)
//! record_draw_call(n)     → increments draw-call / vertex counts
//! record_balls_rendered() → increments ball count
//! end_frame()             → finalises FrameMetrics, pushes FPS to rolling avg
//! average_fps()           → returns smoothed FPS for display
//! metrics()               → latest FrameMetrics (alias for current)
//! debug_stats()           → human-readable metrics summary
//! ```

use std::collections::VecDeque;
use std::ops::{Add, Div, SubAssign};
use std::time::Instant;

// ── RollingAverage ──────────────────────────────────────────────────

/// Circular buffer that maintains the arithmetic mean of the last `N`
/// values pushed.
///
/// Uses a running total for O(1) `push` and `average` calls.
///
/// # Type Parameters
/// * `T` – the numeric type (must support `Add`, `Div`, `SubAssign`, `Copy`,
///   `Default`, and conversion from `usize` via `From<u8>`).
/// * `N` – maximum number of samples to keep (const generic).
pub struct RollingAverage<T, const N: usize> {
    buffer: VecDeque<T>,
    running_total: T,
}

impl<T, const N: usize> RollingAverage<T, N>
where
    T: Add<Output = T> + SubAssign + Div<Output = T> + Copy + Default + From<u8>,
{
    /// Create a new, empty rolling average.
    pub fn new() -> Self {
        const { assert!(N <= 255, "RollingAverage: N must be <= 255 for u8 division") };
        Self {
            buffer: VecDeque::with_capacity(N),
            running_total: T::default(),
        }
    }

    /// Push a new sample.  If the buffer is full the oldest sample is
    /// evicted and subtracted from the running total.
    pub fn push(&mut self, value: T) {
        if self.buffer.len() == N
            && let Some(oldest) = self.buffer.pop_front()
        {
            self.running_total -= oldest;
        }
        self.running_total = self.running_total + value;
        self.buffer.push_back(value);
    }

    /// Return the current arithmetic mean, or `T::default()` (typically
    /// `0.0`) when no samples have been pushed yet.
    pub fn average(&self) -> T {
        let len = self.buffer.len();
        if len == 0 {
            return T::default();
        }
        // Convert len → T via From<u8> (works for f32, f64).
        // We cap N at 255 so a single `u8` suffices; for larger windows
        // a different conversion strategy would be needed.
        self.running_total / T::from(len as u8)
    }

    /// Number of samples currently stored.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear all samples and reset the running total.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.running_total = T::default();
    }
}

impl<T, const N: usize> Default for RollingAverage<T, N>
where
    T: Add<Output = T> + SubAssign + Div<Output = T> + Copy + Default + From<u8>,
{
    fn default() -> Self {
        Self::new()
    }
}

// ── FrameMetrics ────────────────────────────────────────────────────

/// Snapshot of statistics for a single rendered frame.
#[derive(Debug, Clone, Default)]
pub struct FrameMetrics {
    /// Monotonically increasing frame counter.
    pub frame_count: u64,

    /// Wall-clock time for the separation of frames (previous end  → current end), in
    /// milliseconds.
    pub frame_time_ms: f32,

    /// Time spent in the rendering portion of the frame, in
    /// milliseconds.  Currently calcualted by measuring the duration between `begin_frame` and `end_frame`, which captures the time taken to build and submit GPU draw calls.  This is typically much shorter than `frame_time_ms` because GPU command submission is very fast compared to the overall frame rate (e.g. 0.5 ms render time vs 16.7 ms frame time at 60 FPS).
    pub render_time_ms: f32,

    /// Instantaneous FPS derived from `frame_time_ms`.
    pub fps: f32,

    /// Number of GPU draw calls issued during this frame.
    pub draw_calls: u32,

    /// Total vertices submitted this frame.
    pub vertices_rendered: u64,

    /// Number of balls rendered this frame.
    pub balls_rendered: u32,

    /// GPU-side frame time in milliseconds (when available).
    ///
    /// Requires backend-specific GPU timestamp queries; `None` until
    /// a backend populates it.
    pub gpu_time_ms: Option<f32>,
}

// ── MetricsCollector ────────────────────────────────────────────────

/// Renderer-agnostic per-frame metrics collector.
///
/// Lives inside [`super::UnifiedRenderState`] and is used by both ggez
/// and wgpu renderers.
pub struct MetricsCollector {
    /// Latest completed frame metrics.
    current: FrameMetrics,

    /// Rolling FPS average (smoothed over the last 60 frames).
    fps_average: RollingAverage<f32, 60>,

    /// Monotonic frame counter (never reset).
    total_frame_count: u64,

    /// Instant when `begin_frame` was last called (tracks render duration).
    frame_start: Instant,

    /// Instant when `end_frame` was last called (tracks inter-frame rate).
    last_frame_end: Option<Instant>,

    // ── Accumulators (reset each frame) ─────────────────────────────
    pending_draw_calls: u32,
    pending_vertices: u64,
    pending_balls: u32,
}

impl MetricsCollector {
    /// Create a new collector with zeroed counters.
    pub fn new() -> Self {
        Self {
            current: FrameMetrics::default(),
            fps_average: RollingAverage::new(),
            total_frame_count: 0,
            frame_start: Instant::now(),
            last_frame_end: None,
            pending_draw_calls: 0,
            pending_vertices: 0,
            pending_balls: 0,
        }
    }

    // ── Frame lifecycle ─────────────────────────────────────────────

    /// Call at the very start of a frame.
    ///
    /// Resets per-frame accumulators and records the start timestamp.
    pub fn begin_frame(&mut self) {
        self.pending_draw_calls = 0;
        self.pending_vertices = 0;
        self.pending_balls = 0;
        self.frame_start = Instant::now();
    }

    /// Alias for [`begin_frame`](Self::begin_frame).
    ///
    /// Some callers distinguish "begin frame" (snapshot acquisition) from
    /// "begin render" (draw-call timing).  At the `MetricsCollector` level
    /// both map to the same reset-and-start-timer operation.
    pub fn begin_render(&mut self) {
        self.begin_frame();
    }

    /// Call at the end of a frame after all draw calls have been issued.
    ///
    /// Finalises `FrameMetrics`, pushes FPS into the rolling average, and
    /// increments the total frame count.
    ///
    /// `fps` / `frame_time_ms` reflect the **wall-clock rate** at which
    /// frames are produced (time between successive `end_frame` calls).
    /// This correctly captures the physics-snapshot-driven render rate
    /// (~60 Hz) rather than the sub-millisecond GPU command-submission
    /// duration.  `render_time_ms` still records the actual draw-call
    /// duration for GPU workload monitoring.
    pub fn end_frame(&mut self) {
        self.total_frame_count += 1;

        let now = Instant::now();

        // Actual time spent building/submitting draw commands this frame.
        let render_time_ms = self.frame_start.elapsed().as_secs_f32() * 1000.0;

        // Frame rate measured as wall-clock time between successive end_frame
        // calls so FPS reflects how often frames are *produced*, not how fast
        // GPU commands are submitted (which is much faster than real frame rate).
        let (frame_time_ms, fps) = match self.last_frame_end {
            Some(last_end) => {
                let inter_ms = now.saturating_duration_since(last_end).as_secs_f32() * 1000.0;
                let fps = if inter_ms > 0.0 {
                    1000.0 / inter_ms
                } else {
                    0.0
                };
                (inter_ms, fps)
            }
            None => {
                // First frame: no previous timestamp to compare against, so we can't compute a meaningful frame time or FPS yet.  Default to 0.
                let fps = 0.0;
                (render_time_ms, fps)
            }
        };
        self.last_frame_end = Some(now);

        self.fps_average.push(fps);

        self.current = FrameMetrics {
            frame_count: self.total_frame_count,
            frame_time_ms,
            render_time_ms,
            fps,
            draw_calls: self.pending_draw_calls,
            vertices_rendered: self.pending_vertices,
            balls_rendered: self.pending_balls,
            gpu_time_ms: None,
        };
    }

    // ── Recording helpers ───────────────────────────────────────────

    /// Record a draw call with `vertex_count` vertices.
    pub fn record_draw_call(&mut self, vertex_count: u64) {
        self.pending_draw_calls += 1;
        self.pending_vertices += vertex_count;
    }

    /// Record a single ball rendered.
    pub fn record_balls_rendered(&mut self, count: u32) {
        self.pending_balls += count;
    }

    // ── Queries ─────────────────────────────────────────────────────

    /// Smoothed FPS averaged over the last 60 frames.
    pub fn average_fps(&self) -> f32 {
        self.fps_average.average()
    }

    /// The most recent completed [`FrameMetrics`].
    pub fn current(&self) -> &FrameMetrics {
        &self.current
    }

    /// Alias for [`current`](Self::current) matching the PR-specified API.
    pub fn metrics(&self) -> &FrameMetrics {
        &self.current
    }

    /// Return a human-readable summary of the current metrics, useful
    /// for debug overlays or logging.
    pub fn debug_stats(&self) -> String {
        let m = &self.current;
        format!(
            "frame {} | {:.1}ms ({:.0} fps) | draws: {} | verts: {} | balls: {}",
            m.frame_count,
            m.frame_time_ms,
            m.fps,
            m.draw_calls,
            m.vertices_rendered,
            m.balls_rendered,
        )
    }

    /// Total number of frames since the collector was created.
    pub fn total_frame_count(&self) -> u64 {
        self.total_frame_count
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── RollingAverage ──────────────────────────────────────────────

    #[test]
    fn empty_average_returns_zero() {
        let ra: RollingAverage<f32, 4> = RollingAverage::new();
        assert_eq!(ra.average(), 0.0);
        assert!(ra.is_empty());
    }

    #[test]
    fn single_value_is_its_own_average() {
        let mut ra: RollingAverage<f32, 4> = RollingAverage::new();
        ra.push(42.0);
        assert!((ra.average() - 42.0).abs() < f32::EPSILON);
        assert_eq!(ra.len(), 1);
    }

    #[test]
    fn average_of_multiple_values() {
        let mut ra: RollingAverage<f32, 4> = RollingAverage::new();
        ra.push(10.0);
        ra.push(20.0);
        ra.push(30.0);
        // avg = (10+20+30)/3 = 20
        assert!((ra.average() - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn evicts_oldest_when_full() {
        let mut ra: RollingAverage<f32, 3> = RollingAverage::new();
        ra.push(10.0);
        ra.push(20.0);
        ra.push(30.0);
        // Buffer full: [10, 20, 30], avg = 20
        assert!((ra.average() - 20.0).abs() < f32::EPSILON);

        ra.push(40.0);
        // Evicts 10 → [20, 30, 40], avg = 30
        assert!((ra.average() - 30.0).abs() < f32::EPSILON);
        assert_eq!(ra.len(), 3);
    }

    #[test]
    fn clear_resets_everything() {
        let mut ra: RollingAverage<f32, 4> = RollingAverage::new();
        ra.push(100.0);
        ra.push(200.0);
        ra.clear();
        assert!(ra.is_empty());
        assert_eq!(ra.average(), 0.0);
    }

    // ── MetricsCollector ────────────────────────────────────────────

    #[test]
    fn initial_state_is_zeroed() {
        let mc = MetricsCollector::new();
        assert_eq!(mc.total_frame_count(), 0);
        assert_eq!(mc.average_fps(), 0.0);
        let m = mc.current();
        assert_eq!(m.frame_count, 0);
        assert_eq!(m.draw_calls, 0);
        assert_eq!(m.balls_rendered, 0);
    }

    #[test]
    fn end_frame_increments_total_count() {
        let mut mc = MetricsCollector::new();
        mc.begin_frame();
        mc.end_frame();
        assert_eq!(mc.total_frame_count(), 1);

        mc.begin_frame();
        mc.end_frame();
        assert_eq!(mc.total_frame_count(), 2);
    }

    #[test]
    fn recording_accumulates_within_frame() {
        let mut mc = MetricsCollector::new();
        mc.begin_frame();
        mc.record_draw_call(100);
        mc.record_draw_call(50);
        mc.record_balls_rendered(25);
        mc.end_frame();

        let m = mc.current();
        assert_eq!(m.draw_calls, 2);
        assert_eq!(m.vertices_rendered, 150);
        assert_eq!(m.balls_rendered, 25);
    }

    #[test]
    fn begin_frame_resets_accumulators() {
        let mut mc = MetricsCollector::new();
        mc.begin_frame();
        mc.record_draw_call(999);
        mc.record_balls_rendered(42);
        mc.end_frame();

        // New frame should start clean.
        mc.begin_frame();
        mc.end_frame();

        let m = mc.current();
        assert_eq!(m.draw_calls, 0);
        assert_eq!(m.vertices_rendered, 0);
        assert_eq!(m.balls_rendered, 0);
    }

    #[test]
    fn average_fps_smooths_across_frames() {
        let mut mc = MetricsCollector::new();
        // Run a few frames so the rolling average has data.
        for _ in 0..5 {
            mc.begin_frame();
            mc.end_frame();
        }
        // We can't assert an exact FPS value because it depends on
        // wall-clock timing, but it should be non-zero after 5 frames.
        assert!(mc.average_fps() > 0.0);
    }
}
