// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::DisplayMode;
use crate::graphics::energy_plot::EnergyData;
use crate::graphics::energy_plot::energy_renderer::*;
use std::sync::{Arc, Mutex};

#[test]
fn test_renderer_new() {
    let energy_data = Arc::new(Mutex::new(EnergyData::new()));
    let renderer = EnergyPlotRenderer::new(energy_data);
    let (output, _) = renderer.render_to_colored_string(None, DisplayMode::All);

    // Should show waiting message when no data
    assert!(output.contains("Waiting for simulation data"));
    assert!(output.contains("ENERGY PLOT"));
}

#[test]
fn test_render_with_data() {
    let energy_data = Arc::new(Mutex::new(EnergyData::new()));

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
    let (output, _) = renderer.render_to_colored_string(None, DisplayMode::All);

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
    let energy_data = Arc::new(Mutex::new(EnergyData::new()));

    {
        let mut data = energy_data.lock().unwrap();
        data.v_kin.push_back(10.0);
        data.time = 0.5;
    }

    let renderer = EnergyPlotRenderer::new(energy_data);
    let (output, _) = renderer.render_to_colored_string(None, DisplayMode::Kin);

    assert!(output.contains("Time: 0.50"));
    assert!(!output.contains("Waiting for simulation data"));
}

#[test]
fn test_render_only_potential() {
    let energy_data = Arc::new(Mutex::new(EnergyData::new()));

    {
        let mut data = energy_data.lock().unwrap();
        data.v_pot.push_back(5.0);
        data.time = 0.5;
    }

    let renderer = EnergyPlotRenderer::new(energy_data);
    let (output, _) = renderer.render_to_colored_string(None, DisplayMode::Pot);

    assert!(output.contains("Time: 0.50"));
    assert!(!output.contains("Waiting for simulation data"));
}

#[test]
fn test_waiting_message_format() {
    let energy_data = Arc::new(Mutex::new(EnergyData::new()));
    let renderer = EnergyPlotRenderer::new(energy_data);
    let (output, _) = renderer.render_to_colored_string(None, DisplayMode::All);

    assert!(output.contains("═══════════════════════"));
    assert!(output.contains("Add some balls to see the energy plot!"));
    assert!(output.contains("Press F5 in the main window"));
}

#[test]
fn test_render_with_status_text() {
    let energy_data = Arc::new(Mutex::new(EnergyData::new()));

    {
        let mut data = energy_data.lock().unwrap();
        data.v_kin.push_back(10.0);
        data.v_pot.push_back(5.0);
        data.time = 1.0;
    }

    let renderer = EnergyPlotRenderer::new(energy_data);
    let status = "fps 60 time: 1.0\n#Balls: 100";
    let (output, _) = renderer.render_to_colored_string(Some(status), DisplayMode::All);

    assert!(output.contains("fps 60"));
    assert!(output.contains("#Balls: 100"));
    assert!(output.contains("ENERGY PLOT"));
}
