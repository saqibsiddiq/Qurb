//! What happened, and whether it can still be answered tomorrow.
//!
//! The daemon has always known what it did and never written it down, so the
//! only account of a sync was the log of whichever process happened to be
//! running. "Why is my file not here?" is a question about the past, and a log
//! that ended at the last restart cannot answer it.

use qurb_storage::db::Event;
use qurb_storage::{ChunkKey, Store};
use qurb_sync::DeviceId;
use std::time::Duration;

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join(".qurb"), ChunkKey::from_bytes([3; 32])).unwrap();
    (dir, store)
}

#[test]
fn what_happened_comes_back_newest_first() {
    let (_dir, store) = store();
    let db = store.db();

    for path in ["first.txt", "second.txt", "third.txt"] {
        db.note(Event::Stored, path).unwrap();
    }

    let rows = db.activity(10, None).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.path.as_deref().unwrap()).collect::<Vec<_>>(),
        vec!["third.txt", "second.txt", "first.txt"]
    );
}

/// Three events in the same second are indistinguishable by time, so paging by
/// time alone would repeat or skip one at every boundary. Paging is by id.
#[test]
fn paging_neither_repeats_nor_skips_within_one_second() {
    let (_dir, store) = store();
    let db = store.db();
    for i in 0..10 {
        db.note(Event::Stored, &format!("file-{i}.txt")).unwrap();
    }

    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = db.activity(3, before).unwrap();
        if page.is_empty() {
            break;
        }
        before = Some(page.last().unwrap().id);
        seen.extend(page.into_iter().map(|r| r.path.unwrap()));
    }

    assert_eq!(seen.len(), 10, "paging lost or repeated rows: {seen:?}");
    let unique: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(unique.len(), 10);
}

#[test]
fn one_paths_history_is_answerable_on_its_own() {
    let (_dir, store) = store();
    let db = store.db();

    db.note(Event::Stored, "report.pdf").unwrap();
    db.note(Event::Stored, "other.txt").unwrap();
    db.record(
        Event::Evicted,
        Some("report.pdf"),
        Some(4096),
        None,
        Some("dropped to stay under the storage limit"),
    )
    .unwrap();

    let history = db.activity_for("report.pdf", 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].kind, Event::Evicted);
    assert_eq!(history[0].size, Some(4096));
    assert!(history[0].detail.as_deref().unwrap().contains("storage limit"));
}

#[test]
fn the_other_device_is_remembered_by_id_not_by_name() {
    let (_dir, store) = store();
    let phone = DeviceId::from_bytes([0x5A; 32]);
    store
        .db()
        .record(Event::Sent, Some("holiday.jpg"), Some(900), Some(&phone), None)
        .unwrap();

    let rows = store.db().activity(10, None).unwrap();
    assert_eq!(rows[0].device, Some(phone), "a name would change; an id does not");
}

/// Both limits, because they fail differently: age alone lets a busy week grow
/// without bound, a count alone lets a quiet device keep rows from years ago.
#[test]
fn history_is_pruned_by_age_and_by_count() {
    let (_dir, store) = store();
    let db = store.db();

    for i in 0..50 {
        db.note(Event::Stored, &format!("file-{i}.txt")).unwrap();
    }
    // Backdate half of them past any plausible window.
    db.conn()
        .execute("UPDATE activity SET at = at - 100000 WHERE id <= 25", [])
        .unwrap();

    let gone = db.prune_activity(Duration::from_secs(3600), 10_000).unwrap();
    assert_eq!(gone, 25, "the old rows should have gone and the recent ones stayed");
    assert_eq!(db.activity(100, None).unwrap().len(), 25);

    let gone = db.prune_activity(Duration::from_secs(3600), 5).unwrap();
    assert_eq!(gone, 20);
    let left = db.activity(100, None).unwrap();
    assert_eq!(left.len(), 5);
    assert_eq!(left[0].path.as_deref().unwrap(), "file-49.txt", "the newest must survive");
}

/// A build that knew a kind this one does not should not lose the row. Showing
/// one unfamiliar word is better than a gap in somebody's history.
#[test]
fn an_unknown_kind_is_kept_rather_than_dropped() {
    let (_dir, store) = store();
    store
        .db()
        .conn()
        .execute(
            "INSERT INTO activity (at, kind, path) VALUES (unixepoch(), 'teleported', 'x.txt')",
            [],
        )
        .unwrap();

    let rows = store.db().activity(10, None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, Event::Other("teleported".into()));
    assert_eq!(rows[0].kind.as_str(), "teleported");
}
