//! Content is stored once, not twice.
//!
//! A syncing device writes every file into a folder the user can see. Keeping
//! an encrypted copy of the same bytes in the chunk store as well means every
//! synced file costs twice its size — which is the difference between a 10 GB
//! allowance holding 10 GB of files and holding 5.
//!
//! With a tree, the file *is* the payload store for content it holds.

use qurb_storage::{ChunkKey, Store};

/// Incompressible, so nothing here is explained away by compression.
fn noisy(size: usize) -> Vec<u8> {
    seeded(size, 0x9e37_79b9)
}

/// The same, from a chosen seed. Two lengths of the default stream share a
/// prefix and therefore share their leading chunks, which is not what a test
/// about *changed* content wants.
fn seeded(size: usize, seed: u32) -> Vec<u8> {
    let mut out = vec![0u8; size];
    let mut x: u32 = seed;
    for byte in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *byte = x as u8;
    }
    out
}

fn chunk_bytes_on_disk(root: &std::path::Path) -> u64 {
    fn walk(dir: &std::path::Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
        entries
            .flatten()
            .map(|e| {
                let path = e.path();
                if path.is_dir() {
                    walk(&path)
                } else {
                    std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
                }
            })
            .sum()
    }
    walk(&root.join("chunks"))
}

/// The headline. A file in the tree costs its own size and no more.
#[test]
fn a_materialised_file_is_not_stored_twice() {
    let tree = tempfile::tempdir().unwrap();
    let store_dir = tree.path().join(".qurb");

    let data = noisy(4 * 1024 * 1024);
    let path = tree.path().join("video.bin");
    std::fs::write(&path, &data).unwrap();

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([3; 32]))
        .unwrap()
        .in_tree(tree.path());
    store.put_file("video.bin", &path).unwrap();

    let on_disk = chunk_bytes_on_disk(&store_dir);
    assert!(
        on_disk < data.len() as u64 / 10,
        "the chunk store holds {on_disk} bytes for a {} byte file it need not copy",
        data.len()
    );
}

/// And the content must still be readable — from the file, through the same
/// call everything else uses.
#[test]
fn the_content_is_still_readable_chunk_by_chunk() {
    let tree = tempfile::tempdir().unwrap();
    let store_dir = tree.path().join(".qurb");

    let data = noisy(3 * 1024 * 1024);
    let path = tree.path().join("doc.bin");
    std::fs::write(&path, &data).unwrap();

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([4; 32]))
        .unwrap()
        .in_tree(tree.path());
    store.put_file("doc.bin", &path).unwrap();

    // Establish first that there is nothing to read *but* the tree. Without
    // this the assertion below passes just as well on a store that kept a
    // second copy, and proves nothing about where the bytes came from.
    let on_disk = chunk_bytes_on_disk(&store_dir);
    assert!(on_disk < data.len() as u64 / 10, "the chunk store still holds {on_disk} bytes");

    // read_file walks the manifest and reads every chunk, so this exercises
    // the tree fallback for each one and checks the whole-file hash at the end.
    assert_eq!(store.read_file("doc.bin").unwrap(), data);
}

/// A store with no tree keeps every payload, because nothing else has them.
/// This is what a storage-only replica is.
#[test]
fn without_a_tree_every_payload_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store_dir = dir.path().join("store");
    let source = dir.path().join("elsewhere.bin");

    let data = noisy(2 * 1024 * 1024);
    std::fs::write(&source, &data).unwrap();

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([5; 32])).unwrap();
    store.put_file("elsewhere.bin", &source).unwrap();

    let on_disk = chunk_bytes_on_disk(&store_dir);
    assert!(
        on_disk > data.len() as u64 / 2,
        "a replica must keep its payloads; it holds only {on_disk} bytes"
    );
    assert_eq!(store.read_file("elsewhere.bin").unwrap(), data);
}

/// A source outside the tree must still be copied in full. Skipping the write
/// because "there is a tree" would lose content the tree does not actually
/// hold — the one way this optimisation could destroy data.
#[test]
fn a_source_outside_the_tree_is_still_stored() {
    let tree = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let store_dir = tree.path().join(".qurb");

    let data = noisy(1024 * 1024);
    let source = elsewhere.path().join("outside.bin");
    std::fs::write(&source, &data).unwrap();

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([6; 32]))
        .unwrap()
        .in_tree(tree.path());
    // Indexed under a path the tree does *not* have a file at.
    store.put_file("outside.bin", &source).unwrap();

    assert!(
        chunk_bytes_on_disk(&store_dir) > data.len() as u64 / 2,
        "content the tree does not hold was not stored"
    );
    assert_eq!(store.read_file("outside.bin").unwrap(), data);
}

/// Editing the file makes the old chunks unreadable rather than wrong.
///
/// The bytes a hash names are genuinely gone once the file changes. Returning
/// the new bytes under the old hash would corrupt whatever asked for them, so
/// the read must fail instead.
#[test]
fn an_edited_file_does_not_answer_for_its_old_chunks() {
    let tree = tempfile::tempdir().unwrap();
    let store_dir = tree.path().join(".qurb");

    let path = tree.path().join("notes.bin");
    std::fs::write(&path, noisy(1024 * 1024)).unwrap();

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([7; 32]))
        .unwrap()
        .in_tree(tree.path());
    store.put_file("notes.bin", &path).unwrap();

    let before = store.db().file_by_path("notes.bin").unwrap().unwrap().content_hash;

    // Rewrite it with different bytes, without telling the store. A different
    // seed, so not one byte of the old content survives at the front.
    std::fs::write(&path, seeded(1024 * 1024 + 7, 0x5bf0_3635)).unwrap();

    // Reading the *old* content must fail rather than return the new bytes.
    let hashes = store.chunk_hashes_for_content(&before).unwrap().unwrap();
    let outcome = store.read_chunk(&hashes[0]);
    assert!(outcome.is_err(), "an edited file answered for content it no longer holds");
}

/// An existing store, written before the tree supplied anything, sheds its
/// second copy when asked — and the content stays readable afterwards.
#[test]
fn reclaim_drops_what_the_tree_already_holds() {
    let tree = tempfile::tempdir().unwrap();
    let store_dir = tree.path().join(".qurb");

    let data = noisy(4 * 1024 * 1024);
    let path = tree.path().join("old.bin");
    std::fs::write(&path, &data).unwrap();

    // Indexed with no tree attached: both copies exist, as they did before.
    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([9; 32])).unwrap();
    store.put_file("old.bin", &path).unwrap();
    let doubled = chunk_bytes_on_disk(&store_dir);
    assert!(doubled > data.len() as u64 / 2, "the setup did not store a second copy");
    drop(store);

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([9; 32]))
        .unwrap()
        .in_tree(tree.path());
    let freed = store.reclaim().unwrap();

    assert!(freed.chunks_removed > 0, "nothing was reclaimed");
    let after = chunk_bytes_on_disk(&store_dir);
    assert!(after < data.len() as u64 / 10, "the chunk store still holds {after} bytes");
    assert!(
        freed.bytes_reclaimed >= doubled - after,
        "reported {} freed but {} left the disk",
        freed.bytes_reclaimed,
        doubled - after
    );

    // The point of the exercise: the bytes survived.
    assert_eq!(store.read_file("old.bin").unwrap(), data);
    assert!(store.verify(true).unwrap().is_healthy(), "verify is unhappy after reclaim");
}

/// A storage-only replica must not reclaim, because nothing else holds the
/// content. This is the guard against `reclaim` deleting the only copy.
#[test]
fn a_replica_with_no_tree_reclaims_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store_dir = dir.path().join("store");
    let source = dir.path().join("only.bin");

    let data = noisy(1024 * 1024);
    std::fs::write(&source, &data).unwrap();

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([11; 32])).unwrap();
    store.put_file("only.bin", &source).unwrap();

    let freed = store.reclaim().unwrap();
    assert_eq!(freed.chunks_removed, 0);
    assert!(chunk_bytes_on_disk(&store_dir) > data.len() as u64 / 2);
    assert_eq!(store.read_file("only.bin").unwrap(), data);
}

/// Content already in the folder must be recognised as held.
///
/// The caller that asks this is deciding whether to pull a file over the
/// network. A store that says "no" about content sitting in the folder does
/// not fail visibly — it silently re-transfers files the device already has.
#[test]
fn content_in_the_tree_counts_as_held() {
    let tree = tempfile::tempdir().unwrap();
    let store_dir = tree.path().join(".qurb");

    let data = noisy(2 * 1024 * 1024);
    let path = tree.path().join("held.bin");
    std::fs::write(&path, &data).unwrap();

    let mut store = Store::open(&store_dir, ChunkKey::from_bytes([13; 32]))
        .unwrap()
        .in_tree(tree.path());
    store.put_file("held.bin", &path).unwrap();

    let hash = blake3::hash(&data);
    for chunk in store.chunk_hashes_for_content(&hash).unwrap().unwrap() {
        assert!(store.has_chunk(&chunk).unwrap(), "a chunk the folder holds was called absent");
    }

    // And the whole-content read, which is the call that decides whether to
    // fetch, finds it locally.
    let mut out = Vec::new();
    assert!(
        store.read_content_into(&hash, &mut out).unwrap().is_some(),
        "content in the folder was not found by hash"
    );
    assert_eq!(out, data);
}
