// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::{MAX_PLOTS, PRESETS, PlotSelection};

const AVAILABLE: [&str; 6] = [
    "E_kin",
    "E_pot",
    "E_tot",
    "wall_momentum",
    "Ar:count",
    "N2:count",
];

fn series(selection: &PlotSelection) -> Vec<Vec<&str>> {
    selection
        .panels()
        .iter()
        .map(|p| p.series().iter().map(String::as_str).collect())
        .collect()
}

#[test]
fn test_default_is_one_plot_of_the_first_preset() {
    let selection = PlotSelection::default();
    assert_eq!(series(&selection), [PRESETS[0].to_vec()]);
    assert_eq!(selection.focused(), 0);
}

#[test]
fn test_cycle_preset_steps_through_the_presets_and_wraps() {
    let mut selection = PlotSelection::default();
    for preset in PRESETS.iter().skip(1).chain([&PRESETS[0]]) {
        selection.cycle_preset();
        assert_eq!(series(&selection), [preset.to_vec()]);
    }
    // A plot that is no preset starts over at the first.
    let mut other = PlotSelection::from_panels(&[&["wall_momentum"]]);
    other.cycle_preset();
    assert_eq!(series(&other), [PRESETS[0].to_vec()]);
}

#[test]
fn test_step_series_replaces_the_last_one_and_skips_shown_series() {
    let mut selection = PlotSelection::from_panels(&[&["E_kin", "E_pot"]]);
    assert!(selection.step_series(&AVAILABLE, 1));
    // E_pot → E_tot (E_kin is shown already).
    assert_eq!(series(&selection), [vec!["E_kin", "E_tot"]]);
    assert!(selection.step_series(&AVAILABLE, -1));
    // E_tot → E_pot, back.
    assert_eq!(series(&selection), [vec!["E_kin", "E_pot"]]);
    assert!(selection.step_series(&AVAILABLE, -1));
    // E_pot → wraps past the shown E_kin to N2:count.
    assert_eq!(series(&selection), [vec!["E_kin", "N2:count"]]);
    assert!(!PlotSelection::default().step_series(&[], 1));
}

#[test]
fn test_a_series_not_on_offer_steps_to_the_first_or_last() {
    let mut selection = PlotSelection::from_panels(&[&["gone"]]);
    assert!(selection.step_series(&AVAILABLE, 1));
    assert_eq!(series(&selection), [vec!["E_kin"]]);
    let mut selection = PlotSelection::from_panels(&[&["gone"]]);
    assert!(selection.step_series(&AVAILABLE, -1));
    assert_eq!(series(&selection), [vec!["N2:count"]]);
}

#[test]
fn test_add_and_remove_series_keep_at_least_one() {
    let mut selection = PlotSelection::default();
    assert!(selection.add_series(&AVAILABLE));
    assert_eq!(series(&selection), [vec!["E_pot", "E_tot"]]);
    assert!(selection.remove_series());
    assert!(!selection.remove_series(), "a plot keeps one series");
    assert_eq!(series(&selection), [vec!["E_pot"]]);

    let mut full = PlotSelection::from_panels(&[&AVAILABLE]);
    assert!(!full.add_series(&AVAILABLE), "nothing left to add");
}

#[test]
fn test_add_plot_takes_an_unshown_series_focuses_it_and_stops_at_the_maximum() {
    let mut selection = PlotSelection::from_panels(&[&["E_kin", "E_pot", "E_tot"]]);
    assert!(selection.add_plot(&AVAILABLE));
    assert_eq!(selection.focused(), 1);
    assert_eq!(series(&selection)[1], ["wall_momentum"]);
    while selection.panels().len() < MAX_PLOTS {
        assert!(selection.add_plot(&AVAILABLE));
    }
    assert!(!selection.add_plot(&AVAILABLE));
    assert_eq!(selection.panels().len(), MAX_PLOTS);
}

#[test]
fn test_add_plot_goes_below_the_focused_one() {
    let mut selection = PlotSelection::from_panels(&[&["E_kin"], &["E_pot"]]);
    assert!(selection.add_plot(&AVAILABLE));
    assert_eq!(
        series(&selection),
        [vec!["E_kin"], vec!["E_tot"], vec!["E_pot"]]
    );
    assert_eq!(selection.focused(), 1);
}

#[test]
fn test_remove_plot_keeps_one_and_moves_the_focus_up() {
    let mut selection = PlotSelection::from_panels(&[&["E_kin"], &["E_pot"], &["E_tot"]]);
    selection.focus(2);
    assert!(selection.remove_plot());
    assert_eq!(selection.focused(), 1);
    assert!(selection.focus(0));
    assert!(selection.remove_plot());
    assert_eq!(selection.focused(), 0);
    assert_eq!(series(&selection), [vec!["E_pot"]]);
    assert!(!selection.remove_plot(), "the window keeps one plot");
}

#[test]
fn test_focus_next_wraps_and_focus_ignores_missing_plots() {
    let mut selection = PlotSelection::from_panels(&[&["E_kin"], &["E_pot"]]);
    selection.focus_next();
    assert_eq!(selection.focused(), 1);
    selection.focus_next();
    assert_eq!(selection.focused(), 0);
    assert!(!selection.focus(5));
    assert!(!selection.focus(0), "already focused");
}

#[test]
fn test_toggle_hidden_hides_and_shows_and_forgets_removed_series() {
    let mut selection = PlotSelection::from_panels(&[&["E_kin", "E_pot"]]);
    assert!(selection.toggle_hidden(0, "E_pot"));
    let panel = &selection.panels()[0];
    assert!(panel.is_hidden("E_pot"));
    assert_eq!(panel.shown().collect::<Vec<_>>(), ["E_kin"]);
    assert!(!selection.toggle_hidden(0, "E_tot"), "not in the plot");
    assert!(!selection.toggle_hidden(3, "E_kin"), "no such plot");

    // Replaced, then added again: shown.
    assert!(selection.step_series(&AVAILABLE, 1));
    selection.step_series(&AVAILABLE, -1);
    assert!(!selection.panels()[0].is_hidden("E_pot"));
}

#[test]
fn test_from_panels_drops_empty_plots_and_keeps_at_most_the_maximum() {
    let selection = PlotSelection::from_panels(&[&[], &["E_kin"], &["a"], &["b"], &["c"], &["d"]]);
    assert_eq!(selection.panels().len(), MAX_PLOTS);
    assert_eq!(series(&selection)[0], ["E_kin"]);
    assert_eq!(PlotSelection::from_panels(&[]), PlotSelection::default());
}

#[test]
fn test_title_names_up_to_three_series_else_counts() {
    assert_eq!(PlotSelection::default().title(), "E_pot");
    let three = PlotSelection::from_panels(&[&["E_kin", "E_pot", "E_tot"]]);
    assert_eq!(three.title(), "E_kin, E_pot, E_tot");
    assert_eq!(
        PlotSelection::from_panels(&[&AVAILABLE]).title(),
        "6 series"
    );
    let two = PlotSelection::from_panels(&[&["E_kin"], &["E_pot"]]);
    assert_eq!(two.title(), "2 plots");
}
