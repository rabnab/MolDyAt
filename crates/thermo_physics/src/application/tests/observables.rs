// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The table of reported values (#440).

use crate::application::observables::{Kind, OBSERVABLES, Obs, Values};
use crate::test_support::create_main_content;
use std::collections::HashSet;

#[test]
fn table_is_in_the_order_of_the_ids() {
    for (index, observable) in OBSERVABLES.iter().enumerate() {
        assert_eq!(observable.id as usize, index, "{:?}", observable.id);
    }
    assert_eq!(Obs::OverlapCorrections as usize, OBSERVABLES.len() - 1);
}

#[test]
fn output_names_are_unique() {
    let headless: Vec<_> = OBSERVABLES.iter().filter_map(|o| o.headless).collect();
    let series: Vec<_> = OBSERVABLES
        .iter()
        .filter_map(|o| o.series.map(|(name, ..)| name))
        .collect();
    assert_eq!(
        headless.len(),
        headless.iter().collect::<HashSet<_>>().len()
    );
    assert_eq!(series.len(), series.iter().collect::<HashSet<_>>().len());
}

#[test]
fn totals_only_count_up() {
    let (mut content, _) = create_main_content();
    content.add_random_candidates(20);
    let mut previous = Values::default();
    for _ in 0..5 {
        content.simulation_step().unwrap();
        let values = Values::read(&content);
        for (observable, value) in values.iter() {
            if observable.kind == Kind::Total {
                assert!(value >= previous[observable.id], "{:?}", observable.id);
            }
        }
        previous = values;
    }
}

#[test]
fn read_matches_the_accessors_of_the_world() {
    let (mut content, _) = create_main_content();
    content.add_random_candidates(20);
    content.simulation_step().unwrap();
    let values = Values::read(&content);

    let sim = &content.world().sim;
    assert_eq!(
        values[Obs::KineticEnergy],
        f64::from(sim.get_kinetic_energy())
    );
    assert_eq!(
        values[Obs::PotentialEnergy],
        f64::from(sim.get_potential_energy())
    );
    assert_eq!(values[Obs::EnergyTotal], sim.total_energy());
    assert_eq!(values[Obs::Dt], f64::from(content.active_time_step()));
    assert_eq!(values[Obs::Braked], sim.safety_nets_total.braked as f64);
}
