use super::energy_data::{DisplayMode, SharedEnergyData};
use std::collections::VecDeque;

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
    fn new(ch: char, color: u32) -> Self {
        Self { ch, color }
    }

    fn space() -> Self {
        Self::new(' ', COLOR_TEXT)
    }
}

/// Simple text-based renderer for the energy plot window
/// This provides a working implementation while full graphical rendering is being developed
pub struct EnergyPlotRenderer {
    energy_data: SharedEnergyData,
}

impl EnergyPlotRenderer {
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self { energy_data }
    }

    /// Backwards compatible method: use default plot dimensions
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

    /// New layout-aware renderer: total_cols/total_rows are number of text columns/rows available
    /// It will place the status text to the right of the plot and ensure the plot fits into the
    /// visible character grid.
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
                let (clamped, clamped_c) = clamp_to_cols(s.clone(), vec![COLOR_ZERO_LINE; s.len()]);
                lines.push(clamped);
                color_lines.push(clamped_c);
            }
        }

        let output = lines.join("\n");
        (output, color_lines)
    }

    /// Generate a text-based visualization (backward compatibility)
    pub fn render_to_string(&self, status_text: Option<&str>, display_mode: DisplayMode) -> String {
        let (text, _) = self.render_to_colored_string(status_text, display_mode);
        text
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

    fn render_ascii_plot_with_axes_colored_dims(
        &self,
        v_kin: &VecDeque<f32>,
        v_pot: &VecDeque<f32>,
        display_mode: DisplayMode,
        plot_width: usize,
        plot_height: usize,
    ) -> (Vec<String>, Vec<Vec<u32>>) {
        let mut grid = vec![vec![ColoredChar::space(); plot_width]; plot_height];

        // Calculate min/max based on display mode
        let (min_val, max_val) = self.calculate_min_max(v_kin, v_pot, display_mode);
        let range = (max_val - min_val).max(0.001);

        // Draw axes
        for row in grid.iter_mut().take(plot_height) {
            row[0] = ColoredChar::new('│', COLOR_AXIS);
        }
        #[allow(clippy::needless_range_loop)]
        for x in 0..plot_width {
            grid[plot_height - 1][x] = ColoredChar::new('─', COLOR_AXIS);
        }
        grid[plot_height - 1][0] = ColoredChar::new('└', COLOR_AXIS);

        // Calculate zero line position if zero is visible in the plot range (for later use)
        let zero_y_opt = if min_val <= 0.0 && max_val >= 0.0 {
            let normalized = (0.0 - min_val) / range;
            let zero_y = ((1.0 - normalized) * (plot_height - 2) as f32) as usize;
            Some(zero_y.min(plot_height - 2))
        } else {
            None
        };

        // Draw zero reference line BEFORE plotting data (so data can overwrite it)
        if let Some(zero_y) = zero_y_opt {
            #[allow(clippy::needless_range_loop)]
            for x in 1..plot_width {
                grid[zero_y][x] = ColoredChar::new('·', COLOR_ZERO_LINE);
            }
            grid[zero_y][0] = ColoredChar::new('┼', COLOR_ZERO_LINE);
        }

        // Plot based on display mode (these will overwrite the zero line)
        match display_mode {
            DisplayMode::Pot => {
                if !v_pot.is_empty() {
                    self.plot_data_scaled_colored_dims(
                        &mut grid,
                        v_pot,
                        '+',
                        COLOR_POTENTIAL,
                        min_val,
                        range,
                    );
                }
            }
            DisplayMode::Kin => {
                if !v_kin.is_empty() {
                    self.plot_data_scaled_colored_dims(
                        &mut grid,
                        v_kin,
                        '*',
                        COLOR_KINETIC,
                        min_val,
                        range,
                    );
                }
            }
            DisplayMode::Total => {
                if !v_kin.is_empty() && !v_pot.is_empty() {
                    let v_tot: VecDeque<f32> =
                        v_kin.iter().zip(v_pot.iter()).map(|(k, p)| k + p).collect();
                    self.plot_data_scaled_colored_dims(
                        &mut grid,
                        &v_tot,
                        '=',
                        COLOR_TOTAL,
                        min_val,
                        range,
                    );
                }
            }
            DisplayMode::All => {
                if !v_kin.is_empty() {
                    self.plot_data_scaled_colored_dims(
                        &mut grid,
                        v_kin,
                        '*',
                        COLOR_KINETIC,
                        min_val,
                        range,
                    );
                }
                if !v_pot.is_empty() {
                    self.plot_data_scaled_colored_dims(
                        &mut grid,
                        v_pot,
                        '+',
                        COLOR_POTENTIAL,
                        min_val,
                        range,
                    );
                }
                if !v_kin.is_empty() && !v_pot.is_empty() {
                    let v_tot: VecDeque<f32> =
                        v_kin.iter().zip(v_pot.iter()).map(|(k, p)| k + p).collect();
                    self.plot_data_scaled_colored_dims(
                        &mut grid,
                        &v_tot,
                        '=',
                        COLOR_TOTAL,
                        min_val,
                        range,
                    );
                }
            }
        }

        // Convert grid to strings with color information
        let mut text_lines = Vec::new();
        let mut color_lines = Vec::new();

        for (i, row) in grid.iter().enumerate() {
            let mut line = String::new();
            let mut colors = Vec::new();

            // Add Y-axis label on the left (every few rows + zero if visible)
            let label = if i == 0 {
                format!("{:8.2e} ", max_val)
            } else if Some(i) == zero_y_opt {
                // Show zero label at zero line
                "   0.00e+0 ".to_string()
            } else if i == plot_height / 2 && Some(i) != zero_y_opt {
                format!("{:8.2e} ", (max_val + min_val) / 2.0)
            } else if i == plot_height - 1 {
                format!("{:8.2e} ", min_val)
            } else {
                "           ".to_string()
            };

            line.push_str(&label);
            colors.extend(vec![COLOR_TEXT; label.len()]);

            for colored_char in row {
                line.push(colored_char.ch);
                colors.push(colored_char.color);
            }

            text_lines.push(line);
            color_lines.push(colors);
        }

        // Add X-axis labels (single line)
        let data_len = v_kin.len().max(v_pot.len());
        let x_label = format!(
            "           0{:>width$} (samples)",
            data_len,
            width = plot_width - 2
        );
        text_lines.push(x_label.clone());
        color_lines.push(vec![COLOR_TEXT; x_label.len()]);

        (text_lines, color_lines)
    }

    /// Calculate min/max values based on display mode
    fn calculate_min_max(
        &self,
        v_kin: &VecDeque<f32>,
        v_pot: &VecDeque<f32>,
        display_mode: DisplayMode,
    ) -> (f32, f32) {
        let values: Vec<f32> = match display_mode {
            DisplayMode::Pot => v_pot.iter().copied().collect(),
            DisplayMode::Kin => v_kin.iter().copied().collect(),
            DisplayMode::Total => {
                if !v_kin.is_empty() && !v_pot.is_empty() {
                    v_kin.iter().zip(v_pot.iter()).map(|(k, p)| k + p).collect()
                } else {
                    vec![]
                }
            }
            DisplayMode::All => {
                // For ALL mode, include all three traces
                let mut all = Vec::new();
                all.extend(v_kin.iter().copied());
                all.extend(v_pot.iter().copied());
                if !v_kin.is_empty() && !v_pot.is_empty() {
                    all.extend(v_kin.iter().zip(v_pot.iter()).map(|(k, p)| k + p));
                }
                all
            }
        };

        if values.is_empty() {
            return (0.0, 1.0);
        }

        let min_val = values.iter().copied().fold(f32::INFINITY, f32::min);
        let max_val = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        (min_val, max_val)
    }

    fn plot_data_scaled_colored_dims(
        &self,
        grid: &mut [Vec<ColoredChar>],
        data: &VecDeque<f32>,
        symbol: char,
        color: u32,
        min_val: f32,
        range: f32,
    ) {
        if data.is_empty() {
            return;
        }

        let plot_width = grid[0].len();
        let plot_height = grid.len();

        // Calculate how many data points map to each pixel column
        let step = data.len() as f32 / (plot_width - 2) as f32;

        for i in 0..(plot_width - 2) {
            // Find all data points that map to this pixel column
            let start_idx = (i as f32 * step) as usize;
            let end_idx = ((i + 1) as f32 * step).ceil() as usize;
            let end_idx = end_idx.min(data.len());

            if start_idx >= data.len() {
                break;
            }

            // Average all data points that map to this pixel position
            let sum: f32 = data.range(start_idx..end_idx).sum();
            let count = (end_idx - start_idx) as f32;
            let avg_value = sum / count;

            let normalized = (avg_value - min_val) / range;
            let y = ((1.0 - normalized) * (plot_height - 2) as f32) as usize;
            let y = y.min(plot_height - 2);

            let x = i + 1;
            let current_char = grid[y][x].ch;

            // Determine if we should draw at this position
            let should_draw = current_char == ' ' || current_char == '─' || current_char == '·';

            // Check if there's already an energy line here (overlap case)
            let is_energy_overlap = matches!(current_char, '*' | '+' | '=');

            if should_draw {
                // First energy line at this position
                grid[y][x] = ColoredChar::new(symbol, color);
            } else if is_energy_overlap {
                // Overlap detected - alternate between characters based on x position
                // This creates a dashed effect showing both lines
                if x % 2 == 0 {
                    grid[y][x] = ColoredChar::new(symbol, color);
                }
                // else: keep the existing character (alternating pattern)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_renderer_new() {
        let energy_data = Arc::new(Mutex::new(crate::frame::energy_plot::EnergyData::new()));
        let renderer = EnergyPlotRenderer::new(energy_data);
        let output = renderer.render_to_string(None, DisplayMode::All);

        // Should show waiting message when no data
        assert!(output.contains("Waiting for simulation data"));
        assert!(output.contains("ENERGY PLOT"));
    }

    #[test]
    fn test_render_with_data() {
        let energy_data = Arc::new(Mutex::new(crate::frame::energy_plot::EnergyData::new()));

        // Add some energy data
        {
            let mut data = energy_data.lock().unwrap();
            data.v_kin.push_back(10.0);
            data.v_kin.push_back(20.0);
            data.v_kin.push_back(30.0);
            data.v_pot.push_back(5.0);
            data.v_pot.push_back(10.0);
            data.v_pot.push_back(15.0);
            data.time = 1.5;
        }

        let renderer = EnergyPlotRenderer::new(energy_data);
        let output = renderer.render_to_string(None, DisplayMode::All);

        // Should show energy plot
        assert!(output.contains("ENERGY PLOT"));
        assert!(output.contains("Time: 1.50"));
        assert!(output.contains("Ekin (*)"));
        assert!(output.contains("Epot (+)"));
        assert!(output.contains("Etot (=)"));
        assert!(!output.contains("Waiting for simulation data"));
    }

    #[test]
    fn test_render_only_kinetic() {
        let energy_data = Arc::new(Mutex::new(crate::frame::energy_plot::EnergyData::new()));

        {
            let mut data = energy_data.lock().unwrap();
            data.v_kin.push_back(10.0);
            data.time = 0.5;
        }

        let renderer = EnergyPlotRenderer::new(energy_data);
        let output = renderer.render_to_string(None, DisplayMode::Kin);

        assert!(output.contains("Time: 0.50"));
        assert!(!output.contains("Waiting for simulation data"));
    }

    #[test]
    fn test_render_only_potential() {
        let energy_data = Arc::new(Mutex::new(crate::frame::energy_plot::EnergyData::new()));

        {
            let mut data = energy_data.lock().unwrap();
            data.v_pot.push_back(5.0);
            data.time = 0.5;
        }

        let renderer = EnergyPlotRenderer::new(energy_data);
        let output = renderer.render_to_string(None, DisplayMode::Pot);

        assert!(output.contains("Time: 0.50"));
        assert!(!output.contains("Waiting for simulation data"));
    }

    #[test]
    fn test_waiting_message_format() {
        let energy_data = Arc::new(Mutex::new(crate::frame::energy_plot::EnergyData::new()));
        let renderer = EnergyPlotRenderer::new(energy_data);
        let output = renderer.render_to_string(None, DisplayMode::All);

        assert!(output.contains("═══════════════════════"));
        assert!(output.contains("Add some balls to see the energy plot!"));
        assert!(output.contains("Press F5 in the main window"));
    }

    #[test]
    fn test_render_with_status_text() {
        let energy_data = Arc::new(Mutex::new(crate::frame::energy_plot::EnergyData::new()));

        {
            let mut data = energy_data.lock().unwrap();
            data.v_kin.push_back(10.0);
            data.v_pot.push_back(5.0);
            data.time = 1.0;
        }

        let renderer = EnergyPlotRenderer::new(energy_data);
        let status = "fps 60 time: 1.0\n#Balls: 100";
        let output = renderer.render_to_string(Some(status), DisplayMode::All);

        assert!(output.contains("fps 60"));
        assert!(output.contains("#Balls: 100"));
        assert!(output.contains("ENERGY PLOT"));
    }
}
