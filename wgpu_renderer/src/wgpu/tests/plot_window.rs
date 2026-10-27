// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plot window's requests carried out by the backend (#371), against a
//! fake window: no display needed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use thermo_ui::graphics::energy_plot::EnergyData;
use thermo_ui::graphics::renderer::event_loop::{EnergyWindowManager, KeyMap};

use crate::wgpu::plot_surface::{PlotOsWindow, handle_plot_requests};

/// What happened to the fake windows, shared with the test.
#[derive(Default)]
struct Log {
    titles: RefCell<Vec<String>>,
    redraws: Cell<u32>,
    opened: Cell<u32>,
}

struct FakeWindow {
    log: Rc<Log>,
}

impl PlotOsWindow for FakeWindow {
    fn inner_size(&self) -> (u32, u32) {
        (800, 600)
    }
    fn scale_factor(&self) -> f64 {
        1.0
    }
    fn set_title(&self, title: &str) {
        self.log.titles.borrow_mut().push(title.to_string());
    }
    fn request_redraw(&self) {
        self.log.redraws.set(self.log.redraws.get() + 1);
    }
}

fn manager() -> EnergyWindowManager {
    let mut data = EnergyData::new();
    data.insert_series("E_kin", None, [1.0, 2.0]);
    data.insert_series("E_pot", None, [0.0, -1.0]);
    data.sample_times.extend([0.0, 0.1]);
    EnergyWindowManager::new(Arc::new(Mutex::new(data)))
}

/// Carry out the queued requests; `open` makes fake windows logging to `log`
/// (or fails when `fail` is set).
fn apply(
    window: &mut Option<FakeWindow>,
    manager: &mut EnergyWindowManager,
    log: &Rc<Log>,
    fail: bool,
) -> Vec<String> {
    let mut titles = Vec::new();
    handle_plot_requests(window, manager, |title| {
        titles.push(title.to_string());
        if fail {
            return Err("no display".into());
        }
        log.opened.set(log.opened.get() + 1);
        Ok(FakeWindow {
            log: Rc::clone(log),
        })
    });
    titles
}

#[test]
fn toggle_opens_the_window_reports_its_size_and_draws_it() {
    let (mut manager, log) = (manager(), Rc::new(Log::default()));
    let mut window = None;
    manager.toggle();
    let opened = apply(&mut window, &mut manager, &log, false);

    assert_eq!(opened, ["Plot - E_pot"]);
    assert!(window.is_some() && manager.is_open());
    assert_eq!(log.redraws.get(), 1, "a new window is drawn at once");
    let frame = manager
        .handle_redraw(&KeyMap::default())
        .expect("a frame for the open window");
    assert_eq!(
        frame.scene.size,
        (800, 600),
        "the size the backend reported"
    );
}

#[test]
fn toggle_while_open_cycles_the_preset_and_retitles() {
    let (mut manager, log) = (manager(), Rc::new(Log::default()));
    let mut window = None;
    manager.toggle();
    apply(&mut window, &mut manager, &log, false);
    manager.toggle();
    apply(&mut window, &mut manager, &log, false);

    assert_eq!(log.opened.get(), 1, "no second window");
    assert_eq!(*log.titles.borrow(), ["Plot - E_kin"]);
}

#[test]
fn close_drops_the_window_and_reopening_makes_a_new_one() {
    let (mut manager, log) = (manager(), Rc::new(Log::default()));
    let mut window = None;
    manager.toggle();
    apply(&mut window, &mut manager, &log, false);
    manager.close();
    apply(&mut window, &mut manager, &log, false);
    assert!(window.is_none() && !manager.is_open());
    assert!(manager.handle_redraw(&KeyMap::default()).is_none());

    manager.toggle();
    apply(&mut window, &mut manager, &log, false);
    assert!(window.is_some());
    assert_eq!(log.opened.get(), 2);
}

#[test]
fn a_window_that_cannot_be_made_counts_as_closed() {
    let (mut manager, log) = (manager(), Rc::new(Log::default()));
    let mut window = None;
    manager.toggle();
    apply(&mut window, &mut manager, &log, true);
    assert!(window.is_none());
    assert!(!manager.is_open());
    // The next toggle tries again.
    manager.toggle();
    let tried = apply(&mut window, &mut manager, &log, false);
    assert_eq!(tried.len(), 1);
    assert!(window.is_some());
}

#[test]
fn requests_without_a_window_are_dropped() {
    let (mut manager, log) = (manager(), Rc::new(Log::default()));
    let mut window: Option<FakeWindow> = None;
    // A plot command while closed changes the plots but asks nothing.
    use thermo_ui::graphics::renderer::event_loop::command_registry::PlotChange;
    manager.change_plots(PlotChange::NextSeries);
    assert!(manager.take_requests().is_empty());
    apply(&mut window, &mut manager, &log, false);
    assert_eq!(log.opened.get(), 0);
}
