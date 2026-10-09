// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Renderer state layers — shared by both ggez and wgpu backends.
//!
//! Separates renderer state into three explicit categories:
//!
//! - [`RendererConfig`] — stable across many frames (user toggles,
//!   debug switches).
//! - [`FrameState`] — transient per-frame data, reset between renders.
//! - [`CacheStats`] — GPU-agnostic cache metrics (each backend wraps
//!   its own cache type).
//!
//! This module intentionally contains no ggez or wgpu types so that
//! unit tests can exercise state logic without a GPU context.

use crate::application::scene_layers::SceneLayers;

// ── ThermoDisplayMode ────────────────────────────────────────────────

/// Which thermodynamic field overlay to display.
///
/// Cycled by the `T` key: Off → Grid → Temperature → Pressure → Off.
///
/// - `Off`: no overlay rendered.
/// - `Grid`: plain cell-grid overlay only (no heatmap or value labels).
/// - `Temperature`: thermo heatmap + value labels using temperature field.
/// - `Pressure`: thermo heatmap + value labels using pressure field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThermoDisplayMode {
    #[default]
    Off,
    Grid,
    Temperature,
    Pressure,
}

impl ThermoDisplayMode {
    /// Next variant in the cycle (wraps around).
    pub fn next(self) -> Self {
        let next = match self {
            Self::Off => Self::Grid,
            Self::Grid => Self::Temperature,
            Self::Temperature => Self::Pressure,
            Self::Pressure => Self::Off,
        };
        log::info!("Thermo display: {self:?} → {next:?}");
        next
    }

    /// Returns `true` for modes that render the thermo heatmap and value overlay.
    pub fn is_thermo_active(self) -> bool {
        matches!(self, Self::Temperature | Self::Pressure)
    }
}

// ── RendererConfig ──────────────────────────────────────────────────

/// Stable renderer configuration that persists across frames.
///
/// User-facing toggles and debug switches live here.  Changes are
/// infrequent (keyboard shortcut, UI menu, etc.) and survive frame
/// boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererConfig {
    /// Which energy-plot overlay to display.
    pub show_plot: PlotContent,

    /// Whether the cursor-ball (placement preview) is drawn.
    pub draw_cursor: bool,

    /// Whether rendering is enabled (toggled by F2). Off draws only the base
    /// layer (operators, previews, labels), see [`Self::scene_layers`].
    pub render_drawing: bool,

    /// Which thermodynamic field overlay to display (cycled by `T`).
    pub thermo_display: ThermoDisplayMode,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            show_plot: PlotContent::None,
            draw_cursor: false,
            render_drawing: true,
            thermo_display: ThermoDisplayMode::Off,
        }
    }
}

impl RendererConfig {
    /// Cycle [`show_plot`](Self::show_plot) through all variants.
    pub fn cycle_plot_content(&mut self) {
        self.show_plot = self.show_plot.next();
    }

    /// Enable the cursor-ball overlay.
    ///
    /// `repeated` — if `true` the key is being held; first press is
    /// `false`.  Only the first press enables the cursor.
    pub fn enable_cursor(&mut self, repeated: bool) {
        if !repeated {
            self.draw_cursor = true;
        }
    }

    /// Disable the cursor-ball overlay.
    pub fn disable_cursor(&mut self) {
        self.draw_cursor = false;
    }

    /// Toggle rendering on/off (F2).
    pub fn toggle_rendering(&mut self) {
        self.render_drawing = !self.render_drawing;
    }

    /// Scene layers to build and draw: without the heavy ones while
    /// rendering is off, and without `THERMO` while the overlay shows no
    /// temperature or pressure field.
    pub fn scene_layers(&self) -> SceneLayers {
        if !self.render_drawing {
            SceneLayers::ALL.difference(SceneLayers::HEAVY)
        } else if !self.thermo_display.is_thermo_active() {
            SceneLayers::ALL.difference(SceneLayers::THERMO)
        } else {
            SceneLayers::ALL
        }
    }

    /// Cycle the thermodynamic field overlay (T key).
    pub fn cycle_thermo_display(&mut self) {
        self.thermo_display = self.thermo_display.next();
    }
}

// ── PlotContent ─────────────────────────────────────────────────────

/// Which energy-plot overlay is displayed.
///
/// Shared between renderers so both can query / cycle the same enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotContent {
    None,
    Ekin,
    Epot,
    Etot,
    All,
}

impl PlotContent {
    /// Next variant in the cycle (wraps around).
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Ekin,
            Self::Ekin => Self::Epot,
            Self::Epot => Self::Etot,
            Self::Etot => Self::All,
            Self::All => Self::None,
        }
    }
}

// ── FrameState ──────────────────────────────────────────────────────

/// Transient per-frame UI state (ggez-specific).
///
/// Tracks text overlay positions and cursor formatting for the ggez
/// renderer. The wgpu renderer uses `UnifiedRenderState` directly
/// and doesn't need this structure.
///
/// Call [`reset`](Self::reset) at the start of each frame to advance
/// the frame counter.
#[derive(Debug, Clone)]
pub struct FrameState {
    /// Monotonically increasing frame counter.
    pub frame_id: u64,

    /// Cursor position in pixel coordinates (set by mouse-move).
    pub cursor_pos_px: [f32; 2],

    /// Formatted cursor-position text (lazily updated when cursor
    /// moves a significant distance).
    pub cursor_text: String,

    /// Last pixel position at which `cursor_text` was recomputed.
    /// Used to avoid reformatting on every mouse-move event.
    pub cursor_text_origin_px: [f32; 2],

    /// Position for the ball-status overlay text (layout-dependent).
    pub ball_status_text_pos: Option<[f32; 2]>,
}

impl Default for FrameState {
    fn default() -> Self {
        Self {
            frame_id: 0,
            cursor_pos_px: [0.0, 0.0],
            cursor_text: format!("x: {}, y: {} pos ({:.3},{:.3})", 0., 0., 0., 0.),
            cursor_text_origin_px: [0.0, 0.0],
            ball_status_text_pos: None,
        }
    }
}

impl FrameState {
    /// Advance the frame counter.
    ///
    /// Call at the very start of each render frame.  Cursor position
    /// and status text are *not* cleared — they carry over until the
    /// next mouse-move or explicit update.
    pub fn reset(&mut self) {
        self.frame_id += 1;
    }
}

// ── CacheStats ──────────────────────────────────────────────────────

/// GPU cache statistics (ggez-specific).
///
/// Tracks mesh cache sizes for the ggez renderer. The wgpu renderer
/// manages GPU buffers directly through pipelines and doesn't use
/// this structure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Number of cached text strings.
    pub string_entries: usize,
    /// Number of cached glyph meshes.
    pub glyph_entries: usize,
    /// Number of cached ball-label meshes.
    pub ball_label_entries: usize,
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── RendererConfig tests ────────────────────────────────────────

    #[test]
    fn config_default_is_no_plot_no_cursor() {
        let cfg = RendererConfig::default();
        assert_eq!(cfg.show_plot, PlotContent::None);
        assert!(!cfg.draw_cursor);
        assert!(cfg.render_drawing);
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Off);
    }

    #[test]
    fn config_toggle_rendering() {
        let mut cfg = RendererConfig::default();
        assert!(cfg.render_drawing);
        cfg.toggle_rendering();
        assert!(!cfg.render_drawing);
        cfg.toggle_rendering();
        assert!(cfg.render_drawing);
    }

    #[test]
    fn config_cycle_plot_content_wraps_around() {
        let mut cfg = RendererConfig::default();
        cfg.cycle_plot_content();
        assert_eq!(cfg.show_plot, PlotContent::Ekin);
        cfg.cycle_plot_content();
        assert_eq!(cfg.show_plot, PlotContent::Epot);
        cfg.cycle_plot_content();
        assert_eq!(cfg.show_plot, PlotContent::Etot);
        cfg.cycle_plot_content();
        assert_eq!(cfg.show_plot, PlotContent::All);
        cfg.cycle_plot_content();
        assert_eq!(cfg.show_plot, PlotContent::None);
    }

    #[test]
    fn config_enable_cursor_only_on_first_press() {
        let mut cfg = RendererConfig::default();
        cfg.enable_cursor(false);
        assert!(cfg.draw_cursor);

        cfg.disable_cursor();
        assert!(!cfg.draw_cursor);

        // Repeated press should NOT re-enable
        cfg.enable_cursor(true);
        assert!(!cfg.draw_cursor);
    }

    #[test]
    fn config_changes_persist_across_frames() {
        let mut cfg = RendererConfig::default();
        cfg.cycle_plot_content();
        cfg.enable_cursor(false);

        // Simulate "frame reset" — config should be unaffected
        let snapshot = cfg.clone();
        assert_eq!(snapshot.show_plot, PlotContent::Ekin);
        assert!(snapshot.draw_cursor);
    }

    // ── PlotContent tests ───────────────────────────────────────────

    #[test]
    fn plot_content_next_full_cycle() {
        let start = PlotContent::None;
        let cycle: Vec<PlotContent> = (0..6)
            .scan(start, |state, _| {
                let current = *state;
                *state = state.next();
                Some(current)
            })
            .collect();
        assert_eq!(
            cycle,
            vec![
                PlotContent::None,
                PlotContent::Ekin,
                PlotContent::Epot,
                PlotContent::Etot,
                PlotContent::All,
                PlotContent::None, // wraps
            ]
        );
    }

    // ── FrameState tests ────────────────────────────────────────────

    #[test]
    fn frame_state_default_starts_at_zero() {
        let fs = FrameState::default();
        assert_eq!(fs.frame_id, 0);
        assert_eq!(fs.cursor_pos_px, [0.0, 0.0]);
    }

    #[test]
    fn frame_state_reset_increments_counter() {
        let mut fs = FrameState::default();
        fs.reset();
        assert_eq!(fs.frame_id, 1);
        fs.reset();
        assert_eq!(fs.frame_id, 2);
    }

    #[test]
    fn frame_state_cursor_survives_reset() {
        let mut fs = FrameState {
            cursor_pos_px: [100.0, 200.0],
            ..Default::default()
        };
        fs.reset();
        assert_eq!(fs.cursor_pos_px, [100.0, 200.0]);
    }

    // ── CacheStats tests ────────────────────────────────────────────

    #[test]
    fn cache_stats_default_is_zero() {
        let stats = CacheStats::default();
        assert_eq!(stats.string_entries, 0);
        assert_eq!(stats.glyph_entries, 0);
        assert_eq!(stats.ball_label_entries, 0);
    }

    // ── ThermoDisplayMode tests ─────────────────────────────────────

    #[test]
    fn thermo_display_mode_cycle_wraps_around() {
        let mut cfg = RendererConfig::default();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Off);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Grid);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Temperature);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Pressure);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Off);
    }

    #[test]
    fn thermo_display_mode_two_presses_is_temperature() {
        let mut cfg = RendererConfig::default();
        cfg.cycle_thermo_display();
        cfg.cycle_thermo_display();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Temperature);
    }

    #[test]
    fn thermo_display_mode_grid_is_not_thermo_active() {
        assert!(!ThermoDisplayMode::Off.is_thermo_active());
        assert!(!ThermoDisplayMode::Grid.is_thermo_active());
        assert!(ThermoDisplayMode::Temperature.is_thermo_active());
        assert!(ThermoDisplayMode::Pressure.is_thermo_active());
    }

    #[test]
    fn scene_layers_follow_rendering_and_the_thermo_overlay() {
        let mut cfg = RendererConfig::default();
        let without_thermo = SceneLayers::ALL.difference(SceneLayers::THERMO);
        // T off and Grid show no field.
        assert_eq!(cfg.scene_layers(), without_thermo);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.scene_layers(), without_thermo);
        // Temperature and Pressure do.
        cfg.cycle_thermo_display();
        assert_eq!(cfg.scene_layers(), SceneLayers::ALL);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.scene_layers(), SceneLayers::ALL);
        // F2 off: only the base layer, whatever T shows.
        cfg.toggle_rendering();
        assert_eq!(cfg.scene_layers(), SceneLayers::BASE);
    }
}
