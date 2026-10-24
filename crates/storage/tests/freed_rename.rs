//! A file of a phone's own vault, its bytes freed to a computer keeping
//! them, renamed or moved on the phone all the same (decision 0073, item 3):
//! a new version at the new path pointing at the same chunks, the old path
//! deleted, and not a byte read or written.

use qurb_storage::{ChunkKey, Store};

#[test]
fn a_freed_file_of_the_vault_is_renamed_without_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("folder");
    std::fs::create_dir_all(&root).unwrap();
    let mut store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([4; 32])).unwrap().in_tree(&root);
    store.set_new_files_private(true);
    let bytes = vec![7u8; 3 << 20];
    std::fs::write(root.join("diary.txt"), &bytes).unwrap();
    store.put_file("diary.txt", &root.join("diary.txt")).unwrap();
    let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
    store.db().set_peer_kind(&computer, "computer").unwrap();
    store.db().note_replica(&blake3::hash(&bytes), &computer).unwrap();
    store.free_local("diary.txt").unwrap();
    assert!(!root.join("diary.txt").exists());

    store.rename_file("diary.txt", "Notes/2026/diary.txt").unwrap();

    let me = store.device_id().unwrap();
    let (row, scope) = store.db().folder_row("Notes/2026/diary.txt").unwrap().expect("not at the new path");
    assert_eq!(scope, Some(me), "left the vault");
    assert_eq!(row.content_hash, blake3::hash(&bytes));
    assert_eq!(store.db().is_materialised("Notes/2026/diary.txt").unwrap(), Some(false));
    assert!(!root.join("Notes/2026/diary.txt").exists(), "bytes written that were not here");
    assert!(store.db().folder_row("diary.txt").unwrap().is_none(), "still at the old path");
    let old = store.db().chunk_hashes_for(row.id).unwrap();
    assert!(!old.is_empty(), "the new version points at no chunks");
}
