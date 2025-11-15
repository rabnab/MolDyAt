use super::energy_data::{DisplayMode, SharedEnergyData};
use std::collections::VecDeque;

const PLOT_WIDTH: usize = 80;
const PLOT_HEIGHT: usize = 30;

// Color scheme (ARGB format)
pub const COLOR_KINETIC: u32 = 0xFF00DDFF; // Cyan for kinetic energy
pub const COLOR_POTENTIAL: u32 = 0xFFFF6B00; // Orange for potential energy
pub const COLOR_TOTAL: u32 = 0xFF00FF88; // Green for total energy
pub const COLOR_AXIS: u32 = 0xFF666666; // Gray for axes
pub const COLOR_ZERO_LINE: u32 = 0xFF888888; // Lighter gray for zero reference line
pub const COLOR_TEXT: u32 = 0xFFCCCCCC; // Light gray for text
pub const COLOR_TITLE: u32 = 0xFFFFFFFF; // White for title
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

    /// Generate a text-based visualization of the energy plot with optional status text
    /// Returns tuple of (text, color_map) where color_map contains colors for each character
    pub fn render_to_colored_string(
        &self,
        status_text: Option<&str>,
        display_mode: DisplayMode,
    ) -> (String, Vec<Vec<u32>>) {
        let data = self.energy_data.lock().unwrap();

        if data.v_kin.is_empty() && data.v_pot.is_empty() {
            return self.render_waiting_message_colored(display_mode);
        }

        let mut lines = Vec::new();
        let mut color_lines = Vec::new();

        // Title
        let title_line =
            "═══════════════════════════════════════════════════════════════════════════════════";
        lines.push(title_line.to_string());
        color_lines.push(vec![COLOR_TITLE; title_line.len()]);

        let subtitle = format!(
            "                         ENERGY PLOT 🐾 — {}",
            display_mode.as_str()
        );
        lines.push(subtitle.clone());
        color_lines.push(vec![COLOR_TITLE; subtitle.len()]);

        lines.push(title_line.to_string());
        color_lines.push(vec![COLOR_TITLE; title_line.len()]);

        lines.push(String::new());
        color_lines.push(vec![]);

        // Add status text if provided
        if let Some(status) = status_text {
            for line in status.lines() {
                lines.push(line.to_string());
                color_lines.push(vec![COLOR_TEXT; line.len()]);
            }
            lines.push(String::new());
            color_lines.push(vec![]);
        }

        // Render the plot with colors
        let (plot_lines, plot_colors) =
            self.render_ascii_plot_with_axes_colored(&data.v_kin, &data.v_pot, display_mode);
        lines.extend(plot_lines);
        color_lines.extend(plot_colors);

        lines.push(String::new());
        color_lines.push(vec![]);

        let separator =
            "───────────────────────────────────────────────────────────────────────────────────";
        lines.push(separator.to_string());
        color_lines.push(vec![COLOR_TEXT; separator.len()]);

        // Show legend based on mode with colors
        let (legend_line, legend_colors) = self.create_legend_line(display_mode, data.time);
        lines.push(legend_line);
        color_lines.push(legend_colors);

        lines.push(separator.to_string());
        color_lines.push(vec![COLOR_TEXT; separator.len()]);

        lines.push(String::new());
        color_lines.push(vec![]);

        let instructions = "  Press Esc to close | Tab to cycle mode";
        lines.push(instructions.to_string());
        color_lines.push(vec![COLOR_TEXT; instructions.len()]);

        lines.push(title_line.to_string());
        color_lines.push(vec![COLOR_TITLE; title_line.len()]);

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

    fn render_ascii_plot_with_axes_colored(
        &self,
        v_kin: &VecDeque<f32>,
        v_pot: &VecDeque<f32>,
        display_mode: DisplayMode,
    ) -> (Vec<String>, Vec<Vec<u32>>) {
        let mut grid = vec![vec![ColoredChar::space(); PLOT_WIDTH]; PLOT_HEIGHT];

        // Calculate min/max based on display mode
        let (min_val, max_val) = self.calculate_min_max(v_kin, v_pot, display_mode);
        let range = (max_val - min_val).max(0.001);

        // Draw axes
        for row in grid.iter_mut().take(PLOT_HEIGHT) {
            row[0] = ColoredChar::new('│', COLOR_AXIS);
        }
        #[allow(clippy::needless_range_loop)]
        for x in 0..PLOT_WIDTH {
            grid[PLOT_HEIGHT - 1][x] = ColoredChar::new('─', COLOR_AXIS);
        }
        grid[PLOT_HEIGHT - 1][0] = ColoredChar::new('└', COLOR_AXIS);

        // Calculate zero line position if zero is visible in the plot range (for later use)
        let zero_y_opt = if min_val <= 0.0 && max_val >= 0.0 {
            let normalized = (0.0 - min_val) / range;
            let zero_y = ((1.0 - normalized) * (PLOT_HEIGHT - 2) as f32) as usize;
            Some(zero_y.min(PLOT_HEIGHT - 2))
        } else {
            None
        };

        // Draw zero reference line BEFORE plotting data (so data can overwrite it)
        if let Some(zero_y) = zero_y_opt {
            // Draw horizontal zero line across the plot
            // Note: We need the index `x` for being able to assign values in the grid
            #[allow(clippy::needless_range_loop)]
            for x in 1..PLOT_WIDTH {
                // Only draw on empty spaces (energy lines will overwrite this)
                grid[zero_y][x] = ColoredChar::new('·', COLOR_ZERO_LINE);
            }
            // Add tick mark on Y-axis for zero
            grid[zero_y][0] = ColoredChar::new('┼', COLOR_ZERO_LINE);
        }

        // Plot based on display mode (these will overwrite the zero line)
        match display_mode {
            DisplayMode::Pot => {
                if !v_pot.is_empty() {
                    self.plot_data_scaled_colored(
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
                    self.plot_data_scaled_colored(
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
                    self.plot_data_scaled_colored(
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
                // Plot all three traces with different colors
                if !v_kin.is_empty() {
                    self.plot_data_scaled_colored(
                        &mut grid,
                        v_kin,
                        '*',
                        COLOR_KINETIC,
                        min_val,
                        range,
                    );
                }
                if !v_pot.is_empty() {
                    self.plot_data_scaled_colored(
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
                    self.plot_data_scaled_colored(
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
            } else if i == PLOT_HEIGHT / 2 && Some(i) != zero_y_opt {
                format!("{:8.2e} ", (max_val + min_val) / 2.0)
            } else if i == PLOT_HEIGHT - 1 {
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

        // Add X-axis labels
        let data_len = v_kin.len().max(v_pot.len());
        let x_label = format!(
            "           0{:>width$} (samples)",
            data_len,
            width = PLOT_WIDTH - 2
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

    fn plot_data_scaled_colored(
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

        // Calculate how many data points map to each pixel column
        let step = data.len() as f32 / (PLOT_WIDTH - 2) as f32;

        for i in 0..(PLOT_WIDTH - 2) {
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
            let y = ((1.0 - normalized) * (PLOT_HEIGHT - 2) as f32) as usize;
            let y = y.min(PLOT_HEIGHT - 2);

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

    fn plot_data_scaled(
        &self,
        grid: &mut [Vec<char>],
        data: &VecDeque<f32>,
        symbol: char,
        min_val: f32,
        range: f32,
    ) {
        if data.is_empty() {
            return;
        }

        // Calculate how many data points map to each pixel column
        let step = data.len() as f32 / (PLOT_WIDTH - 2) as f32;

        for i in 0..(PLOT_WIDTH - 2) {
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
            let y = ((1.0 - normalized) * (PLOT_HEIGHT - 2) as f32) as usize;
            let y = y.min(PLOT_HEIGHT - 2);

            let x = i + 1;
            if grid[y][x] == ' ' || grid[y][x] == '─' {
                grid[y][x] = symbol;
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
