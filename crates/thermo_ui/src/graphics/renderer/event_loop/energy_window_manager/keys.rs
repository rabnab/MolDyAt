// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Keys of the energy window: which key does what (from the key map) and the
//! key hints the plot shows.

use crate::graphics::renderer::event_loop::command_registry::{
    KeyInput, PlotChange, UiAction, command_by_id,
};
use crate::graphics::renderer::event_loop::key_map::{HeldModifiers, KeyMap, KeyPhase};
use winit::keyboard::KeyCode;

/// What a key does while the energy window has the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::graphics::renderer::event_loop) enum EnergyWindowKey {
    /// Close the window.
    Close,
    /// Change the plots (the plot commands, #339).
    Plot(PlotChange),
    /// Return the plot to its default time view.
    ResetView,
    /// Switch between the time series and the distributions (#343).
    Distributions,
}

/// The energy window's action for a key event. The keys come from the key
/// map, so a `ui.toml` rebinds them here too (#355): the keys of
/// `view.cycle_plot` and `view.toggle_energy_window` (`P` and `TAB` by
/// default) give the focused plot the next energy preset, each on its own
/// trigger, and the `view.plot_*` commands change the plots (#339). `ESC`
/// closes the window, as the close shortcut of the window itself, not a
/// registry command (in the main window `ESC` clears the balls).
pub(in crate::graphics::renderer::event_loop) fn energy_window_key(
    key_map: &KeyMap,
    input: KeyInput,
    phase: KeyPhase,
    held: HeldModifiers,
) -> Option<EnergyWindowKey> {
    if phase == KeyPhase::Down && input.code == KeyCode::Escape {
        return Some(EnergyWindowKey::Close);
    }
    let (command, _) = key_map.command_for_key(input, phase, held)?;
    match command.ui_action()? {
        UiAction::CyclePlotContent | UiAction::ToggleEnergyWindow => {
            Some(EnergyWindowKey::Plot(PlotChange::CyclePreset))
        }
        UiAction::Plot(change) => Some(EnergyWindowKey::Plot(change)),
        UiAction::ResetPlotView => Some(EnergyWindowKey::ResetView),
        UiAction::TogglePlotDistributions => Some(EnergyWindowKey::Distributions),
        _ => None,
    }
}

/// The key hints the energy window shows, from `key_map`, most important
/// first, two spaces apart (the layout drops trailing hints that do not
/// fit): `Esc` closes it (fixed), the keys of the plot commands give the
/// focused plot the next preset (see [`energy_window_key`]), then the keys
/// to step the series, reset the view and move the focus, so a rebinding in
/// `ui.toml` shows here too.
pub(in crate::graphics::renderer::event_loop) fn energy_window_key_hints(
    key_map: &KeyMap,
) -> String {
    let keys = |ids: &[&str]| -> String {
        ids.iter()
            .copied()
            .filter_map(command_by_id)
            .flat_map(|command| key_map.binding_display(command))
            .fold(Vec::new(), |mut keys, key| {
                if !keys.contains(&key) {
                    keys.push(key);
                }
                keys
            })
            .join(" / ")
    };
    let mut hints = vec!["Esc: close".to_string()];
    for (ids, what) in [
        (
            &["view.toggle_energy_window", "view.cycle_plot"][..],
            "preset",
        ),
        (
            &["view.plot_previous_series", "view.plot_next_series"][..],
            "series",
        ),
        (&["view.reset_plot_view"][..], "reset"),
        (&["view.plot_focus_next"][..], "focus"),
        (&["view.plot_distributions"][..], "distributions"),
    ] {
        let keys = keys(ids);
        if !keys.is_empty() {
            hints.push(format!("{keys}: {what}"));
        }
    }
    hints.join("  ")
}

/// The key hints of the distributions view: `Esc` and the key back to the
/// time series.
pub(super) fn distribution_key_hints(key_map: &KeyMap) -> String {
    let mut hints = vec!["Esc: close".to_string()];
    let keys = command_by_id("view.plot_distributions")
        .map(|command| key_map.binding_display(command).join(" / "))
        .unwrap_or_default();
    if !keys.is_empty() {
        hints.push(format!("{keys}: time series"));
    }
    hints.join("  ")
}
