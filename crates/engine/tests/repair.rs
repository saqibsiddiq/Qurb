//! Recovering from a damaged disk.
//!
//! Detecting corruption is not fixing it. Until repair existed, a single bad
//! chunk made every file using it permanently unreadable on that device, even
//! while a peer held a perfect copy.
//!
//! What makes recovery possible is content addressing: a damaged chunk is not a
//! lost chunk, it is a chunk whose bytes are wrong, and the hash says exactly
//! what is wanted. Nothing has to be reconciled or agreed.

use qurb_engine::{Engine, StoreSource};
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
        let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        let path = self.root.join(rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn store_dir(&self) -> PathBuf {
        self.root.join(".qurb")
    }

    /// Damage the first chunk's worth of a file, as a failing disk would.
    ///
    /// Under single-copy storage the file in the folder *is* the payload, so
    /// silent corruption means the user's own file rather than an object in
    /// the chunk store. The modification time is put back afterwards: a
    /// corrupted file whose mtime moved would be re-indexed as an edit on the
    /// next scan, which is a different story from the one these tests tell.
    fn corrupt_a_chunk(&self, of_path: &str) -> blake3::Hash {
        let file = self.engine.store().db().file_by_path(of_path).unwrap().unwrap();
        let victim = self.engine.store().db().chunk_hashes_for(file.id).unwrap()[0];

        let path = self.root.join(of_path);
        let was = fs::metadata(&path).unwrap().modified().unwrap();
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] ^= 0xFF;
        fs::write(&path, &bytes).unwrap();
        fs::File::options().write(true).open(&path).unwrap().set_modified(was).unwrap();

        // And any copy the chunk store happens to hold, so the damage is not
        // quietly covered by a second copy that should not exist.
        let hex = victim.to_hex().to_string();
        let _ = fs::remove_file(self.store_dir().join("chunks").join(&hex[..2]).join(&hex));
        victim
    }

    /// Lose a chunk's payload outright: the file gone from the folder, and
    /// nothing in the chunk store to cover for it.
    fn delete_a_chunk(&self, of_path: &str) -> blake3::Hash {
        let file = self.engine.store().db().file_by_path(of_path).unwrap().unwrap();
        let victim = self.engine.store().db().chunk_hashes_for(file.id).unwrap()[0];

        fs::remove_file(self.root.join(of_path)).unwrap();
        let hex = victim.to_hex().to_string();
        let _ = fs::remove_file(self.store_dir().join("chunks").join(&hex[..2]).join(&hex));
        victim
    }
}

/// Pseudo-random bytes large enough to span several chunks.
fn data(seed: u32, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut x = seed | 1;
    for _ in 0..len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.push(x as u8);
    }
    out
}

/// A healthy peer holding the same content.
fn healthy_peer(files: &[(&str, &[u8])]) -> Device {
    let mut peer = Device::new();
    for (path, content) in files {
        peer.write(path, content);
    }
    peer
}

#[test]
fn a_corrupt_chunk_is_detected_and_refetched() {
    let content = data(1, 2 << 20);
    let peer = healthy_peer(&[("important.bin", &content)]);

    let mut damaged = Device::new();
    damaged.write("important.bin", &content);
    damaged.corrupt_a_chunk("important.bin");

    // The damage is real: the file no longer reads.
    assert!(damaged.engine.store().read_file("important.bin").is_err());

    let reader = Store::open(&peer.store_dir(), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&peer.root);
    let mut source = StoreSource::new(&reader);
    let stats = damaged.engine.repair(&mut source).unwrap();

    assert_eq!(stats.chunks_repaired, 1);
    assert_eq!(stats.files_restored, 1);
    assert!(stats.is_clean(), "{:?}", stats.unrepairable);

    assert_eq!(damaged.engine.store().read_file("important.bin").unwrap(), content);
    assert!(damaged.engine.store().verify(true).unwrap().is_healthy());
    assert_eq!(fs::read(damaged.root.join("important.bin")).unwrap(), content);
}

#[test]
fn a_missing_chunk_is_refetched_too() {
    let content = data(2, 2 << 20);
    let peer = healthy_peer(&[("gone.bin", &content)]);

    let mut damaged = Device::new();
    damaged.write("gone.bin", &content);
    damaged.delete_a_chunk("gone.bin");

    let reader = Store::open(&peer.store_dir(), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&peer.root);
    let mut source = StoreSource::new(&reader);
    let stats = damaged.engine.repair(&mut source).unwrap();

    assert_eq!(stats.files_restored, 1);
    assert_eq!(damaged.engine.store().read_file("gone.bin").unwrap(), content);
    assert!(damaged.engine.store().verify(true).unwrap().is_healthy());
}

#[test]
fn repairing_a_healthy_store_does_nothing() {
    let content = data(3, 1 << 20);
    let peer = healthy_peer(&[("fine.bin", &content)]);

    let mut device = Device::new();
    device.write("fine.bin", &content);

    let reader = Store::open(&peer.store_dir(), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&peer.root);
    let mut source = StoreSource::new(&reader);
    let stats = device.engine.repair(&mut source).unwrap();

    assert!(stats.did_nothing(), "{stats:?}");
}

#[test]
fn one_bad_chunk_shared_by_several_files_repairs_all_of_them() {
    // Deduplication means damage is shared. A chunk belonging to three files
    // takes all three down, and repairing it has to bring all three back.
    let content = data(4, 2 << 20);
    let peer = healthy_peer(&[
        ("one.bin", &content),
        ("two.bin", &content),
        ("three.bin", &content),
    ]);

    let mut damaged = Device::new();
    damaged.write("one.bin", &content);
    damaged.write("two.bin", &content);
    damaged.write("three.bin", &content);
    damaged.corrupt_a_chunk("one.bin");

    for path in ["one.bin", "two.bin", "three.bin"] {
        assert!(damaged.engine.store().read_file(path).is_err(), "{path} should be broken");
    }

    let reader = Store::open(&peer.store_dir(), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&peer.root);
    let mut source = StoreSource::new(&reader);
    let stats = damaged.engine.repair(&mut source).unwrap();

    assert_eq!(stats.chunks_repaired, 1, "one chunk was damaged");
    assert_eq!(stats.files_restored, 3, "three files depended on it");
    for path in ["one.bin", "two.bin", "three.bin"] {
        assert_eq!(damaged.engine.store().read_file(path).unwrap(), content);
    }
}

#[test]
fn repair_does_not_disturb_undamaged_files() {
    let broken = data(5, 2 << 20);
    let intact = data(6, 2 << 20);
    let peer = healthy_peer(&[("broken.bin", &broken), ("intact.bin", &intact)]);

    let mut damaged = Device::new();
    damaged.write("broken.bin", &broken);
    damaged.write("intact.bin", &intact);

    let before = damaged.engine.tree().unwrap();
    damaged.corrupt_a_chunk("broken.bin");

    let reader = Store::open(&peer.store_dir(), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&peer.root);
    let mut source = StoreSource::new(&reader);
    damaged.engine.repair(&mut source).unwrap();

    assert_eq!(damaged.engine.store().read_file("intact.bin").unwrap(), intact);

    // Repair is not an edit. Stamping a new version would make recovering from
    // a bad disk look like a change and push it to every peer.
    let after = damaged.engine.tree().unwrap();
    assert_eq!(before, after, "repair altered the recorded history");
}

#[test]
fn a_source_with_nothing_to_offer_leaves_the_damage_reported() {
    // Repair must not claim success it did not achieve. A device whose only
    // peer is offline stays broken, and says so.
    let content = data(7, 1 << 20);
    let mut damaged = Device::new();
    damaged.write("orphaned.bin", &content);
    damaged.corrupt_a_chunk("orphaned.bin");

    let mut source = qurb_engine::NoContent;
    let stats = damaged.engine.repair(&mut source).unwrap();

    assert_eq!(stats.files_restored, 0);
    assert_eq!(stats.unrepairable.len(), 1);
    assert_eq!(stats.unrepairable[0].0, "orphaned.bin");
    assert!(!stats.is_clean());
}

#[test]
fn a_source_that_sends_the_wrong_bytes_is_refused() {
    // The peer is not trusted. Writing unverified bytes over a file already
    // known to be damaged would turn a detectable problem into an undetectable
    // one.
    struct Liar;
    impl qurb_engine::ContentSource for Liar {
        fn fetch(&mut self, _hash: &[u8; 32], _size: u64) -> qurb_engine::Result<Vec<u8>> {
            Ok(b"convincing but wrong".to_vec())
        }
    }

    let content = data(8, 1 << 20);
    let mut damaged = Device::new();
    damaged.write("target.bin", &content);
    damaged.corrupt_a_chunk("target.bin");

    let stats = damaged.engine.repair(&mut Liar).unwrap();
    assert_eq!(stats.files_restored, 0, "bad content was accepted");
    assert_eq!(stats.unrepairable.len(), 1);
    assert!(
        stats.unrepairable[0].1.contains("not what was asked for"),
        "got {:?}",
        stats.unrepairable[0].1
    );
}

#[test]
fn repair_can_be_run_twice() {
    let content = data(9, 2 << 20);
    let peer = healthy_peer(&[("f.bin", &content)]);

    let mut damaged = Device::new();
    damaged.write("f.bin", &content);
    damaged.corrupt_a_chunk("f.bin");

    let reader = Store::open(&peer.store_dir(), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&peer.root);
    let mut source = StoreSource::new(&reader);

    assert_eq!(damaged.engine.repair(&mut source).unwrap().files_restored, 1);
    assert!(damaged.engine.repair(&mut source).unwrap().did_nothing(), "second pass found work");
}
