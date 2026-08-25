use super::*;
use crate::headless_output::HeadlessSnapshotSink;
use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

struct RecordingSink(Rc<RefCell<Vec<u64>>>);

impl HeadlessSnapshotSink for RecordingSink {
    fn write_snapshot(&mut self, snapshot: &RendererSnapshot) -> io::Result<()> {
        self.0.borrow_mut().push(snapshot.simulation_step);
        Ok(())
    }
}

fn runner_with_snapshot_steps(steps: u32, snapshot_every: u32) -> (HeadlessRunner, Rc<RefCell<Vec<u64>>>) {
    let world_rect = rust_thermo_core::application::MainContent::bootstrap_world_rect(
        1280,
        720,
        rust_thermo_core::graphics::renderer::margins::Margins::new(110.0, 50.0, 50.0, 50.0),
    );
    let application = build_application(world_rect).expect("application");
    let recorded = Rc::new(RefCell::new(Vec::new()));
    let sink = RecordingSink(recorded.clone());
    let runner = HeadlessRunner::builder()
        .application(application)
        .sink(Box::new(sink))
        .steps(steps)
        .snapshot_every(snapshot_every)
        .build();
    (runner, recorded)
}

#[test]
fn test_headless_runner_emits_step_zero_and_final() {
    let (runner, recorded) = runner_with_snapshot_steps(1, 0);
    let report = runner.run().expect("run");
    assert_eq!(report.snapshots_emitted, 2);
    assert_eq!(*recorded.borrow(), vec![0, 1]);
}

#[test]
fn test_headless_runner_emits_interval_and_final() {
    let (runner, recorded) = runner_with_snapshot_steps(3, 2);
    let report = runner.run().expect("run");
    assert_eq!(report.snapshots_emitted, 3);
    assert_eq!(*recorded.borrow(), vec![0, 1, 3]);
}

#[test]
fn test_headless_runner_zero_interval_skips_periodic_snapshots() {
    let (runner, recorded) = runner_with_snapshot_steps(3, 0);
    let report = runner.run().expect("run");
    assert_eq!(report.snapshots_emitted, 2);
    assert_eq!(*recorded.borrow(), vec![0, 3]);
}
