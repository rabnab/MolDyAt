use super::energy_data::SharedEnergyData;
use std::collections::VecDeque;

const PLOT_WIDTH: usize = 60;
const PLOT_HEIGHT: usize = 20;

/// Simple text-based renderer for the energy plot window
/// This provides a working implementation while full graphical rendering is being developed
pub struct EnergyPlotRenderer {
    energy_data: SharedEnergyData,
}

impl EnergyPlotRenderer {
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self { energy_data }
    }

    /// Generate a text-based visualization of the energy plot
    pub fn render_to_string(&self) -> String {
        let data = self.energy_data.lock().unwrap();

        if data.v_kin.is_empty() && data.v_pot.is_empty() {
            return self.render_waiting_message();
        }

        let mut output = String::new();
        output.push_str("═══════════════════════════════════════════════════════════════\n");
        output.push_str("                        ENERGY PLOT 🐾\n");
        output.push_str("═══════════════════════════════════════════════════════════════\n\n");

        // Render the plot
        output.push_str(&self.render_ascii_plot(&data.v_kin, &data.v_pot));

        output.push('\n');
        output.push_str("───────────────────────────────────────────────────────────────\n");
        output.push_str(&format!(
            "  Ekin (*)  Epot (+)  Etot (=)  |  Time: {:.2}\n",
            data.time
        ));
        output.push_str("───────────────────────────────────────────────────────────────\n");
        output.push_str("\n  Press Esc or close window to hide | Tab to return to main\n");
        output.push_str("═══════════════════════════════════════════════════════════════\n");

        output
    }

    fn render_waiting_message(&self) -> String {
        let mut output = String::new();
        output.push_str("═══════════════════════════════════════════════════════════════\n");
        output.push_str("                        ENERGY PLOT 🐾\n");
        output.push_str("═══════════════════════════════════════════════════════════════\n\n");
        output.push_str("                 Waiting for simulation data...\n\n");
        output.push_str("           Add some balls to see the energy plot!\n");
        output.push_str("                (Press F5 in the main window)\n\n");
        output.push_str("═══════════════════════════════════════════════════════════════\n");
        output
    }

    fn render_ascii_plot(&self, v_kin: &VecDeque<f32>, v_pot: &VecDeque<f32>) -> String {
        let mut grid = vec![vec![' '; PLOT_WIDTH]; PLOT_HEIGHT];

        // Draw axes
        for row in grid.iter_mut().take(PLOT_HEIGHT) {
            row[0] = '│';
        }
        #[allow(clippy::needless_range_loop)]
        for x in 0..PLOT_WIDTH {
            grid[PLOT_HEIGHT - 1][x] = '─';
        }
        grid[PLOT_HEIGHT - 1][0] = '└';

        // Plot kinetic energy
        if !v_kin.is_empty() {
            self.plot_data(&mut grid, v_kin, '*');
        }

        // Plot potential energy
        if !v_pot.is_empty() {
            self.plot_data(&mut grid, v_pot, '+');
        }

        // Plot total energy
        if !v_kin.is_empty() && !v_pot.is_empty() {
            let v_tot: VecDeque<f32> = v_kin.iter().zip(v_pot.iter()).map(|(k, p)| k + p).collect();
            self.plot_data(&mut grid, &v_tot, '=');
        }

        // Convert grid to string
        let mut output = String::new();
        for row in &grid {
            output.push_str("  ");
            for &ch in row {
                output.push(ch);
            }
            output.push('\n');
        }

        output
    }

    fn plot_data(&self, grid: &mut [Vec<char>], data: &VecDeque<f32>, symbol: char) {
        if data.is_empty() {
            return;
        }

        // Find min/max for scaling
        let min_val = data.iter().copied().fold(f32::INFINITY, f32::min);
        let max_val = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let range = (max_val - min_val).max(0.001);

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
        let output = renderer.render_to_string();

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
        let output = renderer.render_to_string();

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
        let output = renderer.render_to_string();

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
        let output = renderer.render_to_string();

        assert!(output.contains("Time: 0.50"));
        assert!(!output.contains("Waiting for simulation data"));
    }

    #[test]
    fn test_waiting_message_format() {
        let energy_data = Arc::new(Mutex::new(crate::frame::energy_plot::EnergyData::new()));
        let renderer = EnergyPlotRenderer::new(energy_data);
        let output = renderer.render_to_string();

        assert!(output.contains("═══════════════════════"));
        assert!(output.contains("Add some balls to see the energy plot!"));
        assert!(output.contains("Press F5 in the main window"));
    }
}
