//! What a crash leaves behind.
//!
//! A real child process writes into a store and is killed with SIGKILL at an
//! unpredictable point — no unwinding, no destructors, no flush of anything not
//! already fsynced. The store is then opened and checked.
//!
//! # The invariant being defended
//!
//! **A chunk referenced by the index always exists on disk.** Writes go
//! payload-first, so a crash can leave a chunk nothing points at — wasted space,
//! reclaimed later. The opposite order would leave the index pointing at a
//! payload that was never written, which no amount of local repair can fix.
//!
//! Orphaned data is a cost. A dangling reference is a corruption. These tests
//! exist to prove the system only ever produces the first.

use qurb_storage::{ChunkKey, Store};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn writer_binary() -> std::path::PathBuf {
    // The examples are built alongside the test binary.
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop(); // deps/
    path.pop(); // debug/
    path.push("examples");
    path.push("crash_writer");
    path
}

/// Run the writer against `dir` and kill it after `after`.
///
/// Returns how many files it reported finishing, which is a lower bound on what
/// must survive: the process announces a file only once the write has returned.
fn write_then_kill(dir: &Path, after: Duration) -> usize {
    let binary = writer_binary();
    assert!(
        binary.exists(),
        "crash_writer not built; run `cargo build --examples -p qurb-storage` first ({})",
        binary.display()
    );

    let mut child = Command::new(&binary)
        .arg(dir)
        .arg("100000")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn crash_writer");

    std::thread::sleep(after);
    child.kill().expect("SIGKILL the writer");

    let output = child.wait_with_output().expect("collect writer output");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .and_then(|l| l.trim().parse::<usize>().ok())
        .map(|last| last + 1)
        .unwrap_or(0)
}

fn open(dir: &Path) -> Store {
    Store::open(dir, ChunkKey::from_bytes([77; 32])).expect("reopen after crash")
}

/// Everything that must be true of a store after a crash.
fn assert_intact(store: &Store, confirmed: usize) {
    let report = store.verify(true).expect("verify");

    assert!(
        report.missing.is_empty(),
        "{} chunk(s) referenced by the index are missing from disk -- \
         this is the corruption the write ordering exists to prevent",
        report.missing.len()
    );
    assert!(report.corrupt.is_empty(), "{} chunk(s) failed to decrypt or verify", report.corrupt.len());
    assert!(
        report.refcount_drift.is_empty(),
        "{} reference count(s) disagree with the links they describe",
        report.refcount_drift.len()
    );

    // Every file the index still calls live must actually read back.
    let live = store.db().live_paths().expect("live paths");
    for path in &live {
        store
            .read_file(path)
            .unwrap_or_else(|e| panic!("{path} is indexed but unreadable after a crash: {e}"));
    }

    // Everything the writer confirmed must have survived. SQLite in WAL mode
    // with synchronous=NORMAL can lose recent commits to *power* loss, but a
    // process kill leaves the data with the kernel, which still writes it.
    assert!(
        live.len() >= confirmed,
        "the writer confirmed {confirmed} files but only {} survived -- committed data was lost",
        live.len()
    );
}

#[test]
fn a_crash_never_leaves_a_dangling_reference() {
    let dir = tempfile::tempdir().unwrap();
    let store_dir = dir.path().join("store");

    let confirmed = write_then_kill(&store_dir, Duration::from_millis(400));
    assert!(confirmed > 0, "the writer did not finish any files; test proves nothing");

    let store = open(&store_dir);
    assert_intact(&store, confirmed);
}

#[test]
fn crashes_at_many_different_moments_all_leave_a_usable_store() {
    // One kill point exercises one window. Varying it walks the kill across
    // chunk writes, index commits, and the gaps between them.
    for millis in [80, 150, 230, 310, 420, 550, 700] {
        let dir = tempfile::tempdir().unwrap();
        let store_dir = dir.path().join("store");

        let confirmed = write_then_kill(&store_dir, Duration::from_millis(millis));
        let store = open(&store_dir);
        assert_intact(&store, confirmed);
    }
}

#[test]
fn a_crash_may_orphan_chunks_but_they_are_reclaimable() {
    // The acceptable cost of payload-first writing: a chunk written but never
    // linked. It wastes space until swept, and sweeping must not touch anything
    // live.
    let dir = tempfile::tempdir().unwrap();
    let store_dir = dir.path().join("store");

    let confirmed = write_then_kill(&store_dir, Duration::from_millis(350));
    let mut store = open(&store_dir);

    let before = store.db().live_paths().unwrap();
    let swept = store.sweep_orphans().expect("sweep");

    assert_eq!(store.db().live_paths().unwrap(), before, "sweeping removed live files");
    assert_intact(&store, confirmed);
    // Orphans are expected but not guaranteed -- the kill may have landed
    // between files rather than mid-write.
    let _ = swept;
}

#[test]
fn writing_can_continue_after_a_crash() {
    // Recovery is not just "the old data is readable": the store has to be
    // usable again, including its reference counting and its clock.
    let dir = tempfile::tempdir().unwrap();
    let store_dir = dir.path().join("store");

    let confirmed = write_then_kill(&store_dir, Duration::from_millis(350));

    let mut store = open(&store_dir);
    store.put_bytes("after-the-crash.txt", b"written by the survivor", 1).unwrap();
    assert_eq!(store.read_file("after-the-crash.txt").unwrap(), b"written by the survivor");

    assert_intact(&store, confirmed);
    assert!(store.db().live_paths().unwrap().contains(&"after-the-crash.txt".to_string()));
}

#[test]
fn a_second_crash_on_top_of_a_first_is_still_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let store_dir = dir.path().join("store");

    write_then_kill(&store_dir, Duration::from_millis(250));
    let confirmed = write_then_kill(&store_dir, Duration::from_millis(400));

    let store = open(&store_dir);
    assert_intact(&store, confirmed);
}
