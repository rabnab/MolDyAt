// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The interaction table (#425): which species pairs get which interactions.

use crate::objects2d::ball_impl::interaction::{Model, pair_interactions};
use crate::objects2d::ball_impl::pair_potential::CapSettings;
use crate::objects2d::ball_impl::species::SpeciesTable;

/// Two water species with different hydrogen-bond parameters get no bond
/// between them: a bond with one side's parameters would not be symmetric.
/// Equal parameters bond, molecules get site pairs, atoms get nothing.
#[test]
fn hydrogen_bonds_only_between_equal_water_parameters() {
    let builtin = SpeciesTable::builtin();
    let water = builtin.species(builtin.by_name("H2O").unwrap()).clone();
    let mut other = water.clone();
    other.name = "H2O_b".into();
    if let Model::MbWater(w) = &mut other.model {
        w.epsilon_hb *= 2.0;
    }
    let mut twin = water.clone();
    twin.name = "H2O_c".into();
    let ar = builtin.species(builtin.by_name("Ar").unwrap()).clone();
    let n2 = builtin.species(builtin.by_name("N2_2S").unwrap()).clone();
    let table = SpeciesTable::new(vec![water, other, twin, ar, n2]);
    let count = pair_interactions(&table, CapSettings::default())
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>();
    let n = table.len();
    let at = |a: usize, b: usize| count[a * n + b];
    assert_eq!((at(0, 0), at(1, 1), at(0, 2), at(2, 0)), (1, 1, 1, 1));
    assert_eq!((at(0, 1), at(1, 0), at(1, 2)), (0, 0, 0));
    assert_eq!((at(3, 3), at(0, 3)), (0, 0));
    assert_eq!((at(4, 4), at(3, 4), at(0, 4)), (1, 1, 1));
}
