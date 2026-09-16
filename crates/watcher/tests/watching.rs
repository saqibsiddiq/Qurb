//! Integration tests against a real filesystem and real platform events.
//!
//! These are slower and less precise than the unit tests, because they depend
//! on the kernel actually delivering events. The debouncer's logic is tested
//! deterministically in its own module with a logical clock; what these check
//! is that the wiring is right end to end.

use qurb_watcher::{Change, DebounceConfig, Event, IgnoreRules, Watcher};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Short windows keep the tests quick. Production defaults are much longer.
fn fast() -> DebounceConfig {
    DebounceConfig { quiet: Duration::from_millis(150), max_hold: Duration::from_secs(2) }
}

/// Collect changes until the watcher goes quiet for `idle`.
///
/// Waiting for silence rather than for an expected count means a test fails
/// with "we also got these extra events" instead of passing while noise goes
/// unnoticed.
async fn drain(w: &mut Watcher, idle: Duration) -> Vec<Change> {
    let mut out = Vec::new();
    loop {
        match tokio::time::timeout(idle, w.next()).await {
            Ok(Some(Event::Changes(mut c))) => out.append(&mut c),
            Ok(Some(Event::RescanRequired)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn names(changes: &[Change], root: &Path) -> Vec<String> {
    changes
        .iter()
        .map(|c| {
            let rel = qurb_watcher::logical_path(root, &c.path)
                .unwrap_or_else(|| c.path.display().to_string());
            match c.kind {
                qurb_watcher::ChangeKind::Upserted => format!("+{rel}"),
                qurb_watcher::ChangeKind::Removed => format!("-{rel}"),
            }
        })
        .collect()
}

#[tokio::test]
async fn a_new_file_is_reported_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();

    fs::write(dir.path().join("hello.txt"), b"hi").unwrap();

    let changes = drain(&mut w, Duration::from_millis(800)).await;
    assert_eq!(names(&changes, dir.path()), vec!["+hello.txt"]);
}

#[tokio::test]
async fn a_write_burst_collapses_to_one_change() {
    // Roughly what an editor's save looks like: repeated writes in quick
    // succession. One change should come out, not six.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.txt");
    fs::write(&path, b"v0").unwrap();

    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    for i in 0..6 {
        fs::write(&path, format!("version {i}")).unwrap();
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    let changes = drain(&mut w, Duration::from_millis(800)).await;
    assert_eq!(names(&changes, dir.path()), vec!["+doc.txt"]);
}

#[tokio::test]
async fn deletion_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gone.txt");
    fs::write(&path, b"here").unwrap();

    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    fs::remove_file(&path).unwrap();

    let changes = drain(&mut w, Duration::from_millis(800)).await;
    assert_eq!(names(&changes, dir.path()), vec!["-gone.txt"]);
}

#[tokio::test]
async fn a_rename_is_a_removal_and_an_addition() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("before.txt"), b"content").unwrap();

    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    fs::rename(dir.path().join("before.txt"), dir.path().join("after.txt")).unwrap();

    let changes = drain(&mut w, Duration::from_millis(800)).await;
    assert_eq!(names(&changes, dir.path()), vec!["+after.txt", "-before.txt"]);
}

#[tokio::test]
async fn the_store_directory_is_not_watched() {
    // The most important rule here: our own writes must not generate events,
    // or storing a file would trigger storing it again.
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join(".qurb");
    fs::create_dir_all(store.join("chunks/9f")).unwrap();

    let ignore = IgnoreRules::new().with_store_dir(&store);
    let mut w = Watcher::start(dir.path(), ignore, fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    fs::write(store.join("chunks/9f/deadbeef"), b"a chunk we just wrote").unwrap();
    fs::write(store.join("index.db"), b"index write").unwrap();
    fs::write(dir.path().join("real.txt"), b"a user file").unwrap();

    let changes = drain(&mut w, Duration::from_millis(800)).await;
    assert_eq!(names(&changes, dir.path()), vec!["+real.txt"]);
}

#[tokio::test]
async fn scratch_files_are_not_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    fs::write(dir.path().join("doc.txt.tmp"), b"partial").unwrap();
    fs::write(dir.path().join(".#doc.txt"), b"lock").unwrap();
    fs::write(dir.path().join("doc.txt"), b"real content").unwrap();

    let changes = drain(&mut w, Duration::from_millis(800)).await;
    assert_eq!(names(&changes, dir.path()), vec!["+doc.txt"]);
}

#[tokio::test]
async fn a_save_through_a_temp_file_reports_only_the_final_path() {
    // The write-temp-then-rename pattern used by most editors and by our own
    // chunk store. Only the destination should surface.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("doc.txt"), b"v1").unwrap();

    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let tmp = dir.path().join("doc.txt.tmp");
    fs::write(&tmp, b"v2 contents").unwrap();
    fs::rename(&tmp, dir.path().join("doc.txt")).unwrap();

    let changes = drain(&mut w, Duration::from_millis(800)).await;
    assert_eq!(names(&changes, dir.path()), vec!["+doc.txt"]);
}

#[tokio::test]
async fn nested_files_are_reported_with_their_relative_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    fs::create_dir_all(dir.path().join("a/b")).unwrap();
    fs::write(dir.path().join("a/b/deep.txt"), b"nested").unwrap();

    let changes = drain(&mut w, Duration::from_millis(900)).await;
    assert!(
        names(&changes, dir.path()).contains(&"+a/b/deep.txt".to_string()),
        "got {:?}",
        names(&changes, dir.path())
    );
}

#[tokio::test]
async fn a_file_still_being_written_is_held_back() {
    // The stability check: while a file keeps growing it must not be released,
    // because reading it would store a torn copy.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("growing.bin");
    fs::write(&path, b"start").unwrap();

    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Append for well past the quiet period.
    for _ in 0..8 {
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut f, &[b'x'; 4096]).unwrap();
        drop(f);
        tokio::time::sleep(Duration::from_millis(60)).await;
    }

    let changes = drain(&mut w, Duration::from_millis(900)).await;
    assert_eq!(names(&changes, dir.path()), vec!["+growing.bin"], "exactly one settled change");

    // And what settled is the finished file, not an intermediate size.
    assert_eq!(fs::metadata(&path).unwrap().len(), 5 + 8 * 4096);
}

#[tokio::test]
async fn many_files_at_once_are_all_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    for i in 0..64 {
        fs::write(dir.path().join(format!("f{i:03}.txt")), format!("file {i}")).unwrap();
    }

    let changes = drain(&mut w, Duration::from_millis(1200)).await;
    let upserts = changes
        .iter()
        .filter(|c| c.kind == qurb_watcher::ChangeKind::Upserted)
        .count();
    assert_eq!(upserts, 64, "got {:?}", names(&changes, dir.path()));
}

#[tokio::test]
async fn files_written_into_a_brand_new_directory_are_not_lost() {
    // Regression test. Recursive watching installs a platform watch for a new
    // subdirectory only after that directory exists, so anything written in
    // the gap produces no event whatsoever. Unpacking an archive or cloning a
    // repository into the synced folder does exactly this, and the failure is
    // silent -- files simply never sync.
    //
    // The watcher closes the gap by walking any directory it is told about.
    let dir = tempfile::tempdir().unwrap();
    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Build the whole tree in one go, with no pauses for watches to catch up.
    for sub in ["one", "two", "three"] {
        let d = dir.path().join("project").join(sub);
        fs::create_dir_all(&d).unwrap();
        for i in 0..10 {
            fs::write(d.join(format!("f{i}.txt")), format!("{sub}/{i}")).unwrap();
        }
    }

    let changes = drain(&mut w, Duration::from_millis(1500)).await;
    let found = names(&changes, dir.path());

    // At-least-once, not exactly-once: a file can surface both from the
    // directory walk and from its own event arriving afterwards. See the
    // delivery guarantee documented on `Watcher`.
    for sub in ["one", "two", "three"] {
        for i in 0..10 {
            let want = format!("+project/{sub}/f{i}.txt");
            assert!(found.contains(&want), "missing {want} in {found:?}");
        }
    }
    assert!(
        found.iter().all(|n| n.starts_with('+')),
        "nothing was deleted, so nothing should be reported as removed: {found:?}"
    );
}

#[tokio::test]
async fn a_deleted_directory_is_reported_as_a_removal() {
    // The watcher cannot tell after the fact whether a vanished path was a file
    // or a directory, so it reports the path and leaves the index to work out
    // which files that covered.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("doomed")).unwrap();
    fs::write(dir.path().join("doomed/a.txt"), b"a").unwrap();
    fs::write(dir.path().join("doomed/b.txt"), b"b").unwrap();

    let mut w = Watcher::start(dir.path(), IgnoreRules::new(), fast()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    fs::remove_dir_all(dir.path().join("doomed")).unwrap();

    let changes = drain(&mut w, Duration::from_millis(900)).await;
    let found = names(&changes, dir.path());
    assert!(
        found.iter().all(|n| n.starts_with('-')),
        "nothing should be reported as added; got {found:?}"
    );
    assert!(found.contains(&"-doomed".to_string()), "got {found:?}");
}
