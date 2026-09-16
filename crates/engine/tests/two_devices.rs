//! Two engines, two directories, one shared decision procedure.
//!
//! The convergence tests in `qurb-sync` prove the *logic* converges, using an
//! in-memory model with no files in it. These prove the logic is wired to real
//! storage correctly: that a plan turns into bytes on disk, that vectors
//! survive a round trip through SQLite, and that the two agree afterwards.
//!
//! There is still no network. `StoreSource` stands in for one by serving
//! content out of the other device's store, which is exactly the seam a
//! transport will slot into later.

use qurb_engine::{Engine, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// One simulated device: a directory, a store, and an engine over both.
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
        // Both devices share a key, as a user's own devices would.
        let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);

        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn write(&mut self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn remove(&mut self, rel: &str) {
        fs::remove_file(self.root.join(rel)).unwrap();
        self.engine.reconcile().unwrap();
    }

    /// What is actually on disk, ignoring the store.
    fn on_disk(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap().flatten() {
                let p = entry.path();
                if p.file_name().unwrap() == ".qurb" {
                    continue;
                }
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(&self.root).unwrap().to_string_lossy().to_string();
                    out.insert(rel, fs::read_to_string(&p).unwrap());
                }
            }
        }
        out
    }

    fn live_paths(&self) -> Vec<String> {
        self.engine.store().db().live_paths().unwrap()
    }
}

/// One exchange in both directions, each side fetching content from the other.
fn sync_once(a: &mut Device, b: &mut Device) {
    let a_plan = a.engine.plan_against(&b.engine.tree().unwrap()).unwrap();
    let b_plan = b.engine.plan_against(&a.engine.tree().unwrap()).unwrap();

    {
        let mut source = StoreSource::new(b.engine.store());
        let stats = a.engine.apply_plan(&a_plan, &mut source).unwrap();
        assert!(stats.is_clean(), "device A had failures: {:?}", stats.failures);
    }
    {
        let mut source = StoreSource::new(a.engine.store());
        let stats = b.engine.apply_plan(&b_plan, &mut source).unwrap();
        assert!(stats.is_clean(), "device B had failures: {:?}", stats.failures);
    }
}

fn sync_until_stable(a: &mut Device, b: &mut Device) -> usize {
    for round in 1..=8 {
        sync_once(a, b);
        if a.engine.plan_against(&b.engine.tree().unwrap()).unwrap().is_empty()
            && b.engine.plan_against(&a.engine.tree().unwrap()).unwrap().is_empty()
        {
            return round;
        }
    }
    panic!("two engines did not converge within 8 rounds");
}

fn assert_converged(a: &Device, b: &Device) {
    assert_eq!(a.on_disk(), b.on_disk(), "the two directories differ");
    assert_eq!(a.live_paths(), b.live_paths(), "the two indexes differ");
}

// -- vectors survive storage -------------------------------------------------

#[test]
fn a_local_change_is_stamped_with_this_devices_vector() {
    let mut a = Device::new();
    a.write("notes.txt", "hello");

    let device = a.engine.store().device_id().unwrap();
    let tree = a.engine.tree().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].path, "notes.txt");
    assert_eq!(tree[0].modified_by, device);
    assert_eq!(tree[0].vector.get(&device), 1, "the first change is counter 1");

    a.write("notes.txt", "hello again");
    let tree = a.engine.tree().unwrap();
    assert_eq!(tree[0].vector.get(&device), 2, "an edit advances the counter");
}

#[test]
fn rewriting_identical_bytes_does_not_advance_the_clock() {
    // Touching a file, or saving it unchanged, is not a change. Advancing the
    // clock would make it look newer than a peer's genuinely newer version.
    let mut a = Device::new();
    a.write("notes.txt", "stable");
    let device = a.engine.store().device_id().unwrap();
    let before = a.engine.tree().unwrap()[0].vector.get(&device);

    a.write("notes.txt", "stable");
    assert_eq!(a.engine.tree().unwrap()[0].vector.get(&device), before);
}

#[test]
fn vectors_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sync");
    let store_dir = root.join(".qurb");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.txt"), "content").unwrap();

    let (device, vector) = {
        let store = Store::open(&store_dir, ChunkKey::from_bytes([1; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        let mut engine = Engine::new(&root, store, ignore);
        engine.reconcile().unwrap();
        let d = engine.store().device_id().unwrap();
        (d, engine.tree().unwrap()[0].vector.clone())
    };

    let store = Store::open(&store_dir, ChunkKey::from_bytes([1; 32])).unwrap();
    let ignore = IgnoreRules::new().with_store_dir(&store_dir);
    let engine = Engine::new(&root, store, ignore);

    assert_eq!(engine.store().device_id().unwrap(), device, "identity must be stable");
    assert_eq!(engine.tree().unwrap()[0].vector, vector, "history must survive a restart");
}

#[test]
fn two_stores_get_different_identities() {
    let (a, b) = (Device::new(), Device::new());
    assert_ne!(
        a.engine.store().device_id().unwrap(),
        b.engine.store().device_id().unwrap()
    );
}

// -- syncing -----------------------------------------------------------------

#[test]
fn a_file_reaches_the_other_device() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("notes.txt", "written on A");

    sync_until_stable(&mut a, &mut b);

    assert_eq!(b.on_disk().get("notes.txt").map(String::as_str), Some("written on A"));
    assert_converged(&a, &b);
}

#[test]
fn an_edit_propagates() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("notes.txt", "version one");
    sync_until_stable(&mut a, &mut b);

    a.write("notes.txt", "version two");
    sync_until_stable(&mut a, &mut b);

    assert_eq!(b.on_disk().get("notes.txt").map(String::as_str), Some("version two"));
    assert_converged(&a, &b);
}

#[test]
fn a_deletion_propagates_and_stays_deleted() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("doomed.txt", "temporary");
    sync_until_stable(&mut a, &mut b);
    assert!(b.on_disk().contains_key("doomed.txt"));

    a.remove("doomed.txt");
    sync_until_stable(&mut a, &mut b);

    assert!(!b.on_disk().contains_key("doomed.txt"), "the file must be gone from disk");
    assert_converged(&a, &b);

    // And must not come back on a later exchange.
    sync_until_stable(&mut a, &mut b);
    assert!(a.on_disk().is_empty() && b.on_disk().is_empty());
}

#[test]
fn nested_paths_survive_the_round_trip() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("work/reports/q3.md", "# Q3");
    sync_until_stable(&mut a, &mut b);

    assert_eq!(b.on_disk().get("work/reports/q3.md").map(String::as_str), Some("# Q3"));
    assert_converged(&a, &b);
}

#[test]
fn both_devices_contributing_different_files_converge() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("from-a.txt", "A");
    b.write("from-b.txt", "B");

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(a.on_disk().len(), 2);
}

// -- conflicts ---------------------------------------------------------------

#[test]
fn concurrent_edits_keep_both_versions_on_disk() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("shared.txt", "original");
    sync_until_stable(&mut a, &mut b);

    // Both edit without seeing the other.
    a.write("shared.txt", "edited on A");
    b.write("shared.txt", "edited on B");

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);

    let files = a.on_disk();
    assert_eq!(files.len(), 2, "both edits must survive: {:?}", files.keys().collect::<Vec<_>>());

    let contents: Vec<&str> = files.values().map(String::as_str).collect();
    assert!(contents.contains(&"edited on A"), "A's edit was lost: {contents:?}");
    assert!(contents.contains(&"edited on B"), "B's edit was lost: {contents:?}");
    assert!(files.keys().any(|k| k.starts_with("shared.conflict-")));
}

#[test]
fn a_conflict_settles_and_does_not_recur() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("shared.txt", "original");
    sync_until_stable(&mut a, &mut b);

    a.write("shared.txt", "A wins or loses");
    b.write("shared.txt", "B loses or wins");
    sync_until_stable(&mut a, &mut b);

    let after_first = a.on_disk();
    sync_until_stable(&mut a, &mut b);
    assert_eq!(a.on_disk(), after_first, "syncing again produced more conflict files");
}

#[test]
fn an_edit_survives_a_concurrent_delete() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("contested.txt", "original");
    sync_until_stable(&mut a, &mut b);

    a.write("contested.txt", "edited, not deleted");
    b.remove("contested.txt");

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(
        b.on_disk().get("contested.txt").map(String::as_str),
        Some("edited, not deleted"),
        "the edit must come back to the device that deleted it"
    );
}

#[test]
fn identical_content_created_independently_is_not_a_conflict() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("same.txt", "identical bytes");
    b.write("same.txt", "identical bytes");

    sync_until_stable(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(a.on_disk().len(), 1, "no conflict file should appear");

    // And the histories must actually have merged, or the next edit would
    // raise a conflict over content that never disagreed.
    a.write("same.txt", "now changed");
    sync_until_stable(&mut a, &mut b);
    assert_eq!(a.on_disk().len(), 1, "a phantom conflict appeared after merging");
    assert_eq!(b.on_disk().get("same.txt").map(String::as_str), Some("now changed"));
}

// -- transfer avoidance ------------------------------------------------------

#[test]
fn content_already_held_is_not_fetched_again() {
    // A file that exists under another name is already on disk. Asking the peer
    // for it would be pure waste, and over a real network it would be the
    // difference between a rename costing nothing and costing the whole file.
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("original.txt", "expensive content");
    sync_until_stable(&mut a, &mut b);

    // A copy under a new name: same bytes, new path.
    a.write("copy.txt", "expensive content");

    let plan = b.engine.plan_against(&a.engine.tree().unwrap()).unwrap();
    let mut source = StoreSource::new(a.engine.store());
    let stats = b.engine.apply_plan(&plan, &mut source).unwrap();

    assert_eq!(stats.adopted, 1);
    assert_eq!(stats.fetched, 0, "the bytes were already on this device");
    assert_eq!(b.on_disk().get("copy.txt").map(String::as_str), Some("expensive content"));
}

#[test]
fn syncing_an_already_agreed_pair_plans_no_work() {
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("a.txt", "one");
    b.write("b.txt", "two");
    sync_until_stable(&mut a, &mut b);

    assert!(a.engine.plan_against(&b.engine.tree().unwrap()).unwrap().is_empty());
    assert!(b.engine.plan_against(&a.engine.tree().unwrap()).unwrap().is_empty());
}

// -- the two halves must not fight each other --------------------------------

#[test]
fn reconciling_after_a_sync_does_not_restamp_adopted_files() {
    // The trap: `apply_plan` writes a file to disk, the watcher or a
    // reconciliation notices a file it has not seen, and the engine stamps it
    // as a *local* change. That version would then be concurrent with the
    // peer's own copy, and the next exchange would raise a conflict over a file
    // that was just successfully synced -- forever.
    //
    // What prevents it is `apply_plan` recording the modification time the file
    // actually ended up with, so the size-and-mtime fast path recognises it.
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("shared.txt", "content from A");
    a.write("nested/deep.txt", "also from A");
    sync_until_stable(&mut a, &mut b);

    let before = b.engine.tree().unwrap();

    let stats = b.engine.reconcile().unwrap();
    assert_eq!(stats.stored, 0, "nothing should have been re-read");
    assert_eq!(stats.deleted, 0, "nothing should have been tombstoned");
    assert_eq!(stats.unchanged, 2);

    assert_eq!(b.engine.tree().unwrap(), before, "reconciliation altered adopted history");
    assert!(
        b.engine.plan_against(&a.engine.tree().unwrap()).unwrap().is_empty(),
        "a reconciliation after syncing created new work"
    );
}

#[test]
fn a_tombstone_adopted_from_a_peer_is_not_resurrected_by_reconciliation() {
    // The mirror case: after adopting a deletion, the file is gone from disk
    // and the index says deleted. A reconciliation must agree rather than
    // deciding the local copy vanished and needs re-tombstoning with a fresh
    // local vector.
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("doomed.txt", "temporary");
    sync_until_stable(&mut a, &mut b);
    a.remove("doomed.txt");
    sync_until_stable(&mut a, &mut b);

    let before = b.engine.tree().unwrap();
    let stats = b.engine.reconcile().unwrap();
    assert_eq!(stats.deleted, 0);
    assert_eq!(b.engine.tree().unwrap(), before);
    assert!(b.engine.plan_against(&a.engine.tree().unwrap()).unwrap().is_empty());
}

#[test]
fn a_conflict_file_is_not_treated_as_a_new_local_edit() {
    // A conflict writes a second file onto both devices. If either then stamps
    // it as its own new file, the two copies become concurrent and conflict
    // again -- producing a conflict file for the conflict file.
    let (mut a, mut b) = (Device::new(), Device::new());
    a.write("shared.txt", "original");
    sync_until_stable(&mut a, &mut b);

    a.write("shared.txt", "edited on A");
    b.write("shared.txt", "edited on B");
    sync_until_stable(&mut a, &mut b);

    let files_before = a.on_disk();
    assert_eq!(files_before.len(), 2);

    a.engine.reconcile().unwrap();
    b.engine.reconcile().unwrap();
    sync_until_stable(&mut a, &mut b);

    assert_eq!(a.on_disk(), files_before, "a conflict file spawned another conflict");
    assert_converged(&a, &b);
}
