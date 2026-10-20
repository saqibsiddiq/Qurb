//! The list counter (decision 0072): moved by every change a file list shows,
//! and by nothing else, so that a device can be sent only what changed.

use qurb_storage::{ChunkKey, Store};

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join(".qurb"), ChunkKey::from_bytes([3; 32])).unwrap();
    (dir, store)
}

#[test]
fn a_listed_change_moves_the_counter_and_a_touch_does_not() {
    let (_dir, mut store) = store();
    store.put_bytes("a.txt", b"one", 1).unwrap();
    store.put_bytes("b.txt", b"two", 1).unwrap();
    let before = store.db().list_mark().unwrap();

    // The same bytes again: not a change, so not listed as one.
    store.put_bytes("a.txt", b"one", 2).unwrap();
    assert_eq!(store.db().list_mark().unwrap(), before, "the same bytes moved the counter");
    assert!(store.db().listed_since(before).unwrap().is_empty());

    // New bytes are.
    store.put_bytes("a.txt", b"changed", 3).unwrap();
    let after = store.db().list_mark().unwrap();
    assert!(after > before);
    let changed = store.db().listed_since(before).unwrap();
    assert_eq!(changed.len(), 1);
    assert!(changed.contains(&(0, "a.txt".to_string())));

    // So is a deletion, and a move into this device's own vault.
    store.delete_file("b.txt").unwrap();
    let me = store.device_id().unwrap();
    store.set_scope("a.txt", Some(&me)).unwrap();
    let changed = store.db().listed_since(after).unwrap();
    assert!(changed.contains(&(0, "b.txt".to_string())), "{changed:?}");
    assert!(changed.contains(&(2, "a.txt".to_string())), "{changed:?}");

    // Nothing since now.
    assert!(store.db().listed_since(store.db().list_mark().unwrap()).unwrap().is_empty());
}
