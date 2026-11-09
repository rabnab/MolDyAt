// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The status lines of the energy window (`B`), formatted from a
//! snapshot's numbers in the UI (#341): the physics thread sends numbers
//! only, and the text is built when the window shows it.

use std::fmt::Write;
use thermo_physics::application::observables::{Obs, Values};
use thermo_physics::application::units::UnitSystem;
use thermo_physics::objects2d::Vec2D;
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;

/// Units of the real-units display (key `D`): the reduced units, Ar (see `units`).
const UNITS: UnitSystem = UnitSystem::REDUCED;

/// The status lines for `snapshot`, starting with `metrics_line` (render
/// and physics rates), or `None` while ball stats are not shown.
pub fn status_text(snapshot: &RendererSnapshot, metrics_line: &str) -> Option<String> {
    let status = snapshot.frame.status.as_ref()?;
    let mut text = if status.real_units {
        real_units_lines(snapshot, metrics_line)
    } else {
        reduced_units_lines(snapshot, metrics_line)
    };
    text.push('\n');
    // Safety nets that do not conserve energy, since the start (#422).
    let v = &snapshot.frame.values;
    let _ = writeln!(
        text,
        "Nets: brake {} cap {} backstop {} overlap {}",
        v[Obs::Braked],
        v[Obs::CappedPairs],
        v[Obs::BackstopHits],
        v[Obs::OverlapCorrections]
    );
    if let Some(id) = status.dt_limited_by {
        let name = snapshot
            .balls
            .species
            .get(id)
            .map_or("?", |species| species.name.as_str());
        let _ = writeln!(text, "dt limited by {name}");
    }
    write_species_lines(&mut text, snapshot);
    Some(text)
}

fn ball_count(snapshot: &RendererSnapshot) -> usize {
    snapshot.frame.species_counts.iter().sum()
}

/// Status lines in physical units (Ar reference): time in ps, the time step in fs,
/// temperature in K, energies in J, speeds in m/s.
fn real_units_lines(snapshot: &RendererSnapshot, metrics_line: &str) -> String {
    let frame = &snapshot.frame;
    let v = &frame.values;
    format!(
        "{} | time: {:.1}ps | dt: {:.1}fs\n\
         #Balls: {} | maxNeig: {}\n\
         TotalE: {:.3E}J | Ekin: {:.3E}J\n\
         Epot: {:.3E}J\n\
         Temp: {:.2}K | maxV: {:.2}m/s\n\
         vCOM: ({:.2e}, {:.2e})m/s",
        metrics_line,
        UNITS.to_picoseconds(frame.time),
        UNITS.to_femtoseconds(f32_of(v, Obs::Dt)),
        ball_count(snapshot),
        v[Obs::MaxNeighbours],
        UNITS.to_joules(f32_of(v, Obs::EnergyTotal)),
        UNITS.to_joules(f32_of(v, Obs::KineticEnergy)),
        UNITS.to_joules(f32_of(v, Obs::PotentialEnergy)),
        UNITS.to_kelvin(f32_of(v, Obs::Temperature)),
        UNITS.to_metres_per_second(f32_of(v, Obs::MaxV)),
        UNITS.to_metres_per_second(f32_of(v, Obs::VComX)),
        UNITS.to_metres_per_second(f32_of(v, Obs::VComY))
    )
}

/// Status lines in reduced units.
fn reduced_units_lines(snapshot: &RendererSnapshot, metrics_line: &str) -> String {
    let frame = &snapshot.frame;
    let v = &frame.values;
    format!(
        "{} time: {:.1} (dt:{:.4})\n#Balls: {} #hood: {}\nmaxNeig: {}\n\
            TotalE: {:.3E}\nEkin: {:.3E}\nEpot: {:.3E}\nTemp: {:.3}\nmaxV: {:.2}\nvCOM: {}",
        metrics_line,
        frame.time,
        f32_of(v, Obs::Dt),
        ball_count(snapshot),
        v[Obs::HoodUpdates],
        v[Obs::MaxNeighbours],
        f32_of(v, Obs::EnergyTotal),
        f32_of(v, Obs::KineticEnergy),
        f32_of(v, Obs::PotentialEnergy),
        f32_of(v, Obs::Temperature),
        f32_of(v, Obs::MaxV),
        Vec2D::new(f32_of(v, Obs::VComX), f32_of(v, Obs::VComY)),
    )
}

fn f32_of(values: &Values, id: Obs) -> f32 {
    values[id] as f32
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
