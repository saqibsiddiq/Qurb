//! What a backup still has to give the computer keeping it, named file by
//! file, and which file a piece being collected belongs to: what Home and the
//! notice say while a phone backs up (the owner's list of 2026-10-25: "it
//! only says folder instead it should be informing exactly what is backing
//! up").

use qurb_storage::{ChunkKey, OutsideEntry, Store};

fn phone() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("folder");
    std::fs::create_dir_all(&root).unwrap();
    let mut store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([7; 32])).unwrap().in_tree(&root);
    store.set_new_files_private(true);
    let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
    store.db().set_peer_kind(&computer, "computer").unwrap();
    (dir, store)
}

/// A folder kept where it lies, as `back_up_chosen` keeps one.
fn keep(dir: &tempfile::TempDir, store: &mut Store, prefix: &str, files: &[(&str, &[u8])]) {
    let entries: Vec<OutsideEntry> = files
        .iter()
        .map(|(name, bytes)| {
            let source = dir.path().join("storage").join(prefix.replace('/', "_")).join(name);
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(&source, bytes).unwrap();
            OutsideEntry { path: name.to_string(), source: source.to_string_lossy().into_owned(), size: bytes.len() as u64, mtime_ns: 1 }
        })
        .collect();
    store.keep_outside_folder(prefix, &entries, &|_| true, None, &|_| false).unwrap();
}

#[test]
fn what_is_waiting_is_named_and_what_the_computer_has_is_not() {
    let (dir, mut store) = phone();
    keep(&dir, &mut store, ".qurb-outside/storage/DCIM/Trip", &[("VID_01.mp4", b"first video"), ("VID_02.mp4", b"second, longer video")]);
    keep(&dir, &mut store, ".qurb-outside/whatsapp", &[("Media/IMG-1.jpg", b"a photo")]);

    let (first, files, bytes) = store.db().outside_waiting(".qurb-outside/storage", 1).unwrap();
    assert_eq!((files, bytes), (2, 31));
    assert_eq!(first, vec![(".qurb-outside/storage/DCIM/Trip/VID_01.mp4".to_string(), 11)]);

    // The computer has the first: no longer waiting.
    let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
    store.db().note_replica(&blake3::hash(b"first video"), &computer).unwrap();
    let (first, files, bytes) = store.db().outside_waiting(".qurb-outside/storage", 10).unwrap();
    assert_eq!((files, bytes), (1, 20));
    assert_eq!(first[0].0, ".qurb-outside/storage/DCIM/Trip/VID_02.mp4");
    // WhatsApp's are counted apart.
    assert_eq!(store.db().outside_waiting(".qurb-outside/whatsapp", 10).unwrap().1, 1);
}

#[test]
fn a_piece_being_collected_names_its_file() {
    let (dir, mut store) = phone();
    keep(&dir, &mut store, ".qurb-outside/storage/DCIM/Trip", &[("VID_07.mp4", b"the one going now")]);
    let me = store.device_id().unwrap();
    let row = store.db().live_row_in(".qurb-outside/storage/DCIM/Trip/VID_07.mp4", Some(&me)).unwrap().unwrap();
    let chunk = store.db().chunk_hashes_for(row.id).unwrap()[0];
    assert_eq!(
        store.db().own_file_with_chunk(&chunk).unwrap().as_deref(),
        Some(".qurb-outside/storage/DCIM/Trip/VID_07.mp4")
    );
    assert_eq!(store.db().own_file_with_chunk(&blake3::hash(b"nothing of ours")).unwrap(), None);
}
