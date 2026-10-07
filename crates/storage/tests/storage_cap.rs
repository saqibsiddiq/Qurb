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

/// A copy only a phone holds is not a safe one: one tap in the phone's
/// settings erases it (decision 0053). A computer's copy is.
#[test]
fn a_copy_only_a_phone_holds_does_not_let_the_bytes_go() {
    let mut f = fixture();
    let data = f.write("tickets.pdf", 256 * 1024, 0x3333_3333);
    let hash = blake3::hash(&data);
    let phone = DeviceId::from_bytes([7; 32]);
    f.store.db().trust_peer(&phone, &[7; 32], "phone").unwrap();
    f.store.db().set_peer_kind(&phone, "phone").unwrap();
    f.store.note_replica(&hash, &phone).unwrap();

    assert!(f.store.evictable().unwrap().is_empty(), "offered for eviction on a phone's copy");
    assert_eq!(f.store.db().freeable(10).unwrap().count, 0, "offered as freeable on a phone's copy");
    let refused = f.store.free_local("tickets.pdf").unwrap_err().to_string();
    assert!(refused.contains("only a phone"), "{refused}");
    assert!(f.exists("tickets.pdf"));

    let laptop = DeviceId::from_bytes([8; 32]);
    f.store.db().trust_peer(&laptop, &[8; 32], "laptop").unwrap();
    f.store.db().set_peer_kind(&laptop, "computer").unwrap();
    f.store.note_replica(&hash, &laptop).unwrap();
    assert_eq!(f.store.free_local("tickets.pdf").unwrap(), data.len() as u64);
}

/// A phone lets the first computer it learns of keep its vault, once: a
/// choice made afterwards is not overruled (decision 0053).
#[test]
fn a_phone_has_its_first_computer_keep_its_vault_once() {
    let f = fixture();
    f.store.db().set_local_kind("phone").unwrap();
    let laptop = DeviceId::from_bytes([8; 32]);
    let desk = DeviceId::from_bytes([9; 32]);
    for (device, name) in [(&laptop, "laptop"), (&desk, "desk")] {
        f.store.db().trust_peer(device, device.as_bytes(), name).unwrap();
    }

    assert!(!f.store.learn_kind(&laptop, "phone").unwrap(), "a phone is no keeper");
    assert!(f.store.learn_kind(&laptop, "computer").unwrap());
    assert_eq!(f.store.db().holders().unwrap(), vec![laptop]);
    assert_eq!(f.store.db().peer_kind(&laptop).unwrap().as_deref(), Some("computer"));

    // Removed by the person; a second computer does not bring the default back.
    f.store.db().remove_holder(&laptop).unwrap();
    assert!(!f.store.learn_kind(&desk, "computer").unwrap());
    assert!(f.store.db().holders().unwrap().is_empty());
}

/// What a wipe would lose: a file no other device holds is listed, and stops
/// being listed when one says it has it -- even a phone, whose copy is still a
/// copy if this device is the one wiped (decision 0053).
#[test]
fn only_here_lists_what_a_wipe_would_lose() {
    let mut f = fixture();
    let data = f.write("only.jpg", 64 * 1024, 0x4444_4444);
    f.write("big.bin", 128 * 1024, 0x5555_5555);
    let names = |f: &Fixture| f.store.only_here().unwrap().into_iter().map(|(p, _, _)| p).collect::<Vec<_>>();
    assert_eq!(names(&f), vec!["big.bin".to_string(), "only.jpg".to_string()], "largest first");

    let phone = DeviceId::from_bytes([7; 32]);
    f.store.db().trust_peer(&phone, &[7; 32], "phone").unwrap();
    f.store.db().set_peer_kind(&phone, "phone").unwrap();
    f.store.note_replica(&blake3::hash(&data), &phone).unwrap();
    assert_eq!(names(&f), vec!["big.bin".to_string()]);
}

/// A computer makes nobody its keeper by default: it keeps its own files.
#[test]
fn a_computer_keeps_no_default_keeper() {
    let f = fixture();
    f.store.db().set_local_kind("computer").unwrap();
    let laptop = DeviceId::from_bytes([8; 32]);
    f.store.db().trust_peer(&laptop, &[8; 32], "laptop").unwrap();
    assert!(!f.store.learn_kind(&laptop, "computer").unwrap());
    assert!(f.store.db().holders().unwrap().is_empty());
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

/// A store with no folder must not report its content twice.
///
/// Every file in a replica is "held", and every byte of it is in the chunk
/// store. Counting both makes a replica look like it is using twice the disk
/// it is — which on a small always-on box is the difference between fitting
/// and not.
#[test]
fn a_replica_counts_its_bytes_once() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("payload.bin");
    let data = noisy(1024 * 1024, 0x1234_5678);
    std::fs::write(&source, &data).unwrap();

    let mut store =
        Store::open(&dir.path().join("store"), ChunkKey::from_bytes([31; 32])).unwrap();
    store.put_file("payload.bin", &source).unwrap();

    let usage = store.usage().unwrap();
    assert_eq!(usage.files, 0, "a store with no folder reported bytes held in one");
    assert!(usage.chunks > data.len() as u64 / 2, "the payload was not counted at all");
    assert!(
        usage.total() < data.len() as u64 * 3 / 2,
        "{} counted for {} of content",
        usage.total(),
        data.len()
    );
}

/// A dropped file fetched back is held again.
///
/// The fetch writes the same bytes the index already records, so it looks
/// like an unchanged file. Unless that still marks the file as held, it stays
/// "not here" with its bytes sitting in the folder: never a candidate for
/// eviction again, counted as missing, and -- worst -- a later deletion of it
/// is taken for the cap's own doing and never reaches the other devices.
#[test]
fn a_file_fetched_back_is_held_again() {
    let mut f = fixture();
    let data = f.write("notes.txt", 256 * 1024, 0x5555_5555);
    f.store.note_replica(&blake3::hash(&data), &DeviceId::from_bytes([9; 32])).unwrap();
    f.store.evict("notes.txt").unwrap();
    assert_eq!(f.store.is_materialised("notes.txt").unwrap(), Some(false));

    // Fetched back the way the engine does it: written into the folder, then
    // adopted with the version it already had.
    let path = f.tree.path().join("notes.txt");
    std::fs::write(&path, &data).unwrap();
    let version = f.store.db().version("notes.txt").unwrap().unwrap();
    f.store.db().want("notes.txt").unwrap();
    f.store.adopt_file(&version, &path, 0).unwrap();

    assert_eq!(
        f.store.is_materialised("notes.txt").unwrap(),
        Some(true),
        "fetched back but still marked as dropped"
    );
    assert!(f.store.evicted().unwrap().is_empty());
    assert!(f.store.db().wanted_paths().unwrap().is_empty(), "still asking for it after it arrived");
}

/// What the Storage screen offers to free is exactly what freeing will
/// accept: bytes here that another device holds, biggest first -- never the
/// only copy, and never a file already freed.
#[test]
fn what_can_be_freed_is_only_what_another_device_holds() {
    let mut f = fixture();
    let phone = DeviceId::from_bytes([9; 32]);
    let small = f.write("small.bin", 64 * 1024, 0x3333_3333);
    let large = f.write("large.bin", 256 * 1024, 0x4444_4444);
    f.write("only-here.bin", 128 * 1024, 0x5555_5555);
    f.store.note_replica(&blake3::hash(&small), &phone).unwrap();
    f.store.note_replica(&blake3::hash(&large), &phone).unwrap();

    let offered = f.store.db().freeable(10).unwrap();
    assert_eq!(offered.count, 2);
    assert_eq!(offered.bytes, (64 + 256) * 1024);
    let names: Vec<&str> = offered.files.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(names, ["large.bin", "small.bin"], "biggest first, and not the only copy");

    for entry in &offered.files {
        f.store.free_local(&entry.path).unwrap();
    }
    let after = f.store.db().freeable(10).unwrap();
    assert_eq!((after.count, after.bytes), (0, 0), "a freed file is offered again");
}

/// A file's details name the devices that hold it, and not one that holds it
/// only in somebody's vault, which could not hand it back.
#[test]
fn a_file_says_which_devices_hold_it() {
    let mut f = fixture();
    let data = f.write("report.pdf", 32 * 1024, 0x6666_6666);
    let hash = blake3::hash(&data);
    let (phone, laptop, vault) =
        (DeviceId::from_bytes([1; 32]), DeviceId::from_bytes([2; 32]), DeviceId::from_bytes([3; 32]));
    for (device, seed) in [(&phone, 1u8), (&laptop, 2), (&vault, 3)] {
        f.store.db().trust_peer(device, &[seed; 32], "a device").unwrap();
    }
    f.store.note_replica(&hash, &phone).unwrap();
    f.store.note_replica(&hash, &laptop).unwrap();
    f.store.note_replica_in_vault(&hash, &vault).unwrap();

    let mut holders = f.store.db().holders_of_content(&hash).unwrap();
    holders.sort_by_key(|d| d.to_string());
    assert_eq!(holders, [phone, laptop]);
}
