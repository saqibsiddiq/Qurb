//! The engine writes down what it did, as it does it.
//!
//! Every assertion here is about a question a person asks *later* — after the
//! process that did the work has exited — which is exactly the question a log
//! cannot answer.

use qurb_engine::Engine;
use qurb_storage::db::Event;
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;

struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");
        let store = Store::open(&store_dir, ChunkKey::from_bytes([11; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn history(&self) -> Vec<(Event, Option<String>)> {
        self.engine
            .store()
            .db()
            .activity(50, None)
            .unwrap()
            .into_iter()
            .map(|r| (r.kind, r.path))
            .collect()
    }
}

#[test]
fn storing_and_deleting_a_file_are_both_written_down() {
    let mut device = Device::new();

    fs::write(device.root.join("notes.txt"), b"something worth keeping").unwrap();
    device.engine.reconcile().unwrap();

    assert_eq!(
        device.history(),
        vec![(Event::Stored, Some("notes.txt".to_string()))],
        "storing a file left no trace"
    );

    fs::remove_file(device.root.join("notes.txt")).unwrap();
    device.engine.reconcile().unwrap();

    assert_eq!(
        device.history().first(),
        Some(&(Event::Deleted, Some("notes.txt".to_string()))),
        "deleting a file left no trace"
    );
}

/// The size is on the row, because "why is my disk full" is asked of the same
/// list as "where did my file go".
#[test]
fn a_stored_file_records_its_size() {
    let mut device = Device::new();
    fs::write(device.root.join("big.bin"), vec![0u8; 50_000]).unwrap();
    device.engine.reconcile().unwrap();

    let row = &device.engine.store().db().activity(10, None).unwrap()[0];
    assert_eq!(row.size, Some(50_000));
}

/// Re-reading an unchanged file is the hot path and happens on every
/// reconciliation. It is not something that happened.
#[test]
fn an_unchanged_file_is_not_an_event() {
    let mut device = Device::new();
    fs::write(device.root.join("steady.txt"), b"unchanging").unwrap();
    device.engine.reconcile().unwrap();
    device.engine.reconcile().unwrap();
    device.engine.reconcile().unwrap();

    assert_eq!(device.history().len(), 1, "a reconciliation pass is not news");
}

/// A file that failed is the most important thing in the list, and the reason
/// has to survive the process that saw it.
#[test]
fn a_failure_records_why() {
    let device = Device::new();
    device
        .engine
        .store()
        .db()
        .record(
            Event::Failed,
            Some("locked.txt"),
            None,
            None,
            Some("permission denied"),
        )
        .unwrap();

    let row = &device.engine.store().db().activity(10, None).unwrap()[0];
    assert_eq!(row.kind, Event::Failed);
    assert_eq!(row.detail.as_deref(), Some("permission denied"));
}
