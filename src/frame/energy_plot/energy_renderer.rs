use super::energy_data::{DisplayMode, SharedEnergyData};
use std::collections::VecDeque;

const PLOT_WIDTH: usize = 80;
const PLOT_HEIGHT: usize = 30;

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
    pub fn render_to_string(&self, status_text: Option<&str>, display_mode: DisplayMode) -> String {
        let data = self.energy_data.lock().unwrap();

        if data.v_kin.is_empty() && data.v_pot.is_empty() {
            return self.render_waiting_message(display_mode);
        }

        let mut output = String::new();
        output.push_str(
            "═══════════════════════════════════════════════════════════════════════════════════\n",
        );
        output.push_str(&format!(
            "                         ENERGY PLOT 🐾 — {}\n",
            display_mode.as_str()
        ));
        output.push_str("═══════════════════════════════════════════════════════════════════════════════════\n\n");

        // Add status text if provided
        if let Some(status) = status_text {
            output.push_str(status);
            output.push_str("\n\n");
        }

        // Render the plot with axis labels based on display mode
        output.push_str(&self.render_ascii_plot_with_axes(&data.v_kin, &data.v_pot, display_mode));

        output.push('\n');
        output.push_str(
            "───────────────────────────────────────────────────────────────────────────────────\n",
        );

        // Show legend based on mode
        let legend = match display_mode {
            DisplayMode::Pot => "  Epot (+)",
            DisplayMode::Kin => "  Ekin (*)",
            DisplayMode::Total => "  Etot (=)",
            DisplayMode::All => "  Ekin (*)  Epot (+)  Etot (=)",
        };
        output.push_str(&format!("{}  |  Time: {:.2}\n", legend, data.time));
        output.push_str(
            "───────────────────────────────────────────────────────────────────────────────────\n",
        );
        output.push_str("\n  Press Esc to close | Tab to cycle mode\n");
        output.push_str(
            "═══════════════════════════════════════════════════════════════════════════════════\n",
        );

        output
    }

    fn render_waiting_message(&self, display_mode: DisplayMode) -> String {
        let mut output = String::new();
        output.push_str(
            "═══════════════════════════════════════════════════════════════════════════════════\n",
        );
        output.push_str(&format!(
            "                         ENERGY PLOT 🐾 — {}\n",
            display_mode.as_str()
        ));
        output.push_str("═══════════════════════════════════════════════════════════════════════════════════\n\n");
        output.push_str("                        Waiting for simulation data...\n\n");
        output.push_str("                  Add some balls to see the energy plot!\n");
        output.push_str("                       (Press F5 in the main window)\n\n");
        output.push_str(
            "═══════════════════════════════════════════════════════════════════════════════════\n",
        );
        output
    }

    fn render_ascii_plot_with_axes(
        &self,
        v_kin: &VecDeque<f32>,
        v_pot: &VecDeque<f32>,
        display_mode: DisplayMode,
    ) -> String {
        let mut grid = vec![vec![' '; PLOT_WIDTH]; PLOT_HEIGHT];

        // Calculate min/max based on display mode
        let (min_val, max_val) = self.calculate_min_max(v_kin, v_pot, display_mode);
        let range = (max_val - min_val).max(0.001);

        // Draw axes
        for row in grid.iter_mut().take(PLOT_HEIGHT) {
            row[0] = '│';
        }
        #[allow(clippy::needless_range_loop)]
        for x in 0..PLOT_WIDTH {
            grid[PLOT_HEIGHT - 1][x] = '─';
        }
        grid[PLOT_HEIGHT - 1][0] = '└';

        // Plot based on display mode
        match display_mode {
            DisplayMode::Pot => {
                if !v_pot.is_empty() {
                    self.plot_data_scaled(&mut grid, v_pot, '+', min_val, range);
                }
            }
            DisplayMode::Kin => {
                if !v_kin.is_empty() {
                    self.plot_data_scaled(&mut grid, v_kin, '*', min_val, range);
                }
            }
            DisplayMode::Total => {
                if !v_kin.is_empty() && !v_pot.is_empty() {
                    let v_tot: VecDeque<f32> =
                        v_kin.iter().zip(v_pot.iter()).map(|(k, p)| k + p).collect();
                    self.plot_data_scaled(&mut grid, &v_tot, '=', min_val, range);
                }
            }
            DisplayMode::All => {
                // Plot all three traces
                if !v_kin.is_empty() {
                    self.plot_data_scaled(&mut grid, v_kin, '*', min_val, range);
                }
                if !v_pot.is_empty() {
                    self.plot_data_scaled(&mut grid, v_pot, '+', min_val, range);
                }
                if !v_kin.is_empty() && !v_pot.is_empty() {
                    let v_tot: VecDeque<f32> =
                        v_kin.iter().zip(v_pot.iter()).map(|(k, p)| k + p).collect();
                    self.plot_data_scaled(&mut grid, &v_tot, '=', min_val, range);
                }
            }
        }

        // Convert grid to string with Y-axis labels
        let mut output = String::new();
        for (i, row) in grid.iter().enumerate() {
            // Add Y-axis label on the left (every few rows)
            if i == 0 {
                output.push_str(&format!("{:8.2e} ", max_val));
            } else if i == PLOT_HEIGHT / 2 {
                output.push_str(&format!("{:8.2e} ", (max_val + min_val) / 2.0));
            } else if i == PLOT_HEIGHT - 1 {
                output.push_str(&format!("{:8.2e} ", min_val));
            } else {
                output.push_str("           ");
            }

            for &ch in row {
                output.push(ch);
            }
            output.push('\n');
        }

        // Add X-axis labels
        let data_len = v_kin.len().max(v_pot.len());
        output.push_str("           ");
        output.push_str(&format!("0{:>width$}", data_len, width = PLOT_WIDTH - 2));
        output.push_str(" (samples)\n");

        output
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

        // Sample data to fit width
        let step = (data.len() as f32 / (PLOT_WIDTH - 2) as f32).max(1.0);

        for i in 0..(PLOT_WIDTH - 2) {
            let data_idx = (i as f32 * step) as usize;
            if data_idx >= data.len() {
                break;
            }

            let value = data[data_idx];
            let normalized = (value - min_val) / range;
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
