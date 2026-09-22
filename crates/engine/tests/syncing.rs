//! Engine integration tests.
//!
//! These drive `reconcile` and `apply` directly rather than going through
//! `run`, so they are synchronous and deterministic. The watcher's own tests
//! cover event delivery; what matters here is what the engine decides to do
//! with a change once it has one.

use qurb_engine::Engine;
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::{Change, IgnoreRules};
use std::fs;
use std::path::PathBuf;

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        let store_dir = root.join(".qurb");
        fs::create_dir_all(&root).unwrap();

        let store = Store::open(&store_dir, ChunkKey::generate()).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        let engine = Engine::new(&root, store, ignore);

        Self { _dir: dir, root, engine }
    }

    fn write(&self, rel: &str, content: &[u8]) -> PathBuf {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        path
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Force a distinct modification time, so tests do not depend on the
    /// filesystem's timestamp resolution to notice an edit.
    fn touch_later(&self, rel: &str) {
        let path = self.path(rel);
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        let f = fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(later).unwrap();
    }

    fn stored(&self) -> Vec<String> {
        self.engine.store().list().unwrap()
    }
}

// -- reconciliation ----------------------------------------------------------

#[test]
fn reconcile_stores_everything_it_finds() {
    let mut f = Fixture::new();
    f.write("notes.txt", b"hello");
    f.write("photos/a.jpg", b"jpeg bytes");
    f.write("deep/nested/x.bin", b"binary");

    let stats = f.engine.reconcile().unwrap();
    assert!(stats.is_clean(), "{:?}", stats.failures);
    assert_eq!(stats.stored, 3);
    assert_eq!(stats.unchanged, 0);
    assert_eq!(f.stored(), vec!["deep/nested/x.bin", "notes.txt", "photos/a.jpg"]);
}

#[test]
fn reconcile_is_idempotent_and_cheap_the_second_time() {
    // The fast path exists because reconciliation revisits every file: without
    // it, every startup would re-read the whole library.
    let mut f = Fixture::new();
    f.write("a.txt", b"content a");
    f.write("b.txt", b"content b");

    let first = f.engine.reconcile().unwrap();
    assert_eq!(first.stored, 2);

    let second = f.engine.reconcile().unwrap();
    assert_eq!(second.stored, 0, "nothing changed, so nothing should be stored");
    assert_eq!(second.unchanged, 2, "both should hit the size and mtime fast path");
    assert_eq!(second.bytes_written, 0);
}

#[test]
fn reconcile_notices_an_edit() {
    let mut f = Fixture::new();
    f.write("doc.txt", b"version one");
    f.engine.reconcile().unwrap();

    f.write("doc.txt", b"version two, which is longer");
    let stats = f.engine.reconcile().unwrap();
    assert_eq!(stats.stored, 1);
    assert_eq!(f.engine.store().read_file("doc.txt").unwrap(), b"version two, which is longer");
}

#[test]
fn reconcile_notices_an_edit_that_keeps_the_same_length() {
    // Same size, different mtime. Only the timestamp distinguishes these.
    let mut f = Fixture::new();
    f.write("doc.txt", b"aaaa");
    f.engine.reconcile().unwrap();

    f.write("doc.txt", b"bbbb");
    f.touch_later("doc.txt");

    let stats = f.engine.reconcile().unwrap();
    assert_eq!(stats.stored, 1, "an mtime change alone must trigger a re-read");
    assert_eq!(f.engine.store().read_file("doc.txt").unwrap(), b"bbbb");
}

#[test]
fn reconcile_tombstones_files_deleted_while_not_running() {
    // The case reconciliation exists for: changes that produced no event
    // because nothing was watching.
    let mut f = Fixture::new();
    f.write("keep.txt", b"keep");
    f.write("gone.txt", b"gone");
    f.engine.reconcile().unwrap();

    fs::remove_file(f.path("gone.txt")).unwrap();

    let stats = f.engine.reconcile().unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(f.stored(), vec!["keep.txt"]);
}

#[test]
fn reconcile_ignores_the_store_and_scratch_files() {
    let mut f = Fixture::new();
    f.write("real.txt", b"user content");
    f.write("draft.txt.tmp", b"scratch");
    f.write(".git/config", b"vcs metadata");

    let stats = f.engine.reconcile().unwrap();
    assert_eq!(stats.stored, 1);
    assert_eq!(f.stored(), vec!["real.txt"]);

    // Storing files wrote chunks and index rows inside the tree. A second pass
    // must not discover them as user data.
    let second = f.engine.reconcile().unwrap();
    assert_eq!(second.stored, 0, "the engine must not sync its own store");
    assert_eq!(second.deleted, 0);
}

#[test]
fn reconcile_restores_a_file_that_came_back() {
    let mut f = Fixture::new();
    f.write("flaky.txt", b"original");
    f.engine.reconcile().unwrap();

    fs::remove_file(f.path("flaky.txt")).unwrap();
    f.engine.reconcile().unwrap();
    assert!(f.stored().is_empty());

    f.write("flaky.txt", b"came back");
    let stats = f.engine.reconcile().unwrap();
    assert_eq!(stats.stored, 1);
    assert_eq!(f.engine.store().read_file("flaky.txt").unwrap(), b"came back");
}

// -- applying watcher changes ------------------------------------------------

#[test]
fn apply_stores_an_upserted_file() {
    let mut f = Fixture::new();
    let path = f.write("new.txt", b"fresh content");

    let stats = f.engine.apply(&[Change::upserted(path)]).unwrap();
    assert_eq!(stats.stored, 1);
    assert_eq!(f.engine.store().read_file("new.txt").unwrap(), b"fresh content");
}

#[test]
fn applying_the_same_change_twice_reads_the_file_once() {
    // The watcher delivers at least once, so this happens routinely.
    // See docs/decisions/0008-watcher-delivery-guarantee.md.
    let mut f = Fixture::new();
    let path = f.write("dup.txt", b"content");

    let first = f.engine.apply(&[Change::upserted(path.clone())]).unwrap();
    assert_eq!(first.stored, 1);

    let second = f.engine.apply(&[Change::upserted(path)]).unwrap();
    assert_eq!(second.stored, 0);
    assert_eq!(second.unchanged, 1, "a duplicate report must cost a stat, not a read");
    assert_eq!(second.bytes_written, 0);
}

#[test]
fn apply_tombstones_a_removed_file() {
    let mut f = Fixture::new();
    let path = f.write("doomed.txt", b"content");
    f.engine.reconcile().unwrap();
    fs::remove_file(&path).unwrap();

    let stats = f.engine.apply(&[Change::removed(path)]).unwrap();
    assert_eq!(stats.deleted, 1);
    assert!(f.stored().is_empty());
}

#[test]
fn removing_a_directory_tombstones_everything_under_it() {
    // The watcher cannot tell whether a vanished path was a file or a
    // directory. The index is what remembers what lived there.
    let mut f = Fixture::new();
    f.write("project/a.txt", b"a");
    f.write("project/sub/b.txt", b"b");
    f.write("project/sub/c.txt", b"c");
    f.write("elsewhere.txt", b"untouched");
    f.engine.reconcile().unwrap();

    let dir = f.path("project");
    fs::remove_dir_all(&dir).unwrap();

    let stats = f.engine.apply(&[Change::removed(dir)]).unwrap();
    assert_eq!(stats.deleted, 3);
    assert_eq!(f.stored(), vec!["elsewhere.txt"]);
}

#[test]
fn a_directory_removal_does_not_catch_similarly_named_siblings() {
    let mut f = Fixture::new();
    f.write("docs/inside.txt", b"in");
    f.write("docs.txt", b"sibling file");
    f.write("docstring/other.txt", b"sibling dir");
    f.engine.reconcile().unwrap();

    let dir = f.path("docs");
    fs::remove_dir_all(&dir).unwrap();

    let stats = f.engine.apply(&[Change::removed(dir)]).unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(f.stored(), vec!["docs.txt", "docstring/other.txt"]);
}

#[test]
fn an_upsert_for_a_file_that_vanished_becomes_a_removal() {
    // Delivery is at-least-once, and the file can be gone by the time the
    // change is acted on.
    let mut f = Fixture::new();
    let path = f.write("fleeting.txt", b"here for now");
    f.engine.reconcile().unwrap();
    fs::remove_file(&path).unwrap();

    let stats = f.engine.apply(&[Change::upserted(path)]).unwrap();
    assert_eq!(stats.deleted, 1);
    assert!(stats.is_clean(), "a vanished file is expected, not an error");
    assert!(f.stored().is_empty());
}

#[test]
fn removing_something_never_stored_is_not_an_error() {
    let mut f = Fixture::new();
    let stats = f.engine.apply(&[Change::removed(f.path("never-existed.txt"))]).unwrap();
    assert_eq!(stats.deleted, 0);
    assert!(stats.is_clean());
}

#[test]
fn changes_outside_the_root_are_ignored() {
    let mut f = Fixture::new();
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("not-ours.txt");
    fs::write(&path, b"someone else's file").unwrap();

    let stats = f.engine.apply(&[Change::upserted(path)]).unwrap();
    assert_eq!(stats.stored, 0);
    assert!(f.stored().is_empty());
}

// -- robustness --------------------------------------------------------------

#[test]
fn one_unreadable_file_does_not_stop_the_others() {
    // An engine that gives up on the first failure leaves everything unsynced
    // for a reason the user cannot see.
    let mut f = Fixture::new();
    f.write("before.txt", b"first");
    f.write("locked.txt", b"secret");
    f.write("after.txt", b"last");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(f.path("locked.txt"), fs::Permissions::from_mode(0o000)).unwrap();
    }

    let stats = f.engine.reconcile().unwrap();

    #[cfg(unix)]
    {
        // Running as root defeats the permission bit, so only assert the
        // failure path when it actually took effect.
        if fs::read(f.path("locked.txt")).is_err() {
            assert_eq!(stats.failures.len(), 1, "the unreadable file is recorded");
            assert_eq!(stats.stored, 2, "its neighbours still synced");
            assert!(f.stored().contains(&"before.txt".to_string()));
            assert!(f.stored().contains(&"after.txt".to_string()));
        }
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(f.path("locked.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    }

    #[cfg(not(unix))]
    assert_eq!(stats.stored, 3);
}

#[test]
fn content_survives_a_full_cycle_and_verifies() {
    let mut f = Fixture::new();
    let big: Vec<u8> = {
        let mut v = Vec::with_capacity(3 << 20);
        let mut x: u32 = 0x9E3779B1;
        for _ in 0..(3 << 20) {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            v.push(x as u8);
        }
        v
    };
    f.write("large.bin", &big);
    f.write("small.txt", b"tiny");

    f.engine.reconcile().unwrap();

    assert_eq!(f.engine.store().read_file("large.bin").unwrap(), big);
    let report = f.engine.store().verify(true).unwrap();
    assert!(report.is_healthy(), "{report:?}");
}

// -- end to end --------------------------------------------------------------

/// The only test that exercises the real loop: a live watcher feeding the
/// engine, on the multi-threaded runtime `Engine::run` requires.
#[tokio::test(flavor = "multi_thread")]
async fn the_run_loop_syncs_a_live_directory() {
    use qurb_watcher::{DebounceConfig, Watcher};
    use std::time::Duration;

    let mut f = Fixture::new();
    let root = f.root.clone();

    // A file present before the engine starts: reconciliation must find it,
    // because it produced no event.
    f.write("existing.txt", b"was here first");

    let store_dir = root.join(".qurb");
    let ignore = IgnoreRules::new().with_store_dir(&store_dir);
    let config =
        DebounceConfig { quiet: Duration::from_millis(150), max_hold: Duration::from_secs(2) };
    let watcher = Watcher::start(&root, ignore, config).unwrap();

    let writer_root = root.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;

        fs::write(writer_root.join("created.txt"), b"appeared while running").unwrap();

        fs::create_dir_all(writer_root.join("bulk")).unwrap();
        for i in 0..10 {
            fs::write(writer_root.join(format!("bulk/f{i}.txt")), format!("bulk {i}")).unwrap();
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
        fs::remove_file(writer_root.join("existing.txt")).unwrap();
    });

    // Runs until the timeout; there is no other way to end the loop, since the
    // watcher only stops when dropped.
    let _ = tokio::time::timeout(Duration::from_secs(4), f.engine.run(watcher)).await;

    let stored = f.stored();
    assert!(
        !stored.contains(&"existing.txt".to_string()),
        "a file deleted while running should be tombstoned; got {stored:?}"
    );
    assert!(stored.contains(&"created.txt".to_string()), "got {stored:?}");
    for i in 0..10 {
        assert!(stored.contains(&format!("bulk/f{i}.txt")), "missing bulk/f{i}.txt in {stored:?}");
    }
    assert_eq!(
        f.engine.store().read_file("created.txt").unwrap(),
        b"appeared while running"
    );
    let report = f.engine.store().verify(true).unwrap();
    assert!(
        report.is_healthy(),
        "missing={} corrupt={} drift={:?}",
        report.missing.len(),
        report.corrupt.len(),
        report.refcount_drift
    );
}
