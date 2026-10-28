// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The status lines of the energy window (`B`), formatted from a
//! snapshot's numbers in the UI (#341): the physics thread sends numbers
//! only, and the text is built when the window shows it.

use std::fmt::Write;
use thermo_physics::application::sim_stats::StatusNumbers;
use thermo_physics::application::units::UnitSystem;
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;

/// Units of the real-units display (key `D`): N₂ (see `units`).
const UNITS: UnitSystem = UnitSystem::N2;

/// The status lines for `snapshot`, starting with `metrics_line` (render
/// and physics rates), or `None` while ball stats are not shown.
pub fn status_text(snapshot: &RendererSnapshot, metrics_line: &str) -> Option<String> {
    let status = snapshot.frame.status.as_ref()?;
    let mut text = if status.real_units {
        real_units_lines(snapshot, status, metrics_line)
    } else {
        reduced_units_lines(snapshot, status, metrics_line)
    };
    text.push('\n');
    write_species_lines(&mut text, snapshot);
    Some(text)
}

fn ball_count(snapshot: &RendererSnapshot) -> usize {
    snapshot.frame.species_counts.iter().sum()
}

/// Status lines in physical units of N₂: time in ps, the time step in fs,
/// temperature in K, energies in J, speeds in m/s.
fn real_units_lines(snapshot: &RendererSnapshot, s: &StatusNumbers, metrics_line: &str) -> String {
    let frame = &snapshot.frame;
    format!(
        "{} | time: {:.1}ps | dt: {:.1}fs\n\
         #Balls: {} | maxNeig: {}\n\
         TotalE: {:.3E}J | Ekin: {:.3E}J\n\
         Epot: {:.3E}J\n\
         Temp: {:.2}K | maxV: {:.2}m/s\n\
         vCOM: ({:.2e}, {:.2e})m/s",
        metrics_line,
        UNITS.to_picoseconds(frame.time),
        UNITS.to_femtoseconds(s.dt),
        ball_count(snapshot),
        s.max_neighbours,
        UNITS.to_joules(frame.total_energy()),
        UNITS.to_joules(frame.kinetic_energy),
        UNITS.to_joules(frame.potential_energy),
        UNITS.to_kelvin(s.temperature),
        UNITS.to_metres_per_second(s.max_v),
        UNITS.to_metres_per_second(s.v_com.x),
        UNITS.to_metres_per_second(s.v_com.y)
    )
}

/// Status lines in reduced units.
fn reduced_units_lines(
    snapshot: &RendererSnapshot,
    s: &StatusNumbers,
    metrics_line: &str,
) -> String {
    let frame = &snapshot.frame;
    format!(
        "{} time: {:.1} (dt:{:.4})\n#Balls: {} #hood: {}\nmaxNeig: {}\n\
            TotalE: {:.3E}\nEkin: {:.3E}\nEpot: {:.3E}\nTemp: {:.3}\nmaxV: {:.2}\nvCOM: {}",
        metrics_line,
        frame.time,
        s.dt,
        ball_count(snapshot),
        s.hood_updates,
        s.max_neighbours,
        frame.total_energy(),
        frame.kinetic_energy,
        frame.potential_energy,
        s.temperature,
        s.max_v,
        s.v_com,
    )
}

/// Status lines for the species (#238): the species of new balls, then
/// one line per species present with its ball count, in table order,
/// e.g. `New: Ar`, `N2: 120`, `Ar: 80`. One line each, so a narrow
/// status pane (the energy window cuts lines at its width) never hides a
/// count.
pub fn write_species_lines(text: &mut String, snapshot: &RendererSnapshot) {
    let table = &snapshot.balls.species;
    let new_name = table
        .get(snapshot.balls.cursor_species)
        .map_or("?", |species| species.name.as_str());
    let _ = write!(text, "New: {new_name}");
    for (id, &count) in table.ids().zip(&snapshot.frame.species_counts) {
        if count > 0 {
            let _ = write!(text, "\n{}: {count}", table.species(id).name);
        }
    }
}
