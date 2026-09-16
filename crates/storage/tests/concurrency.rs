//! Two writers, one store.
//!
//! SQLite allows one writer at a time. Everything in this system has so far
//! assumed a single process with a single connection, and the garbage collector
//! takes the write lock for its deletions specifically so that a concurrent
//! writer cannot add a reference to a chunk between it being judged unreferenced
//! and being removed.
//!
//! That reasoning had never been exercised, and it turned out to be wrong. The
//! writer decides which chunks it already has *before* opening the transaction
//! that references them, so collection could remove one in the gap — the lock
//! protects the collector's own work, not a check someone else made earlier.
//!
//! SQLite caught it as a foreign-key violation rather than letting a dangling
//! reference through, so the failure was a refused write rather than corruption.
//! The write now re-establishes every chunk inside its own transaction, where
//! the lock genuinely does exclude collection.

use qurb_storage::{ChunkKey, Store};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const KEY: [u8; 32] = [88; 32];

fn open(dir: &Path) -> Store {
    Store::open(dir, ChunkKey::from_bytes(KEY)).expect("open store")
}

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

#[test]
fn a_second_connection_can_read_while_the_first_writes() {
    // WAL mode's headline property, and the thing the peer server depends on:
    // it serves from its own connection while the engine keeps writing.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("store");

    let mut writer = open(&root);
    writer.put_bytes("first.bin", &data(1, 256 << 10), 0).unwrap();

    let reader = open(&root);
    assert_eq!(reader.read_file("first.bin").unwrap(), data(1, 256 << 10));

    writer.put_bytes("second.bin", &data(2, 256 << 10), 0).unwrap();
    assert_eq!(reader.read_file("second.bin").unwrap(), data(2, 256 << 10));
}

#[test]
fn two_writers_do_not_corrupt_each_other() {
    // Both threads write through their own connection. SQLite serialises them;
    // what is being checked is that the result is coherent rather than that they
    // ran in parallel.
    let dir = tempfile::tempdir().unwrap();
    let root = Arc::new(dir.path().join("store"));
    open(&root); // create it once, so neither thread races the schema

    let handles: Vec<_> = (0..2u32)
        .map(|worker| {
            let root = Arc::clone(&root);
            std::thread::spawn(move || {
                let mut store = open(&root);
                for i in 0..40 {
                    store
                        .put_bytes(
                            &format!("w{worker}-f{i:03}.bin"),
                            &data(worker * 1000 + i, 64 << 10),
                            i as i64,
                        )
                        .expect("write under contention");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("worker panicked");
    }

    let store = open(&root);
    assert_eq!(store.db().live_paths().unwrap().len(), 80);
    assert!(store.verify(true).unwrap().is_healthy());
    assert!(store.db().audit_refcounts().unwrap().is_empty());
}

#[test]
fn collecting_while_another_connection_writes_never_removes_live_data() {
    // The invariant the collector's write lock exists for. A chunk judged
    // unreferenced and then referenced again before removal would leave the
    // index pointing at nothing.
    let dir = tempfile::tempdir().unwrap();
    let root = Arc::new(dir.path().join("store"));
    open(&root);

    let stop = Arc::new(AtomicBool::new(false));

    let writer = {
        let root = Arc::clone(&root);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut store = open(&root);
            let mut i = 0u32;
            while !stop.load(Ordering::Relaxed) {
                // Rewriting the same paths constantly releases old chunks,
                // which is exactly what gives the collector something to do.
                let path = format!("churn{}.bin", i % 8);
                // A failure here is the bug this test exists for: the writer
                // referencing a chunk that collection removed between the check
                // and the reference.
                store
                    .put_bytes(&path, &data(i, 128 << 10), i as i64)
                    .expect("write raced with collection");
                if i.is_multiple_of(5) {
                    let _ = store.delete_file(&format!("churn{}.bin", (i + 3) % 8));
                }
                i += 1;
            }
            i
        })
    };

    let collector = {
        let root = Arc::clone(&root);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut store = open(&root);
            let mut passes = 0;
            while !stop.load(Ordering::Relaxed) {
                store.gc(Duration::ZERO).expect("collect under contention");
                store.sweep_orphans().expect("sweep under contention");
                passes += 1;
                std::thread::sleep(Duration::from_millis(2));
            }
            passes
        })
    };

    std::thread::sleep(Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);

    let written = writer.join().expect("writer panicked");
    let passes = collector.join().expect("collector panicked");
    assert!(written > 10, "the writer barely ran ({written} writes)");
    assert!(passes > 5, "the collector barely ran ({passes} passes)");

    let store = open(&root);
    let report = store.verify(true).unwrap();
    assert!(
        report.missing.is_empty(),
        "collection removed {} chunk(s) that the index still references",
        report.missing.len()
    );
    assert!(report.corrupt.is_empty());
    assert!(report.refcount_drift.is_empty());

    for path in store.db().live_paths().unwrap() {
        store
            .read_file(&path)
            .unwrap_or_else(|e| panic!("{path} is indexed but unreadable after collection: {e}"));
    }
}

#[test]
fn rewriting_content_that_collection_just_removed_succeeds() {
    // The race, forced rather than waited for. Content is written, deleted, and
    // collected — so its chunks are gone from both the index and the disk — and
    // then the identical content is written again. A writer that trusted a
    // stale check would reference chunks that no longer exist.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("store");
    let mut store = open(&root);

    let content = data(7, 512 << 10);
    for round in 0..5 {
        store.put_bytes("recreated.bin", &content, round).unwrap();
        store.delete_file("recreated.bin").unwrap();
        store.gc(Duration::ZERO).unwrap();
        store.gc(Duration::ZERO).unwrap();

        // Everything is gone, including the payloads.
        assert_eq!(store.db().chunk_count().unwrap(), 0, "round {round}: collection left chunks");

        store.put_bytes("recreated.bin", &content, round + 1).unwrap();
        assert_eq!(store.read_file("recreated.bin").unwrap(), content, "round {round}");
        assert!(store.verify(true).unwrap().is_healthy(), "round {round}");
    }
}

#[test]
fn a_reader_is_not_broken_by_concurrent_collection() {
    // A peer serving chunks while the local engine collects. The reader must
    // either succeed or fail cleanly, never return wrong bytes.
    let dir = tempfile::tempdir().unwrap();
    let root = Arc::new(dir.path().join("store"));
    {
        let mut store = open(&root);
        for i in 0..20 {
            store.put_bytes(&format!("keep{i:02}.bin"), &data(i, 32 << 10), i as i64).unwrap();
        }
    }

    let stop = Arc::new(AtomicBool::new(false));
    let collector = {
        let root = Arc::clone(&root);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut store = open(&root);
            while !stop.load(Ordering::Relaxed) {
                store.gc(Duration::ZERO).expect("collect");
                store.sweep_orphans().expect("sweep");
            }
        })
    };

    let reader = open(&root);
    for _ in 0..25 {
        for i in 0..20u32 {
            let path = format!("keep{i:02}.bin");
            let got = reader.read_file(&path).expect("live file became unreadable");
            assert_eq!(got, data(i, 32 << 10), "{path} returned the wrong bytes");
        }
    }

    stop.store(true, Ordering::Relaxed);
    collector.join().expect("collector panicked");
}
