// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The `Export` world command on the physics thread.

use super::save_load_thread::scratch_dir;
use crate::application::persistence::SaveStore;
use crate::application::{PhysicsEvent, PhysicsThread, SnapshotPublisher, WorldCommand};
use crate::export::{ExportFormat, ExportKind};
use crate::metrics::MetricsRegistry;
use std::fs;
use std::time::{Duration, Instant};
use thermo_physics::test_support::create_main_content;

fn next_event(thread: &PhysicsThread) -> PhysicsEvent {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = thread.try_recv_event() {
            return event;
        }
        assert!(
            Instant::now() < deadline,
            "no event from the physics thread"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn export(kind: ExportKind, format: ExportFormat) -> WorldCommand {
    WorldCommand::Export { kind, format }
}

#[test]
fn export_writes_timestamped_files_to_the_save_directory() {
    let dir = scratch_dir("export_thread");
    let (mut content, _) = create_main_content();
    content.add_random_candidates(5);
    let thread = PhysicsThread::spawn(
        content.with_save_store(SaveStore::new(&dir)),
        SnapshotPublisher::new(),
        Box::new(|| {}),
        MetricsRegistry::shared(),
    );
    // Let the thread sample at least once.
    std::thread::sleep(Duration::from_millis(200));

    thread
        .send_command(export(ExportKind::Series, ExportFormat::Csv))
        .unwrap();
    let PhysicsEvent::DataExported {
        path: series,
        note: _,
    } = next_event(&thread)
    else {
        panic!("expected DataExported");
    };
    thread
        .send_command(export(ExportKind::Snapshot, ExportFormat::Jsonl))
        .unwrap();
    let PhysicsEvent::DataExported { path: snapshot, .. } = next_event(&thread) else {
        panic!("expected DataExported");
    };
    thread.shutdown_and_join().unwrap();

    assert!(
        series.to_string_lossy().ends_with("_series.csv"),
        "{series:?}"
    );
    assert!(
        snapshot.to_string_lossy().ends_with("_snapshot.jsonl"),
        "{snapshot:?}"
    );
    let series = fs::read_to_string(series).unwrap();
    assert!(series.starts_with("step,"), "{series}");
    assert!(
        series.lines().next().unwrap().contains(",E_kin [J],"),
        "{series}"
    );
    assert!(series.lines().count() >= 2);
    let snapshot = fs::read_to_string(snapshot).unwrap();
    assert_eq!(snapshot.lines().count(), 5 + 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn export_without_a_save_directory_fails_cleanly() {
    let (content, _) = create_main_content();
    let thread = PhysicsThread::spawn(
        content,
        SnapshotPublisher::new(),
        Box::new(|| {}),
        MetricsRegistry::shared(),
    );
    thread
        .send_command(export(ExportKind::Series, ExportFormat::Csv))
        .unwrap();
    assert!(matches!(next_event(&thread), PhysicsEvent::ExportFailed(_)));
    thread.shutdown_and_join().unwrap();
}

#[test]
fn a_series_export_warns_when_older_samples_were_evicted() {
    let dir = scratch_dir("export_evicted");
    let (content, _) = create_main_content();
    // A registry of two samples fills up within a few snapshots.
    let metrics = std::sync::Arc::new(std::sync::RwLock::new(MetricsRegistry::new(2)));
    let thread = PhysicsThread::spawn(
        content.with_save_store(SaveStore::new(&dir)),
        SnapshotPublisher::new(),
        Box::new(|| {}),
        std::sync::Arc::clone(&metrics),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while crate::metrics::read_metrics(&metrics)
        .steps()
        .total_pushed()
        < 4
    {
        assert!(Instant::now() < deadline, "the thread did not take samples");
        std::thread::sleep(Duration::from_millis(5));
    }
    thread
        .send_command(export(ExportKind::Series, ExportFormat::Csv))
        .unwrap();
    let PhysicsEvent::DataExported { note, .. } = next_event(&thread) else {
        panic!("expected DataExported");
    };
    thread.shutdown_and_join().unwrap();
    assert!(note.is_some_and(|n| n.contains("evicted")));
    fs::remove_dir_all(dir).unwrap();
}

fn toggle_stream() -> WorldCommand {
    WorldCommand::ToggleSampleStream {
        format: ExportFormat::Csv,
    }
}

/// Wait until the thread took `count` more samples than now.
fn wait_for_samples(metrics: &crate::metrics::SharedMetricsRegistry, count: u64) {
    let pushed = || crate::metrics::read_metrics(metrics).steps().total_pushed();
    let target = pushed() + count;
    let deadline = Instant::now() + Duration::from_secs(10);
    while pushed() < target {
        assert!(Instant::now() < deadline, "the thread did not take samples");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_sample_stream_writes_from_toggle_on_to_toggle_off() {
    let dir = scratch_dir("stream_thread");
    let (content, _) = create_main_content();
    let metrics = MetricsRegistry::shared();
    let thread = PhysicsThread::spawn(
        content.with_save_store(SaveStore::new(&dir)),
        SnapshotPublisher::new(),
        Box::new(|| {}),
        std::sync::Arc::clone(&metrics),
    );
    thread.send_command(toggle_stream()).unwrap();
    assert!(matches!(
        next_event(&thread),
        PhysicsEvent::StreamEnabled { .. }
    ));
    let PhysicsEvent::StreamFile(path) = next_event(&thread) else {
        panic!("expected StreamFile");
    };
    wait_for_samples(&metrics, 3);
    thread.send_command(toggle_stream()).unwrap();
    let PhysicsEvent::StreamStopped { files, rows } = next_event(&thread) else {
        panic!("expected StreamStopped");
    };
    thread.shutdown_and_join().unwrap();

    assert_eq!(files, std::slice::from_ref(&path));
    assert!(path.to_string_lossy().ends_with("_stream.csv"), "{path:?}");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("step,time,"), "{text}");
    assert!(rows >= 3, "{rows} rows");
    assert_eq!(text.lines().count() as u64, rows + 1, "all rows flushed");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn quitting_flushes_the_sample_stream() {
    let dir = scratch_dir("stream_quit");
    let (content, _) = create_main_content();
    let metrics = MetricsRegistry::shared();
    let thread = PhysicsThread::spawn(
        content.with_save_store(SaveStore::new(&dir)),
        SnapshotPublisher::new(),
        Box::new(|| {}),
        std::sync::Arc::clone(&metrics),
    );
    thread.send_command(toggle_stream()).unwrap();
    next_event(&thread);
    let PhysicsEvent::StreamFile(path) = next_event(&thread) else {
        panic!("expected StreamFile");
    };
    wait_for_samples(&metrics, 3);
    thread.shutdown_and_join().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.lines().count() >= 4, "{text}");
    assert!(text.ends_with('\n'), "no partial row: {text}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failing_sample_stream_is_reported_once_and_the_simulation_goes_on() {
    let dir = scratch_dir("stream_unwritable");
    fs::create_dir_all(&dir).unwrap();
    let blocker = dir.join("not_a_dir");
    fs::write(&blocker, "").unwrap();
    let (content, _) = create_main_content();
    let metrics = MetricsRegistry::shared();
    let thread = PhysicsThread::spawn(
        content.with_save_store(SaveStore::new(blocker.join("saves"))),
        SnapshotPublisher::new(),
        Box::new(|| {}),
        std::sync::Arc::clone(&metrics),
    );
    thread.send_command(toggle_stream()).unwrap();
    assert!(matches!(
        next_event(&thread),
        PhysicsEvent::StreamEnabled { .. }
    ));
    assert!(matches!(next_event(&thread), PhysicsEvent::StreamFailed(_)));
    wait_for_samples(&metrics, 5);
    assert_eq!(
        thread.try_recv_event(),
        None,
        "the failure is reported once"
    );
    thread.shutdown_and_join().unwrap();
    fs::remove_dir_all(dir).unwrap();
}
