//! Large, disruptive changes.
//!
//! A user renames a directory holding thousands of files, or moves a tree while
//! a sync is in flight. Each of these turns one gesture into thousands of path
//! changes, and each has a plausible-looking implementation that loses data.

use qurb_engine::{Engine, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;

const COUNT: usize = 2_000;

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

    fn store_dir(&self) -> PathBuf {
        self.root.join(".qurb")
    }

    fn populate(&mut self, dir: &str, count: usize) {
        for i in 0..count {
            let path = self.root.join(dir).join(format!("sub{:02}", i % 10)).join(format!("f{i:05}.bin"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("contents of file {i}").repeat(20)).unwrap();
        }
        self.engine.reconcile().unwrap();
    }

    fn live(&self) -> Vec<String> {
        self.engine.store().db().live_paths().unwrap()
    }
}

/// One exchange in both directions.
fn sync(a: &mut Device, b: &mut Device) {
    for _ in 0..3 {
        let plan = b.engine.plan_against(&a.engine.tree().unwrap()).unwrap();
        let reader = Store::open(&a.store_dir(), ChunkKey::from_bytes([42; 32])).unwrap();
        let stats = b.engine.apply_plan(&plan, &mut StoreSource::new(&reader)).unwrap();
        assert!(stats.is_clean(), "{:?}", stats.failures);

        let plan = a.engine.plan_against(&b.engine.tree().unwrap()).unwrap();
        let reader = Store::open(&b.store_dir(), ChunkKey::from_bytes([42; 32])).unwrap();
        let stats = a.engine.apply_plan(&plan, &mut StoreSource::new(&reader)).unwrap();
        assert!(stats.is_clean(), "{:?}", stats.failures);

        if a.engine.plan_against(&b.engine.tree().unwrap()).unwrap().is_empty()
            && b.engine.plan_against(&a.engine.tree().unwrap()).unwrap().is_empty()
        {
            return;
        }
    }
    panic!("did not converge");
}

#[test]
fn renaming_a_directory_of_thousands_of_files_propagates() {
    let mut a = Device::new();
    a.populate("project", COUNT);

    let mut b = Device::new();
    sync(&mut a, &mut b);
    assert_eq!(b.live().len(), COUNT);

    // One gesture, thousands of path changes.
    fs::rename(a.root.join("project"), a.root.join("archive")).unwrap();
    let stats = a.engine.reconcile().unwrap();
    assert_eq!(stats.stored, COUNT, "every file should appear at its new path");
    assert_eq!(stats.deleted, COUNT, "every old path should be tombstoned");

    sync(&mut a, &mut b);

    let live = b.live();
    assert_eq!(live.len(), COUNT, "got {} paths", live.len());
    assert!(live.iter().all(|p| p.starts_with("archive/")), "old paths survived");
    assert!(b.root.join("archive").exists());
    assert!(
        !b.root.join("project").exists(),
        "the old directory was left behind as an empty skeleton"
    );
}

#[test]
fn a_rename_moves_no_content_over_the_wire() {
    // The payoff of addressing content by hash: the bytes are already on the
    // other device under the old name, so a rename is metadata only. Getting
    // this wrong re-transfers an entire library because someone tidied up.
    //
    // Both directions are tested because the original bug only appeared in one.
    // A plan is applied in path order, so renaming `project` to `archive` put
    // the additions first and found the content locally, while renaming it to
    // `renamed` put the deletions first and threw the content away just before
    // wanting it. Whether a rename was free depended on the alphabet.
    for new_name in ["archive", "renamed"] {
        let mut a = Device::new();
        a.populate("project", 300);

        let mut b = Device::new();
        sync(&mut a, &mut b);

        fs::rename(a.root.join("project"), a.root.join(new_name)).unwrap();
        a.engine.reconcile().unwrap();

        let plan = b.engine.plan_against(&a.engine.tree().unwrap()).unwrap();
        let reader = Store::open(&a.store_dir(), ChunkKey::from_bytes([42; 32])).unwrap();
        let stats = b.engine.apply_plan(&plan, &mut StoreSource::new(&reader)).unwrap();

        assert!(stats.is_clean());
        assert_eq!(
            stats.fetched, 0,
            "renaming to {new_name:?} fetched {} chunk(s) for a pure rename",
            stats.fetched
        );
        assert_eq!(b.live().len(), 300);
    }
}

#[test]
fn a_directory_renamed_between_planning_and_applying_is_survivable() {
    // The real race: a plan names paths, and the filesystem changes underneath
    // before the plan runs. Nothing may be lost, and the next pass must settle
    // it.
    let mut a = Device::new();
    a.populate("project", 200);

    let mut b = Device::new();
    sync(&mut a, &mut b);

    // A edits, so B has genuine work to do.
    for i in 0..200 {
        let path = a.root.join("project").join(format!("sub{:02}", i % 10)).join(format!("f{i:05}.bin"));
        fs::write(path, format!("edited {i}")).unwrap();
    }
    a.engine.reconcile().unwrap();

    let plan = b.engine.plan_against(&a.engine.tree().unwrap()).unwrap();
    assert!(!plan.is_empty());

    // Now B's own tree moves out from under the plan it is about to apply.
    fs::rename(b.root.join("project"), b.root.join("moved")).unwrap();

    let reader = Store::open(&a.store_dir(), ChunkKey::from_bytes([42; 32])).unwrap();
    let stats = b.engine.apply_plan(&plan, &mut StoreSource::new(&reader)).unwrap();
    assert!(stats.is_clean(), "applying onto a moved tree failed: {:?}", stats.failures);

    // B now has both the re-created originals and the moved copies. The next
    // reconciliation and sync must settle it without losing anything.
    b.engine.reconcile().unwrap();
    sync(&mut a, &mut b);

    assert_eq!(a.live(), b.live(), "the devices disagree after the race");
    assert!(a.engine.store().verify(true).unwrap().is_healthy());
    assert!(b.engine.store().verify(true).unwrap().is_healthy());
}

#[test]
fn deleting_a_whole_directory_propagates_as_deletions() {
    let mut a = Device::new();
    a.populate("doomed", COUNT);
    a.populate("keep", 50);

    let mut b = Device::new();
    sync(&mut a, &mut b);
    assert_eq!(b.live().len(), COUNT + 50);

    fs::remove_dir_all(a.root.join("doomed")).unwrap();
    let stats = a.engine.reconcile().unwrap();
    assert_eq!(stats.deleted, COUNT);

    sync(&mut a, &mut b);

    let live = b.live();
    assert_eq!(live.len(), 50, "got {} paths", live.len());
    assert!(live.iter().all(|p| p.starts_with("keep/")));
    assert!(!b.root.join("doomed").exists(), "the deleted directory was left behind");
}

#[test]
fn a_directory_swapped_with_another_converges() {
    // Two directories exchanging names. Every path in both changes meaning at
    // once, which breaks any scheme that tracks files by path alone.
    let mut a = Device::new();
    a.populate("alpha", 100);
    a.populate("beta", 100);

    let mut b = Device::new();
    sync(&mut a, &mut b);

    fs::rename(a.root.join("alpha"), a.root.join("tmp")).unwrap();
    fs::rename(a.root.join("beta"), a.root.join("alpha")).unwrap();
    fs::rename(a.root.join("tmp"), a.root.join("beta")).unwrap();
    a.engine.reconcile().unwrap();

    sync(&mut a, &mut b);

    assert_eq!(a.live(), b.live());
    for path in a.live() {
        let on_a = fs::read(a.root.join(&path)).unwrap();
        let on_b = fs::read(b.root.join(&path)).unwrap();
        assert_eq!(on_a, on_b, "{path} differs after the swap");
    }
}
