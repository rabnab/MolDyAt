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

use std::borrow::Cow;
use thermo_physics::application::thermo_field::{CellFields, ThermoGrid};
use thermo_runtime::application::scene_layers::SceneLayers;

// ── Field overlays ───────────────────────────────────────────────────

/// A scalar per-cell field the heatmap and the value labels can show
/// (#342). All come from the thermo grid's [`CellFields`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarField {
    Temperature,
    Pressure,
    /// Balls per unit area.
    Density,
    /// `|v_com|`, the length of the flow arrows.
    FlowSpeed,
}

/// How the heatmap shows one [`ScalarField`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalarFieldDescriptor {
    pub field: ScalarField,
    pub name: &'static str,
    /// Values mapped to the ends of the colour scale (reduced units).
    pub range: (f32, f32),
}

/// Every scalar field, in the order `T` cycles through them. A new field
/// is one entry here plus its [`ScalarField::grid`] arm.
pub const SCALAR_FIELDS: &[ScalarFieldDescriptor] = &[
    ScalarFieldDescriptor {
        field: ScalarField::Temperature,
        name: "Temperature",
        range: (0.0, 5.0),
    },
    ScalarFieldDescriptor {
        field: ScalarField::Pressure,
        name: "Pressure",
        range: (-4.0, 4.0),
    },
    ScalarFieldDescriptor {
        field: ScalarField::Density,
        name: "Density",
        // A dense LJ liquid has about 0.8 balls per σ².
        range: (0.0, 1.0),
    },
    ScalarFieldDescriptor {
        field: ScalarField::FlowSpeed,
        name: "Flow speed",
        // The thermal speed at T = 1 is about 1.4.
        range: (0.0, 1.0),
    },
];

impl ScalarField {
    /// The field's descriptor in [`SCALAR_FIELDS`].
    pub fn descriptor(self) -> &'static ScalarFieldDescriptor {
        SCALAR_FIELDS
            .iter()
            .find(|d| d.field == self)
            .expect("every scalar field has a descriptor")
    }

    /// The field's per-cell values.
    pub fn grid(self, cells: &CellFields) -> Cow<'_, ThermoGrid> {
        match self {
            Self::Temperature => Cow::Borrowed(&cells.temperature),
            Self::Pressure => Cow::Borrowed(&cells.pressure),
            Self::Density => Cow::Borrowed(&cells.density),
            Self::FlowSpeed => Cow::Owned(cells.flow_speed()),
        }
    }
}

/// Which field overlay to display.
///
/// Cycled by the `T` key: Off → Grid → every scalar field of
/// [`SCALAR_FIELDS`] → flow arrows → Off.
///
/// - `Off`: no overlay rendered.
/// - `Grid`: plain cell-grid overlay only (no heatmap or value labels).
/// - `Scalar`: heatmap and value labels of one scalar field.
/// - `FlowArrows`: one arrow per cell along its flow velocity (#342).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThermoDisplayMode {
    #[default]
    Off,
    Grid,
    Scalar(ScalarField),
    FlowArrows,
}

impl ThermoDisplayMode {
    /// Next variant in the cycle (wraps around).
    pub fn next(self) -> Self {
        let scalar_at = |index: usize| {
            SCALAR_FIELDS
                .get(index)
                .map_or(Self::FlowArrows, |d| Self::Scalar(d.field))
        };
        let next = match self {
            Self::Off => Self::Grid,
            Self::Grid => scalar_at(0),
            Self::Scalar(field) => {
                let index = SCALAR_FIELDS.iter().position(|d| d.field == field);
                scalar_at(index.map_or(SCALAR_FIELDS.len(), |i| i + 1))
            }
            Self::FlowArrows => Self::Off,
        };
        log::info!("Thermo display: {self:?} → {next:?}");
        next
    }

    /// Returns `true` for modes that need the thermo fields (the `THERMO`
    /// layer): a scalar heatmap or the flow arrows.
    pub fn is_thermo_active(self) -> bool {
        matches!(self, Self::Scalar(_) | Self::FlowArrows)
    }

    /// The scalar field shown, if any.
    pub fn scalar_field(self) -> Option<ScalarField> {
        match self {
            Self::Scalar(field) => Some(field),
            _ => None,
        }
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

    /// Which field overlay to display (cycled by `T`).
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

    /// Cycle the field overlay (T key).
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
        for field in [
            ScalarField::Temperature,
            ScalarField::Pressure,
            ScalarField::Density,
            ScalarField::FlowSpeed,
        ] {
            cfg.cycle_thermo_display();
            assert_eq!(cfg.thermo_display, ThermoDisplayMode::Scalar(field));
        }
        cfg.cycle_thermo_display();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::FlowArrows);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.thermo_display, ThermoDisplayMode::Off);
    }

    #[test]
    fn every_scalar_field_has_one_descriptor_with_a_range() {
        for (i, d) in SCALAR_FIELDS.iter().enumerate() {
            assert_eq!(d.field.descriptor(), d);
            assert!(d.range.0 < d.range.1, "{}", d.name);
            assert!(SCALAR_FIELDS[i + 1..].iter().all(|o| o.field != d.field));
        }
    }

    #[test]
    fn scalar_fields_read_their_grid() {
        let mut cells = CellFields::filled(1, 1, 1.0);
        cells.pressure = ThermoGrid::filled(1, 1, 2.0);
        cells.density = ThermoGrid::filled(1, 1, 0.5);
        cells.velocity_x = ThermoGrid::filled(1, 1, 3.0);
        cells.velocity_y = ThermoGrid::filled(1, 1, 4.0);
        let value = |field: ScalarField| field.grid(&cells).get(0, 0);
        assert_eq!(value(ScalarField::Temperature), 1.0);
        assert_eq!(value(ScalarField::Pressure), 2.0);
        assert_eq!(value(ScalarField::Density), 0.5);
        assert_eq!(value(ScalarField::FlowSpeed), 5.0);
    }

    #[test]
    fn thermo_display_mode_two_presses_is_temperature() {
        let mut cfg = RendererConfig::default();
        cfg.cycle_thermo_display();
        cfg.cycle_thermo_display();
        assert_eq!(
            cfg.thermo_display,
            ThermoDisplayMode::Scalar(ScalarField::Temperature)
        );
    }

    #[test]
    fn thermo_display_mode_grid_is_not_thermo_active() {
        assert!(!ThermoDisplayMode::Off.is_thermo_active());
        assert!(!ThermoDisplayMode::Grid.is_thermo_active());
        assert!(ThermoDisplayMode::Scalar(ScalarField::Temperature).is_thermo_active());
        assert!(ThermoDisplayMode::Scalar(ScalarField::Pressure).is_thermo_active());
        assert!(ThermoDisplayMode::FlowArrows.is_thermo_active());
    }

    #[test]
    fn scene_layers_follow_rendering_and_the_thermo_overlay() {
        let mut cfg = RendererConfig::default();
        let without_thermo = SceneLayers::ALL.difference(SceneLayers::THERMO);
        // T off and Grid show no field.
        assert_eq!(cfg.scene_layers(), without_thermo);
        cfg.cycle_thermo_display();
        assert_eq!(cfg.scene_layers(), without_thermo);
        // Every scalar field and the flow arrows do.
        for _ in 0..=SCALAR_FIELDS.len() {
            cfg.cycle_thermo_display();
            assert_eq!(cfg.scene_layers(), SceneLayers::ALL);
        }
        // F2 off: only the base layer, whatever T shows.
        cfg.toggle_rendering();
        assert_eq!(cfg.scene_layers(), SceneLayers::BASE);
    }
}
