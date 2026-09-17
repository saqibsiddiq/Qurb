//! A device that holds content without a directory behind it.
//!
//! The answer to the availability gap: a peer-to-peer system where every device
//! is a person's laptop is a system where files are unreachable whenever those
//! laptops are shut. A replica is always on and holds the content so the others
//! do not have to be awake at once.
//!
//! See ../../docs/decisions/0006-availability-gap.md.

use qurb_engine::{Engine, PinSet, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;

const KEY: [u8; 32] = [42; 32];

struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
}

impl Device {
    fn syncing() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");
        let store = Store::open(&store_dir, ChunkKey::from_bytes(KEY)).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn replica(pins: PinSet) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("replica");
        fs::create_dir_all(&root).unwrap();
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes(KEY)).unwrap();
        Self { _dir: dir, root: root.clone(), engine: Engine::replica(root, store, pins) }
    }

    fn store_dir(&self) -> PathBuf {
        self.root.join(".qurb")
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn live(&self) -> Vec<String> {
        self.engine.store().db().live_paths().unwrap()
    }

    /// Files actually present on disk, ignoring the store.
    fn materialised(&self) -> Vec<String> {
        let mut out = Vec::new();
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
                    out.push(p.strip_prefix(&self.root).unwrap().to_string_lossy().to_string());
                }
            }
        }
        out.sort();
        out
    }
}

/// Pull everything `from` has into `into`.
fn pull(into: &mut Device, from: &Device) -> qurb_engine::PlanStats {
    let plan = into.engine.plan_against(&from.engine.tree().unwrap()).unwrap();
    let reader = Store::open(&from.store_dir(), ChunkKey::from_bytes(KEY)).unwrap();
    let stats = into.engine.apply_plan(&plan, &mut StoreSource::new(&reader)).unwrap();
    assert!(stats.is_clean(), "{:?}", stats.failures);
    stats
}

#[test]
fn a_replica_holds_content_without_materialising_it() {
    let mut laptop = Device::syncing();
    laptop.write("notes.txt", b"content");
    laptop.write("photos/a.jpg", b"pretend jpeg");

    let mut replica = Device::replica(PinSet::everything());
    pull(&mut replica, &laptop);

    assert_eq!(replica.live(), vec!["notes.txt".to_string(), "photos/a.jpg".to_string()]);
    assert!(
        replica.materialised().is_empty(),
        "a replica wrote files to disk: {:?}",
        replica.materialised()
    );
    assert!(replica.engine.store().verify(true).unwrap().is_healthy());
}

#[test]
fn a_replica_can_serve_back_what_it_holds() {
    // The point of the exercise. Content reaches a device that never had it,
    // from a replica rather than from the device that created it.
    let mut laptop = Device::syncing();
    laptop.write("important.txt", b"the thing that matters");

    let mut replica = Device::replica(PinSet::everything());
    pull(&mut replica, &laptop);

    let mut phone = Device::syncing();
    pull(&mut phone, &replica);

    assert_eq!(fs::read(phone.root.join("important.txt")).unwrap(), b"the thing that matters");
}

#[test]
fn a_device_recovers_everything_from_a_replica() {
    // The availability gap, closed. A device loses its store entirely and gets
    // it back from something that was merely awake.
    let mut laptop = Device::syncing();
    for i in 0..20 {
        laptop.write(&format!("work/file{i:02}.txt"), format!("contents {i}").as_bytes());
    }

    let mut replica = Device::replica(PinSet::everything());
    pull(&mut replica, &laptop);
    assert_eq!(replica.live().len(), 20);

    // A new, empty device -- the replacement for one that was lost.
    let mut replacement = Device::syncing();
    pull(&mut replacement, &replica);

    assert_eq!(replacement.live().len(), 20);
    for i in 0..20 {
        let path = format!("work/file{i:02}.txt");
        assert_eq!(
            fs::read(replacement.root.join(&path)).unwrap(),
            format!("contents {i}").as_bytes(),
            "{path} did not come back"
        );
    }
}

#[test]
fn reconciling_a_replica_does_not_delete_everything() {
    // The destructive failure this role exists to prevent. A syncing device
    // decides a file is gone by not finding it on disk; a replica has nothing on
    // disk by design, so the same inference would tombstone the entire library
    // and push those deletions to every device that trusts it.
    let mut laptop = Device::syncing();
    laptop.write("a.txt", b"one");
    laptop.write("b.txt", b"two");

    let mut replica = Device::replica(PinSet::everything());
    pull(&mut replica, &laptop);
    let before = replica.engine.tree().unwrap();

    let stats = replica.engine.reconcile().unwrap();
    assert_eq!(stats.deleted, 0, "reconciling a replica tombstoned {} paths", stats.deleted);
    assert_eq!(stats.stored, 0);
    assert_eq!(replica.engine.tree().unwrap(), before, "reconciling altered a replica's history");

    // And the laptop must see no work as a result.
    let plan = laptop.engine.plan_against(&replica.engine.tree().unwrap()).unwrap();
    assert!(plan.is_empty(), "a replica's reconcile created work: {plan:?}");
}

#[test]
fn a_replica_never_originates_a_change() {
    // It has no way to: it never writes, never deletes, and never advances its
    // own counter. So it can never win a conflict or push a change of its own.
    let mut laptop = Device::syncing();
    laptop.write("shared.txt", b"from the laptop");

    let mut replica = Device::replica(PinSet::everything());
    pull(&mut replica, &laptop);

    let replica_device = replica.engine.store().device_id().unwrap();
    for version in replica.engine.tree().unwrap() {
        assert_eq!(
            version.vector.get(&replica_device),
            0,
            "{} carries a change from the replica",
            version.path
        );
        assert_ne!(version.modified_by, replica_device);
    }
}

#[test]
fn a_partial_replica_holds_only_what_it_was_asked_to() {
    let mut laptop = Device::syncing();
    laptop.write("work/report.txt", b"keep this");
    laptop.write("work/deep/notes.txt", b"and this");
    laptop.write("photos/holiday.jpg", b"not this");
    laptop.write("workshop/plans.txt", b"nor this");

    let mut replica = Device::replica(PinSet::under(["work"]));
    pull(&mut replica, &laptop);

    let mut held = replica.live();
    held.sort();
    assert_eq!(held, vec!["work/deep/notes.txt".to_string(), "work/report.txt".to_string()]);
}

#[test]
fn a_partial_replica_stores_no_chunks_for_what_it_skipped() {
    // Skipping the path but fetching the bytes anyway would make partial
    // replication pointless -- the space is the whole reason for it.
    let mut laptop = Device::syncing();
    laptop.write("work/small.txt", b"tiny");
    laptop.write("photos/large.bin", &vec![7u8; 900_000]);

    let mut replica = Device::replica(PinSet::under(["work"]));
    pull(&mut replica, &laptop);

    let (plaintext, _stored) = replica.engine.store().db().size_totals().unwrap();
    assert!(
        plaintext < 10_000,
        "a partial replica stored {plaintext} bytes; it should hold only the pinned file"
    );
}

#[test]
fn a_deletion_reaches_a_replica() {
    let mut laptop = Device::syncing();
    laptop.write("doomed.txt", b"temporary");

    let mut replica = Device::replica(PinSet::everything());
    pull(&mut replica, &laptop);
    assert_eq!(replica.live(), vec!["doomed.txt".to_string()]);

    fs::remove_file(laptop.root.join("doomed.txt")).unwrap();
    laptop.engine.reconcile().unwrap();
    pull(&mut replica, &laptop);

    assert!(replica.live().is_empty(), "the deletion did not reach the replica");
}

#[test]
fn a_replica_can_repair_a_damaged_device() {
    // Always-on storage is also the obvious place to repair from. A device with
    // a failing disk does not have to wait for another laptop to be opened.
    let content = vec![3u8; 600_000];
    let mut laptop = Device::syncing();
    laptop.write("precious.bin", &content);

    let mut replica = Device::replica(PinSet::everything());
    pull(&mut replica, &laptop);

    // Corrupt a chunk on the laptop.
    let file = laptop.engine.store().db().file_by_path("precious.bin").unwrap().unwrap();
    let victim = laptop.engine.store().db().chunk_hashes_for(file.id).unwrap()[0];
    let hex = victim.to_hex().to_string();
    let path = laptop.store_dir().join("chunks").join(&hex[..2]).join(&hex);
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(&path, &bytes).unwrap();
    assert!(laptop.engine.store().read_file("precious.bin").is_err());

    let reader = Store::open(&replica.store_dir(), ChunkKey::from_bytes(KEY)).unwrap();
    let stats = laptop.engine.repair(&mut StoreSource::new(&reader)).unwrap();

    assert_eq!(stats.files_restored, 1);
    assert!(stats.is_clean(), "{:?}", stats.unrepairable);
    assert_eq!(laptop.engine.store().read_file("precious.bin").unwrap(), content);
}
