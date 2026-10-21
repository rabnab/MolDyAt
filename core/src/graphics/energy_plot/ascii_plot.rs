// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Drawing the energy curves as coloured ASCII: axes, scaling and plotting
//! for [`EnergyPlotRenderer`].

use super::energy_data::DisplayMode;
use super::energy_renderer::*;
use std::collections::VecDeque;

impl EnergyPlotRenderer {
    pub(super) fn render_ascii_plot_with_axes_colored_dims(
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
