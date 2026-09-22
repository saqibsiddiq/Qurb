//! Dropping local copies to stay under a limit, without losing anything.
//!
//! The cap's whole contract is that it bounds *disk*, not data. A file whose
//! bytes exist nowhere else is never a candidate, however cold it is and
//! however far over the limit the device has gone.

use qurb_storage::{ChunkKey, Store};
use qurb_sync::DeviceId;

fn noisy(size: usize, seed: u32) -> Vec<u8> {
    let mut out = vec![0u8; size];
    let mut x = seed;
    for byte in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *byte = x as u8;
    }
    out
}

struct Fixture {
    tree: tempfile::TempDir,
    store: Store,
}

fn fixture() -> Fixture {
    let tree = tempfile::tempdir().unwrap();
    let store = Store::open(&tree.path().join(".qurb"), ChunkKey::from_bytes([21; 32]))
        .unwrap()
        .in_tree(tree.path());
    Fixture { tree, store }
}

impl Fixture {
    fn write(&mut self, name: &str, size: usize, seed: u32) -> Vec<u8> {
        let data = noisy(size, seed);
        let path = self.tree.path().join(name);
        std::fs::write(&path, &data).unwrap();
        self.store.put_file(name, &path).unwrap();
        data
    }

    fn exists(&self, name: &str) -> bool {
        self.tree.path().join(name).exists()
    }
}

/// The refusal that makes the cap safe. Nothing else in this file matters if
/// this one does not hold.
#[test]
fn the_only_copy_is_never_dropped() {
    let mut f = fixture();
    f.write("precious.bin", 512 * 1024, 0x1111_1111);

    let outcome = f.store.evict("precious.bin");
    assert!(outcome.is_err(), "a file no other device holds was dropped");
    assert!(f.exists("precious.bin"), "the file was removed despite the refusal");
    assert_eq!(f.store.is_materialised("precious.bin").unwrap(), Some(true));
}

/// Once another device has taken delivery, the bytes may go.
#[test]
fn content_another_device_holds_can_be_dropped() {
    let mut f = fixture();
    let data = f.write("holiday.mp4", 512 * 1024, 0x2222_2222);
    let hash = blake3::hash(&data);
    f.store.note_replica(&hash, &DeviceId::from_bytes([9; 32])).unwrap();

    let freed = f.store.evict("holiday.mp4").unwrap();

    assert_eq!(freed, data.len() as u64);
    assert!(!f.exists("holiday.mp4"), "the bytes are still on disk");
    assert_eq!(f.store.is_materialised("holiday.mp4").unwrap(), Some(false));
}

/// Evicting frees space rather than merely moving it. With single-copy
/// storage there is no encrypted duplicate left behind to pay for.
#[test]
fn evicting_actually_frees_the_space() {
    let mut f = fixture();
    f.write("big.bin", 2 * 1024 * 1024, 0x3333_3333);
    let hash = f.store.db().file_by_path("big.bin").unwrap().unwrap().content_hash;
    f.store.note_replica(&hash, &DeviceId::from_bytes([9; 32])).unwrap();

    let before = f.store.usage().unwrap();
    f.store.evict("big.bin").unwrap();
    let after = f.store.usage().unwrap();

    assert!(before.total() >= 2 * 1024 * 1024, "usage did not account for the file");
    assert!(
        after.total() < before.total() / 10,
        "usage went from {} to {}, which is not freeing",
        before.total(),
        after.total()
    );
}

/// An evicted file is still a file. It keeps its place in the index, its
/// content hash, and its chunk list — that is what "keep the index" means.
#[test]
fn an_evicted_file_is_still_known() {
    let mut f = fixture();
    let data = f.write("notes.txt", 256 * 1024, 0x4444_4444);
    let hash = blake3::hash(&data);
    f.store.note_replica(&hash, &DeviceId::from_bytes([9; 32])).unwrap();
    f.store.evict("notes.txt").unwrap();

    assert!(
        f.store.list().unwrap().contains(&"notes.txt".to_string()),
        "an evicted file vanished from the listing"
    );
    let row = f.store.db().file_by_path("notes.txt").unwrap().unwrap();
    assert!(row.deleted_at.is_none(), "eviction wrote a tombstone");
    assert_eq!(row.content_hash, hash);
    assert!(!f.store.chunk_hashes_for_content(&hash).unwrap().unwrap().is_empty());
    assert_eq!(f.store.evicted().unwrap(), vec!["notes.txt".to_string()]);
}

/// An evicted file must not answer for its own content out of the tree. The
/// bytes are gone from this device; a read has to fail so the caller fetches.
#[test]
fn an_evicted_file_does_not_answer_for_its_chunks() {
    let mut f = fixture();
    let data = f.write("gone.bin", 512 * 1024, 0x5555_5555);
    let hash = blake3::hash(&data);
    f.store.note_replica(&hash, &DeviceId::from_bytes([9; 32])).unwrap();

    let chunks = f.store.chunk_hashes_for_content(&hash).unwrap().unwrap();
    f.store.evict("gone.bin").unwrap();

    assert!(
        f.store.read_chunk(&chunks[0]).is_err(),
        "an evicted file answered for content this device no longer holds"
    );
}

/// Candidates come coldest-first, and only content that is safe to drop is
/// offered at all.
#[test]
fn candidates_are_the_cold_ones_that_are_safe() {
    let mut f = fixture();
    let shared = f.write("shared.bin", 256 * 1024, 0x6666_6666);
    f.write("alone.bin", 256 * 1024, 0x7777_7777);
    f.store
        .note_replica(&blake3::hash(&shared), &DeviceId::from_bytes([9; 32]))
        .unwrap();

    let candidates = f.store.evictable().unwrap();
    let names: Vec<_> = candidates.iter().map(|(p, ..)| p.as_str()).collect();

    assert_eq!(names, vec!["shared.bin"], "offered a file no other device holds");
}

/// A storage-only replica holds the only copy of everything it has, so it
/// evicts nothing at all.
#[test]
fn a_replica_evicts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("payload.bin");
    let data = noisy(256 * 1024, 0x8888_8888);
    std::fs::write(&source, &data).unwrap();

    let mut store =
        Store::open(&dir.path().join("store"), ChunkKey::from_bytes([22; 32])).unwrap();
    store.put_file("payload.bin", &source).unwrap();
    store.note_replica(&blake3::hash(&data), &DeviceId::from_bytes([9; 32])).unwrap();

    assert!(store.evict("payload.bin").is_err(), "a replica dropped its only copy");
    assert_eq!(store.read_file("payload.bin").unwrap(), data);
}
