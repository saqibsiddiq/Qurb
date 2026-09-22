//! `README` and `readme`.
//!
//! On Linux they are two files. On macOS and Windows they are one, and writing
//! the second destroys the first. That alone would be a nuisance; what makes it
//! data loss is what happens next — the reconciliation after the overwrite sees
//! one file where the index expected two, reports the missing one as deleted,
//! and propagates that deletion to every other device.
//!
//! A Linux desktop holding both files is therefore fine until the moment a
//! phone joins, and then it is not.

use qurb_engine::{Engine, Error, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_sync::FileVersion;
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
        let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        fs::write(self.root.join(rel), contents).unwrap();
        self.engine.reconcile().unwrap();
    }
}

#[test]
fn a_case_sensitive_filesystem_keeps_both_files() {
    // Establishes the baseline: where the filesystem can tell them apart,
    // nothing here should interfere.
    if qurb_watcher::is_case_insensitive(std::env::temp_dir().as_path()) {
        eprintln!("skipped: this machine's temp filesystem is case-insensitive");
        return;
    }

    let mut d = Device::new();
    d.write("README", b"upper");
    d.write("readme", b"lower");

    let live = d.engine.store().db().live_paths().unwrap();
    assert!(live.contains(&"README".to_string()));
    assert!(live.contains(&"readme".to_string()));
    assert!(!d.engine.folds_case(), "probe says this filesystem is case-sensitive");
}

#[test]
fn colliding_paths_are_reported_even_where_they_are_currently_harmless() {
    // The collision is latent, not absent. Reporting it on a case-sensitive
    // machine is the only chance to warn before a phone joins and turns it into
    // data loss.
    let mut d = Device::new();
    d.write("README", b"upper");
    d.write("readme", b"lower");
    d.write("notes.txt", b"unrelated");

    let groups = d.engine.case_collisions().unwrap();
    assert_eq!(groups.len(), 1, "got {groups:?}");
    assert!(groups[0].contains(&"README".to_string()));
    assert!(groups[0].contains(&"readme".to_string()));
    assert!(!groups[0].contains(&"notes.txt".to_string()));
}

#[test]
fn a_colliding_path_is_refused_rather_than_silently_overwriting() {
    // The guard. `set_fold_case(true)` makes a case-sensitive machine behave as
    // a case-insensitive one, which is what a mixed fleet needs anyway: the
    // hazard belongs to the set of devices, not to the local disk.
    let mut source = Device::new();
    source.write("README", b"from the peer");

    let mut target = Device::new();
    target.write("readme", b"already here, different case");
    target.engine.set_fold_case(true);

    let tree = source.engine.tree().unwrap();
    let plan = target.engine.plan_against(&tree).unwrap();
    assert!(!plan.is_empty(), "setup: the peer's file should need adopting");

    let reader = Store::open(&source.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&source.root);
    let mut content = StoreSource::new(&reader);
    let stats = target.engine.apply_plan(&plan, &mut content).unwrap();

    assert_eq!(stats.failures.len(), 1, "the collision should have been refused");
    assert!(
        matches!(stats.failures[0].error, Error::CaseCollision { .. }),
        "got {:?}",
        stats.failures[0].error
    );

    // The existing file must be untouched -- that is the whole point.
    assert_eq!(
        fs::read(target.root.join("readme")).unwrap(),
        b"already here, different case",
        "the existing file was destroyed by the colliding write"
    );
}

#[test]
fn refusing_one_path_does_not_stop_the_rest_of_the_plan() {
    // A collision is a problem with one file. Everything else must still sync,
    // for the same reason an unreadable file does not abort a reconciliation.
    let mut source = Device::new();
    source.write("README", b"collides");
    source.write("safe-one.txt", b"fine");
    source.write("safe-two.txt", b"also fine");

    let mut target = Device::new();
    target.write("readme", b"already here");
    target.engine.set_fold_case(true);

    let tree = source.engine.tree().unwrap();
    let plan = target.engine.plan_against(&tree).unwrap();
    let reader = Store::open(&source.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&source.root);
    let mut content = StoreSource::new(&reader);
    let stats = target.engine.apply_plan(&plan, &mut content).unwrap();

    assert_eq!(stats.failures.len(), 1);
    assert_eq!(fs::read(target.root.join("safe-one.txt")).unwrap(), b"fine");
    assert_eq!(fs::read(target.root.join("safe-two.txt")).unwrap(), b"also fine");
}

#[test]
fn a_path_that_only_differs_in_case_from_a_tombstone_is_allowed() {
    // A deleted file is not on disk, so there is nothing to collide with.
    // Refusing here would block a legitimate rename-by-case.
    let mut d = Device::new();
    d.write("README", b"content");
    fs::remove_file(d.root.join("README")).unwrap();
    d.engine.reconcile().unwrap();
    d.engine.set_fold_case(true);

    let colliding = d.engine.store().db().live_path_colliding_with("readme").unwrap();
    assert_eq!(colliding, None, "a tombstone should not block the path");
}

#[test]
fn rewriting_the_same_path_is_never_a_collision() {
    // Guarding against the guard: a path must not collide with itself, or no
    // file could ever be updated.
    let mut d = Device::new();
    d.write("README", b"first");
    d.engine.set_fold_case(true);

    assert_eq!(d.engine.store().db().live_path_colliding_with("README").unwrap(), None);
}

#[test]
fn the_probe_agrees_with_the_filesystem() {
    let d = Device::new();
    let detected = d.engine.folds_case();

    fs::write(d.root.join("CaseProbe"), b"x").unwrap();
    let actual = fs::metadata(d.root.join("caseprobe")).is_ok();
    assert_eq!(detected, actual, "the probe disagreed with the filesystem");
}

#[test]
fn a_version_for_a_colliding_path_still_plans_normally() {
    // Planning is filesystem-independent: it must produce the action, and the
    // refusal happens when the action is applied. Otherwise a device would
    // silently drop the path from its view of what it is missing.
    let mut target = Device::new();
    target.write("readme", b"here");
    target.engine.set_fold_case(true);

    let remote: Vec<FileVersion> = {
        let mut source = Device::new();
        source.write("README", b"there");
        source.engine.tree().unwrap()
    };

    let plan = target.engine.plan_against(&remote).unwrap();
    assert!(
        plan.iter().any(|a| a.path() == "README"),
        "the colliding path vanished from the plan: {plan:?}"
    );
}
