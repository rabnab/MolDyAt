// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::energy_data::{DisplayMode, SharedEnergyData};

const DEFAULT_PLOT_WIDTH: usize = 80;
const DEFAULT_PLOT_HEIGHT: usize = 30;
const HEADER_LINES: usize = 4; // Title + axis label + spacing
const FOOTER_LINES: usize = 7; // Legend + key hints + spacing

// Color scheme (ARGB format) - improved for better readability
pub const COLOR_KINETIC: u32 = 0xFF4DCC4D; // Bright green for kinetic energy
pub const COLOR_POTENTIAL: u32 = 0xFFCC4D4D; // Bright red for potential energy
pub const COLOR_TOTAL: u32 = 0xFF4D99E6; // Bright blue for total energy
pub const COLOR_AXIS: u32 = 0xFF4D4D4D; // Darker gray for axes
pub const COLOR_ZERO_LINE: u32 = 0xFF666666; // Medium gray for zero reference line
pub const COLOR_TEXT: u32 = 0xFFD9D9D9; // Lighter gray for better contrast
pub const COLOR_STATUS_TEXT: u32 = 0xFFA9A9F9; // Lighter gray for better contrast
pub const COLOR_TITLE: u32 = 0xFFE6E6E6; // Soft white for title (less harsh)
pub const COLOR_LEGEND_KIN: u32 = COLOR_KINETIC;
pub const COLOR_LEGEND_POT: u32 = COLOR_POTENTIAL;
pub const COLOR_LEGEND_TOT: u32 = COLOR_TOTAL;

/// Character with associated color for rendering
#[derive(Clone, Copy)]
pub struct ColoredChar {
    pub ch: char,
    pub color: u32,
}

impl ColoredChar {
    pub(super) fn new(ch: char, color: u32) -> Self {
        Self { ch, color }
    }

    pub(super) fn space() -> Self {
        Self::new(' ', COLOR_TEXT)
    }
}

/// Simple text-based renderer for the energy plot window
/// This provides a working implementation while full graphical rendering is being developed
pub struct EnergyPlotRenderer {
    energy_data: SharedEnergyData,
}

impl EnergyPlotRenderer {
    /// Create new energy plot renderer with shared data.
    ///
    /// # Arguments
    /// * `energy_data` - Arc-wrapped Mutex to energy data shared with simulation
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self { energy_data }
    }

    /// Render energy plot with default dimensions.
    /// Uses fixed 80×30 character plot area.
    ///
    /// # Arguments
    /// * `status_text` - Optional status text to display
    /// * `display_mode` - Which energy values to show (All, Kinetic, Potential, Total)
    ///
    /// # Returns
    /// Tuple of (text string, color matrix for each character)
    pub fn render_to_colored_string(
        &self,
        status_text: Option<&str>,
        display_mode: DisplayMode,
    ) -> (String, Vec<Vec<u32>>) {
        self.render_to_colored_string_with_layout(
            status_text,
            display_mode,
            // Left label width (~11) + plot width
            11 + DEFAULT_PLOT_WIDTH,
            // Header + plot height + footer
            HEADER_LINES + DEFAULT_PLOT_HEIGHT + FOOTER_LINES,
        )
    }

    /// Render energy plot sized to fit available terminal dimensions.
    /// Automatically calculates plot dimensions based on window size minus headers/footers.
    /// Places status text below plot.
    ///
    /// # Arguments
    /// * `status_text` - Optional status text to display below plot
    /// * `display_mode` - Which energy values to show (All, Kinetic, Potential, Total)
    /// * `total_cols` - Available terminal width in characters
    /// * `total_rows` - Available terminal height in characters
    ///
    /// # Returns
    /// Tuple of (text string, color matrix for each character)
    pub fn render_to_colored_string_with_layout(
        &self,
        status_text: Option<&str>,
        display_mode: DisplayMode,
        total_cols: usize,
        total_rows: usize,
    ) -> (String, Vec<Vec<u32>>) {
        let data = self.energy_data.lock().unwrap();

        if data.v_kin.is_empty() && data.v_pot.is_empty() {
            return self.render_waiting_message_colored(display_mode);
        }

        // Prepare status lines
        let status_lines: Vec<String> = status_text
            .map(|s| s.lines().map(|l| l.to_string()).collect())
            .unwrap_or_default();

        // Layout constants in characters
        let left_label_width = 11; // e.g. "-1.23e+02 "
        let header_lines = 3; // title, subtitle, title
        let footer_lines = 2; // compact footer: legend+instructions, separator

        // Compute available plot height
        // Dynamically calculate plot height based on window size and status lines
        let status_lines_count = status_lines.len();
        let mut plot_height = if total_rows > header_lines + footer_lines + status_lines_count {
            total_rows - (header_lines + footer_lines + status_lines_count)
        } else {
            6
        };

        // Compute plot width in characters available for the plot area (excluding left label)
        let plot_total_available = if total_cols > left_label_width {
            total_cols - left_label_width
        } else {
            DEFAULT_PLOT_WIDTH
        };

        // Ensure at least a small plot width
        let plot_width = plot_total_available.max(16);
        // Ensure plot height is reasonable
        if plot_height < 6 {
            plot_height = 6;
        }

        // Generate plot lines and colors sized to plot_width x plot_height
        let (plot_lines, plot_colors) = self.render_ascii_plot_with_axes_colored_dims(
            &data.v_kin,
            &data.v_pot,
            display_mode,
            plot_width,
            plot_height,
        );

        // Compose full text lines: header + plot area + footer + status below
        let mut lines: Vec<String> = Vec::new();
        let mut color_lines: Vec<Vec<u32>> = Vec::new();

        // Helper to clamp line and colors to total_cols
        let clamp_to_cols = |mut s: String, mut cols_vec: Vec<u32>| -> (String, Vec<u32>) {
            if s.chars().count() > total_cols {
                s = s.chars().take(total_cols).collect();
            }
            if cols_vec.len() > s.chars().count() {
                cols_vec.truncate(s.chars().count());
            } else if cols_vec.len() < s.chars().count() {
                cols_vec.extend(vec![COLOR_TEXT; s.chars().count() - cols_vec.len()]);
            }
            (s, cols_vec)
        };

        // Title block
        let title_line =
            "═══════════════════════════════════════════════════════════════════════════════════";
        let subtitle = format!(
            "                         ENERGY PLOT 🐾 — {}",
            display_mode.as_str()
        );
        let (t0, c0) = clamp_to_cols(
            title_line.to_string(),
            vec![COLOR_TITLE; title_line.chars().count()],
        );
        lines.push(t0);
        color_lines.push(c0);
        let (t1, c1) = clamp_to_cols(
            subtitle.clone(),
            vec![COLOR_TITLE; subtitle.chars().count()],
        );
        lines.push(t1);
        color_lines.push(c1);
        let (t2, c2) = clamp_to_cols(
            title_line.to_string(),
            vec![COLOR_TITLE; title_line.chars().count()],
        );
        lines.push(t2);
        color_lines.push(c2);

        // Add plot lines (no status column)
        for (i, plot_line) in plot_lines.iter().enumerate() {
            let mut full_line = String::new();
            let mut colors = Vec::new();
            full_line.push_str(plot_line);
            if let Some(pc) = plot_colors.get(i) {
                colors.extend(pc.iter().copied());
            }
            let (clamped_line, clamped_colors) = clamp_to_cols(full_line, colors);
            lines.push(clamped_line);
            color_lines.push(clamped_colors);
        }

        // Compact footer: legend and instructions on one line, then separator
        let (legend_line, legend_colors) = self.create_legend_line(display_mode, data.time);
        let instructions = " | Esc: close | Tab: mode ";
        let footer = format!("{}{}", legend_line, instructions);
        let (footer_s, footer_c) = clamp_to_cols(
            footer,
            legend_colors
                .iter()
                .cloned()
                .chain(vec![COLOR_TEXT; instructions.len()])
                .collect(),
        );
        lines.push(footer_s);
        color_lines.push(footer_c);
        let separator =
            "───────────────────────────────────────────────────────────────────────────────────";
        let (sep_s, sep_c) = clamp_to_cols(
            separator.to_string(),
            vec![COLOR_TEXT; separator.chars().count()],
        );
        lines.push(sep_s);
        color_lines.push(sep_c);

        // Add status lines below everything else, with smaller font (if supported) or color separation
        if !status_lines.is_empty() {
            for s in &status_lines {
                // Use a different color for status lines to visually separate (simulate smaller font)
                let (clamped, clamped_c) =
                    clamp_to_cols(s.clone(), vec![COLOR_STATUS_TEXT; s.len()]);
                lines.push(clamped);
                color_lines.push(clamped_c);
            }
        }

        let output = lines.join("\n");
        (output, color_lines)
    }

    fn create_legend_line(&self, display_mode: DisplayMode, time: f32) -> (String, Vec<u32>) {
        let mut line = String::from("  ");
        let mut colors = vec![COLOR_TEXT; 2];

        match display_mode {
            DisplayMode::Pot => {
                line.push_str("Epot (+)");
                colors.extend(vec![COLOR_POTENTIAL; 8]);
            }
            DisplayMode::Kin => {
                line.push_str("Ekin (*)");
                colors.extend(vec![COLOR_KINETIC; 8]);
            }
            DisplayMode::Total => {
                line.push_str("Etot (=)");
                colors.extend(vec![COLOR_TOTAL; 8]);
            }
            DisplayMode::All => {
                line.push_str("Ekin (*)");
                colors.extend(vec![COLOR_KINETIC; 8]);
                line.push_str("  ");
                colors.extend(vec![COLOR_TEXT; 2]);
                line.push_str("Epot (+)");
                colors.extend(vec![COLOR_POTENTIAL; 8]);
                line.push_str("  ");
                colors.extend(vec![COLOR_TEXT; 2]);
                line.push_str("Etot (=)");
                colors.extend(vec![COLOR_TOTAL; 8]);
            }
        };

        let time_str = format!("  |  Time: {:.2}", time);
        colors.extend(vec![COLOR_TEXT; time_str.len()]);
        line.push_str(&time_str);

        (line, colors)
    }

    fn render_waiting_message_colored(&self, display_mode: DisplayMode) -> (String, Vec<Vec<u32>>) {
        let mut lines = Vec::new();
        let mut color_lines = Vec::new();

        let border =
            "═══════════════════════════════════════════════════════════════════════════════════";
        lines.push(border.to_string());
        color_lines.push(vec![COLOR_TITLE; border.len()]);

        let subtitle = format!(
            "                         ENERGY PLOT 🐾 — {}",
            display_mode.as_str()
        );
        lines.push(subtitle.clone());
        color_lines.push(vec![COLOR_TITLE; subtitle.len()]);

        lines.push(border.to_string());
        color_lines.push(vec![COLOR_TITLE; border.len()]);

        lines.push(String::new());
        color_lines.push(vec![]);

        let msg1 = "                        Waiting for simulation data...";
        lines.push(msg1.to_string());
        color_lines.push(vec![COLOR_TEXT; msg1.len()]);

        lines.push(String::new());
        color_lines.push(vec![]);

        let msg2 = "                  Add some balls to see the energy plot!";
        lines.push(msg2.to_string());
        color_lines.push(vec![COLOR_TEXT; msg2.len()]);

        let msg3 = "                       (Press F5 in the main window)";
        lines.push(msg3.to_string());
        color_lines.push(vec![COLOR_TEXT; msg3.len()]);

        lines.push(String::new());
        color_lines.push(vec![]);

        lines.push(border.to_string());
        color_lines.push(vec![COLOR_TITLE; border.len()]);

        let output = lines.join("\n");
        (output, color_lines)
    }
}
