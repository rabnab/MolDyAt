// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::renderer::interpolation::fill_nans_with_neighbour_mean;

// ── fill_nans_with_neighbour_mean ────────────────────────────────

#[test]
fn test_fill_nans_keeps_finite_cells() {
    let data = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(fill_nans_with_neighbour_mean(&data, 2, 2), data);
}

#[test]
fn test_fill_nans_single_nan_takes_neighbour_mean() {
    let mut data = vec![10.0; 9];
    data[4] = f32::NAN;
    let out = fill_nans_with_neighbour_mean(&data, 3, 3);
    assert_eq!(out[4], 10.0);
}

#[test]
fn test_fill_nans_propagates_across_grid() {
    // One finite seed in a corner reaches the opposite corner.
    let mut data = vec![f32::NAN; 12];
    data[0] = 7.0;
    let out = fill_nans_with_neighbour_mean(&data, 3, 4);
    assert!(out.iter().all(|&v| v == 7.0), "{out:?}");
}

#[test]
fn test_fill_nans_all_nan_stays_nan() {
    let out = fill_nans_with_neighbour_mean(&[f32::NAN; 9], 3, 3);
    assert!(out.iter().all(|v| v.is_nan()));
}
