// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The scalar values the simulation reports, declared once: a snapshot
//! carries them as one array ([`Values`]); headless frames, the metrics
//! series and the status lines read it by [`Obs`] or iterate
//! [`OBSERVABLES`]. A new scalar is a variant, its line in [`Values::read`]
//! (the compiler asks for it) and an entry of the table.

use crate::application::MainContent;
use crate::objects2d::ball_impl::get_update_hood_call_cnt;
use std::ops::{Index, IndexMut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Gauge,
    /// A count since the start: printed as an integer, and its series holds
    /// the increase since the previous sample (exact in f32).
    Total,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obs {
    Dt,
    KineticEnergy,
    PotentialEnergy,
    EnergyTotal,
    WallMomentum,
    HoodUpdates,
    MaxNeighbours,
    Temperature,
    MaxV,
    VComX,
    VComY,
    PhysicsFps,
    Braked,
    CappedPairs,
    BackstopHits,
    OverlapCorrections,
}

/// Where a value appears besides the status lines.
#[derive(Clone, Copy)]
pub struct Observable {
    pub id: Obs,
    pub kind: Kind,
    /// Key in the headless frame; `group.key` nests it in an object.
    pub headless: Option<&'static str>,
    /// Metrics series: name, description and unit.
    pub series: Option<(&'static str, &'static str, Option<&'static str>)>,
}

use Kind::{Gauge, Total};
const ENERGY: Option<&str> = Some("eps*");
const MOMENTUM: Option<&str> = Some("m*sigma*/tau*");

/// In the order of [`Obs`]. The safety nets (#422) hide instabilities (a
/// time step too coarse for a species, balls pushed into each other): in a
/// healthy run they stay at or near zero.
#[rustfmt::skip]
pub static OBSERVABLES: [Observable; 16] = [
    Observable { id: Obs::Dt, kind: Gauge, headless: Some("dt"), series: None },
    Observable { id: Obs::KineticEnergy, kind: Gauge, headless: Some("kinetic_energy"), series: Some(("E_kin", "Kinetic Energy", ENERGY)) },
    Observable { id: Obs::PotentialEnergy, kind: Gauge, headless: Some("potential_energy"), series: Some(("E_pot", "Potential Energy", ENERGY)) },
    Observable { id: Obs::EnergyTotal, kind: Gauge, headless: None, series: Some(("E_tot", "Total Energy", ENERGY)) },
    Observable { id: Obs::WallMomentum, kind: Gauge, headless: None, series: Some(("wall_momentum", "Wall Momentum", MOMENTUM)) },
    Observable { id: Obs::HoodUpdates, kind: Total, headless: None, series: None },
    Observable { id: Obs::MaxNeighbours, kind: Gauge, headless: None, series: None },
    Observable { id: Obs::Temperature, kind: Gauge, headless: None, series: None },
    Observable { id: Obs::MaxV, kind: Gauge, headless: None, series: None },
    Observable { id: Obs::VComX, kind: Gauge, headless: None, series: None },
    Observable { id: Obs::VComY, kind: Gauge, headless: None, series: None },
    Observable { id: Obs::PhysicsFps, kind: Gauge, headless: Some("physics_fps"), series: None },
    Observable { id: Obs::Braked, kind: Total, headless: Some("safety_nets.braked"), series: Some(("net_braked", "Balls Braked (per sample)", None)) },
    Observable { id: Obs::CappedPairs, kind: Total, headless: Some("safety_nets.capped_pairs"), series: Some(("net_capped_pairs", "Capped Pairs (per sample)", None)) },
    Observable { id: Obs::BackstopHits, kind: Total, headless: Some("safety_nets.backstop_hits"), series: Some(("net_backstop_hits", "Backstop Hits (per sample)", None)) },
    Observable { id: Obs::OverlapCorrections, kind: Total, headless: Some("safety_nets.overlap_corrections"), series: Some(("net_overlap_corrections", "Overlap Corrections (per sample)", None)) },
];

/// The values of [`OBSERVABLES`] at one moment, indexed by [`Obs`]. All in
/// f64: f32 values round-trip exactly and counts stay exact up to 2^53; an
/// output narrows to what it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Values([f64; OBSERVABLES.len()]);

impl Values {
    /// The values of the world now. The `match` is exhaustive, so a new
    /// [`Obs`] without its line here does not compile.
    pub fn read(main: &MainContent) -> Self {
        let sim = &main.world().sim;
        let nets = sim.safety_nets_total;
        let (px, py) = main.wall_momentum();
        Self(OBSERVABLES.map(|o| match o.id {
            Obs::Dt => f64::from(main.active_time_step()),
            Obs::KineticEnergy => f64::from(sim.get_kinetic_energy()),
            Obs::PotentialEnergy => f64::from(sim.get_potential_energy()),
            // In f64 (#421): the headless delta and `E_tot` are formed from it.
            Obs::EnergyTotal => sim.total_energy(),
            Obs::WallMomentum => f64::from(px.hypot(py) as f32),
            Obs::HoodUpdates => get_update_hood_call_cnt() as f64,
            Obs::MaxNeighbours => sim.max_neighbours as f64,
            // Centre-of-mass frame.
            Obs::Temperature => f64::from(sim.temperature),
            Obs::MaxV => f64::from(sim.max_v),
            Obs::VComX => f64::from(sim.v_com.x),
            Obs::VComY => f64::from(sim.v_com.y),
            Obs::PhysicsFps => f64::from(main.runtime().physics_fps()),
            Obs::Braked => nets.braked as f64,
            Obs::CappedPairs => nets.capped_pairs as f64,
            Obs::BackstopHits => nets.backstop_hits as f64,
            Obs::OverlapCorrections => nets.overlap_corrections as f64,
        }))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static Observable, f64)> {
        OBSERVABLES.iter().zip(self.0)
    }
}

impl Index<Obs> for Values {
    type Output = f64;

    fn index(&self, id: Obs) -> &f64 {
        &self.0[id as usize]
    }
}

impl IndexMut<Obs> for Values {
    fn index_mut(&mut self, id: Obs) -> &mut f64 {
        &mut self.0[id as usize]
    }
}
