//! Integration tests for the storage layer.
//!
//! The garbage collection tests carry most of the weight here. Deduplication
//! means a chunk can belong to several files, so a collector that is even
//! slightly wrong destroys data in a file nobody touched — and does it without
//! any error being raised. These tests exist to make that failure loud.

use qurb_storage::{ChunkKey, Store};
use std::time::Duration;

const HOUR: Duration = Duration::from_secs(3600);
const NOW: Duration = Duration::ZERO;

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("store"), ChunkKey::generate()).unwrap();
    (dir, store)
}

/// Deterministic pseudo-random bytes: incompressible, so chunk counts are
/// driven by the chunker rather than by zstd, and reproducible across runs.
fn data(seed: u32, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    for _ in 0..len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.push(x as u8);
    }
    out
}

// -- basics ------------------------------------------------------------------

#[test]
fn round_trip_preserves_bytes() {
    let (_d, mut s) = store();
    let content = data(1, 3 << 20);

    let stats = s.put_bytes("a.bin", &content, 0).unwrap();
    assert!(stats.chunks_total > 1, "3 MiB should span several chunks");
    assert_eq!(stats.chunks_written, stats.chunks_total);

    assert_eq!(s.read_file("a.bin").unwrap(), content);
    assert!(s.verify(true).unwrap().is_healthy());
}

#[test]
fn empty_file_round_trips() {
    let (_d, mut s) = store();
    s.put_bytes("empty", b"", 0).unwrap();
    assert_eq!(s.read_file("empty").unwrap(), Vec::<u8>::new());
}

#[test]
fn storing_identical_content_twice_writes_no_new_payloads() {
    let (_d, mut s) = store();
    let content = data(2, 2 << 20);

    let first = s.put_bytes("one.bin", &content, 0).unwrap();
    let second = s.put_bytes("two.bin", &content, 0).unwrap();

    assert!(first.chunks_written > 0);
    assert_eq!(second.chunks_written, 0, "every chunk was already held");
    assert_eq!(second.bytes_written, 0);
    assert_eq!(s.read_file("two.bin").unwrap(), content);
}

#[test]
fn rewriting_the_same_path_with_the_same_content_is_a_no_op() {
    let (_d, mut s) = store();
    let content = data(3, 1 << 20);
    s.put_bytes("x.bin", &content, 100).unwrap();

    let again = s.put_bytes("x.bin", &content, 200).unwrap();
    assert!(again.unchanged);
    assert_eq!(again.chunks_written, 0);
}

#[test]
fn payload_on_disk_is_not_the_plaintext() {
    // Encryption at rest is the point; a regression here would be silent.
    let (_d, mut s) = store();
    let needle = b"CANARY-c0ffee-SECRET";
    let mut content = data(4, 512 << 10);
    content[1000..1000 + needle.len()].copy_from_slice(needle);
    s.put_bytes("secret.bin", &content, 0).unwrap();

    let mut found = false;
    for entry in walkdir(s.cas().root()) {
        let bytes = std::fs::read(&entry).unwrap();
        if bytes.windows(needle.len()).any(|w| w == needle) {
            found = true;
        }
    }
    assert!(!found, "plaintext must never appear in the chunk store");
}

fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            if e.path().is_dir() {
                stack.push(e.path())
            } else {
                out.push(e.path())
            }
        }
    }
    out
}

// -- reference counting ------------------------------------------------------

#[test]
fn refcounts_track_shared_chunks() {
    let (_d, mut s) = store();
    let content = data(5, 2 << 20);

    s.put_bytes("a.bin", &content, 0).unwrap();
    let hashes = {
        let f = s.db().file_by_path("a.bin").unwrap().unwrap();
        s.db().chunk_hashes_for(f.id).unwrap()
    };
    for h in &hashes {
        assert_eq!(s.db().chunk(h).unwrap().unwrap().refcount, 1);
    }

    s.put_bytes("b.bin", &content, 0).unwrap();
    for h in &hashes {
        assert_eq!(s.db().chunk(h).unwrap().unwrap().refcount, 2, "shared by two files");
    }

    assert!(s.db().audit_refcounts().unwrap().is_empty());
}

#[test]
fn refcounts_survive_many_mixed_operations() {
    // The triggers should make drift impossible. This asserts it across a
    // workload that overwrites, deletes, restores and re-adds.
    let (_d, mut s) = store();

    for round in 0..6u32 {
        for i in 0..5u32 {
            let path = format!("f{i}.bin");
            s.put_bytes(&path, &data(round * 10 + i, 400 << 10), 0).unwrap();
        }
        s.delete_file("f1.bin").unwrap();
        s.restore_file("f1.bin").unwrap();
        s.delete_file("f2.bin").unwrap();
        s.put_bytes("f2.bin", &data(round, 300 << 10), 0).unwrap();
    }

    let drift = s.db().audit_refcounts().unwrap();
    assert!(drift.is_empty(), "reference counts drifted: {drift:?}");
    assert!(s.verify(true).unwrap().is_healthy());
}

#[test]
fn a_file_repeating_a_chunk_internally_stores_it_once() {
    // The repeated region has to span several chunks for this to show anything.
    // After a repeat begins, the chunker needs a chunk or two to resynchronise
    // its boundaries with the earlier copy; below about 2 MiB the whole file is
    // too few chunks for that to happen at all, and nothing dedups.
    let (_d, mut s) = store();
    let block = data(6, 8 << 20);
    let mut content = block.clone();
    content.extend_from_slice(&block);

    let stats = s.put_bytes("doubled.bin", &content, 0).unwrap();
    assert!(
        stats.bytes_deduplicated > (6 << 20),
        "most of the repeated 8 MiB should dedup, got {}",
        stats.bytes_deduplicated
    );
    assert_eq!(s.read_file("doubled.bin").unwrap(), content);
    assert!(s.db().audit_refcounts().unwrap().is_empty());
}

// -- deletion and retention --------------------------------------------------

#[test]
fn delete_is_a_tombstone_and_is_reversible() {
    let (_d, mut s) = store();
    let content = data(7, 1 << 20);
    s.put_bytes("gone.bin", &content, 0).unwrap();

    s.delete_file("gone.bin").unwrap();
    assert!(s.read_file("gone.bin").is_err(), "a deleted file must not read back");
    assert!(!s.list().unwrap().contains(&"gone.bin".to_string()));

    s.restore_file("gone.bin").unwrap();
    assert_eq!(s.read_file("gone.bin").unwrap(), content, "content survived the tombstone");
}

#[test]
fn gc_within_the_retention_window_reclaims_nothing() {
    let (_d, mut s) = store();
    s.put_bytes("keep.bin", &data(8, 1 << 20), 0).unwrap();
    s.delete_file("keep.bin").unwrap();

    let stats = s.gc(HOUR).unwrap();
    assert_eq!(stats.tombstones_expired, 0);
    assert_eq!(stats.chunks_removed, 0);

    s.restore_file("keep.bin").unwrap();
    assert!(s.verify(true).unwrap().is_healthy());
}

#[test]
fn gc_past_retention_reclaims_the_chunks() {
    let (_d, mut s) = store();
    let content = data(9, 2 << 20);
    s.put_bytes("temp.bin", &content, 0).unwrap();
    let before = s.db().chunk_count().unwrap();
    assert!(before > 0);

    s.delete_file("temp.bin").unwrap();

    // With a zero window both stages fire in one pass: stage 1 expires the
    // tombstone and releases the references, and stage 2's cutoff is late
    // enough to collect what stage 1 just released. A non-zero window would
    // split this across two passes.
    let stats = s.gc(NOW).unwrap();
    assert_eq!(stats.tombstones_expired, 1);
    assert_eq!(stats.chunks_removed as i64, before);
    assert!(stats.bytes_reclaimed > 0);

    assert_eq!(s.db().chunk_count().unwrap(), 0);
    assert!(s.cas().iter_hashes().unwrap().is_empty(), "payloads must be gone from disk too");
    assert!(s.db().audit_refcounts().unwrap().is_empty());
    assert!(s.verify(true).unwrap().is_healthy());
}

/// The test this whole module exists for.
#[test]
fn gc_never_removes_a_chunk_a_live_file_still_needs() {
    let (_d, mut s) = store();
    let shared = data(10, 3 << 20);

    s.put_bytes("keeper.bin", &shared, 0).unwrap();
    s.put_bytes("doomed.bin", &shared, 0).unwrap();

    s.delete_file("doomed.bin").unwrap();
    for _ in 0..3 {
        s.gc(NOW).unwrap();
    }

    // Every chunk is still referenced by keeper.bin, so nothing may have gone.
    assert_eq!(
        s.read_file("keeper.bin").unwrap(),
        shared,
        "collecting a deleted file destroyed a live file's data"
    );
    let report = s.verify(true).unwrap();
    assert!(report.is_healthy(), "{report:?}");
    assert!(report.missing.is_empty());
    assert!(s.db().audit_refcounts().unwrap().is_empty());
}

#[test]
fn overwriting_a_file_releases_only_the_chunks_it_stops_using() {
    let (_d, mut s) = store();

    // v2 shares its beginning with v1, so content-defined chunking should leave
    // the early chunks in place and release only the tail.
    let v1 = data(11, 4 << 20);
    let mut v2 = v1[..3 << 20].to_vec();
    v2.extend_from_slice(&data(12, 1 << 20));

    s.put_bytes("doc.bin", &v1, 0).unwrap();
    let v1_chunks = {
        let f = s.db().file_by_path("doc.bin").unwrap().unwrap();
        s.db().chunk_hashes_for(f.id).unwrap()
    };

    s.put_bytes("doc.bin", &v2, 1).unwrap();
    let v2_chunks = {
        let f = s.db().file_by_path("doc.bin").unwrap().unwrap();
        s.db().chunk_hashes_for(f.id).unwrap()
    };

    let shared = v1_chunks.iter().filter(|h| v2_chunks.contains(h)).count();
    assert!(shared > 0, "a shared prefix should produce shared chunks");

    // Chunks only v1 used are now unreferenced but still within retention.
    for h in v1_chunks.iter().filter(|h| !v2_chunks.contains(h)) {
        let row = s.db().chunk(h).unwrap().unwrap();
        assert_eq!(row.refcount, 0);
        assert!(row.unreferenced_at.is_some(), "the retention clock must be running");
    }
    // Chunks both versions use must not have been released.
    for h in v1_chunks.iter().filter(|h| v2_chunks.contains(h)) {
        assert!(s.db().chunk(h).unwrap().unwrap().refcount > 0);
    }

    assert_eq!(s.read_file("doc.bin").unwrap(), v2);
    assert!(s.db().audit_refcounts().unwrap().is_empty());
}

#[test]
fn superseded_chunks_are_reclaimed_once_retention_passes() {
    let (_d, mut s) = store();
    s.put_bytes("doc.bin", &data(13, 2 << 20), 0).unwrap();
    s.put_bytes("doc.bin", &data(14, 2 << 20), 1).unwrap();

    let before = s.db().chunk_count().unwrap();
    let stats = s.gc(NOW).unwrap();
    assert!(stats.chunks_removed > 0, "the previous version's chunks should go");
    assert!(s.db().chunk_count().unwrap() < before);
    assert!(s.verify(true).unwrap().is_healthy(), "the live version must be intact");
}

// -- verification ------------------------------------------------------------

#[test]
fn verify_detects_a_missing_chunk() {
    let (_d, mut s) = store();
    s.put_bytes("a.bin", &data(15, 1 << 20), 0).unwrap();

    let f = s.db().file_by_path("a.bin").unwrap().unwrap();
    let victim = s.db().chunk_hashes_for(f.id).unwrap()[0];
    s.cas().remove(&victim).unwrap();

    let report = s.verify(false).unwrap();
    assert!(!report.is_healthy());
    assert_eq!(report.missing, vec![victim]);
    assert!(s.read_file("a.bin").is_err());
}

#[test]
fn verify_detects_a_corrupted_chunk() {
    let (_d, mut s) = store();
    s.put_bytes("a.bin", &data(16, 1 << 20), 0).unwrap();

    let f = s.db().file_by_path("a.bin").unwrap().unwrap();
    let victim = s.db().chunk_hashes_for(f.id).unwrap()[0];

    // Flip a bit in the stored payload, as a failing disk would.
    let path = walkdir(s.cas().root())
        .into_iter()
        .find(|p| p.file_name().unwrap().to_str().unwrap() == victim.to_hex().as_str())
        .unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    std::fs::write(&path, &bytes).unwrap();

    let shallow = s.verify(false).unwrap();
    assert!(shallow.is_healthy(), "a shallow pass only checks presence");

    let deep = s.verify(true).unwrap();
    assert_eq!(deep.corrupt, vec![victim]);
    assert!(!deep.is_healthy());
}

#[test]
fn orphaned_payloads_are_found_and_swept() {
    let (_d, mut s) = store();
    s.put_bytes("a.bin", &data(17, 1 << 20), 0).unwrap();

    // A payload written by an interrupted put, before the index knew about it.
    let orphan = blake3::hash(b"never indexed");
    s.cas().put(&orphan, b"junk payload").unwrap();

    let report = s.verify(false).unwrap();
    assert_eq!(report.orphaned, vec![orphan]);
    assert!(report.is_healthy(), "an orphan wastes space but is not corruption");

    let stats = s.sweep_orphans().unwrap();
    assert_eq!(stats.chunks_removed, 1);
    assert!(s.verify(true).unwrap().orphaned.is_empty());
    assert_eq!(s.read_file("a.bin").unwrap(), data(17, 1 << 20), "the live file is untouched");
}

// -- persistence -------------------------------------------------------------

#[test]
fn a_store_reopens_with_its_contents_intact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("store");
    let key = ChunkKey::from_bytes([7u8; 32]);
    let content = data(18, 2 << 20);

    {
        let mut s = Store::open(&root, key.clone()).unwrap();
        s.put_bytes("kept.bin", &content, 0).unwrap();
    }
    {
        let s = Store::open(&root, key).unwrap();
        assert_eq!(s.read_file("kept.bin").unwrap(), content);
        assert!(s.verify(true).unwrap().is_healthy());
    }
}

#[test]
fn the_wrong_key_cannot_read_a_store() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("store");
    {
        let mut s = Store::open(&root, ChunkKey::from_bytes([1u8; 32])).unwrap();
        s.put_bytes("a.bin", &data(19, 512 << 10), 0).unwrap();
    }
    let s = Store::open(&root, ChunkKey::from_bytes([2u8; 32])).unwrap();
    assert!(s.read_file("a.bin").is_err());
}

// -- prefix queries ----------------------------------------------------------

#[test]
fn live_paths_under_resolves_a_directory_removal() {
    let (_d, mut s) = store();
    for path in ["docs/a.txt", "docs/sub/b.txt", "docs.txt", "docstring.md", "other/c.txt"] {
        s.put_bytes(path, b"content", 0).unwrap();
    }

    let under = s.db().live_paths_under("docs").unwrap();
    assert_eq!(under, vec!["docs/a.txt", "docs/sub/b.txt"]);
}

#[test]
fn live_paths_under_matches_an_exact_file() {
    let (_d, mut s) = store();
    s.put_bytes("notes.txt", b"x", 0).unwrap();
    assert_eq!(s.db().live_paths_under("notes.txt").unwrap(), vec!["notes.txt"]);
}

#[test]
fn live_paths_under_treats_wildcards_literally() {
    // A directory named "100%" must not behave as a LIKE pattern.
    let (_d, mut s) = store();
    s.put_bytes("100%/real.txt", b"x", 0).unwrap();
    s.put_bytes("100x/other.txt", b"x", 0).unwrap();
    s.put_bytes("a_b/under.txt", b"x", 0).unwrap();
    s.put_bytes("axb/decoy.txt", b"x", 0).unwrap();

    assert_eq!(s.db().live_paths_under("100%").unwrap(), vec!["100%/real.txt"]);
    assert_eq!(s.db().live_paths_under("a_b").unwrap(), vec!["a_b/under.txt"]);
}

#[test]
fn live_paths_under_excludes_deleted_files() {
    let (_d, mut s) = store();
    s.put_bytes("d/keep.txt", b"x", 0).unwrap();
    s.put_bytes("d/gone.txt", b"x", 0).unwrap();
    s.delete_file("d/gone.txt").unwrap();

    assert_eq!(s.db().live_paths_under("d").unwrap(), vec!["d/keep.txt"]);
}
