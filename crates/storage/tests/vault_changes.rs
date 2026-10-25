//! A phone changing its own folder on a computer as one changes files on a
//! computer (the owner's list of 2026-10-26, item 13): copying a file or a
//! whole folder, pasting where its name is taken, moving and renaming a
//! folder -- each a change of the phone's own vault, which the computer
//! follows -- whether the bytes are on the phone or freed to the computer.

use qurb_storage::{ChunkKey, Store};
use std::path::PathBuf;

struct Phone {
    _dir: tempfile::TempDir,
    root: PathBuf,
    store: Store,
}

impl Phone {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("folder");
        std::fs::create_dir_all(&root).unwrap();
        let mut store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([5; 32])).unwrap().in_tree(&root);
        store.set_new_files_private(true);
        let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
        store.db().set_peer_kind(&computer, "computer").unwrap();
        Self { _dir: dir, root, store }
    }

    fn add(&mut self, path: &str, bytes: &[u8]) {
        let on_disk = self.root.join(path);
        std::fs::create_dir_all(on_disk.parent().unwrap()).unwrap();
        std::fs::write(&on_disk, bytes).unwrap();
        self.store.put_file(path, &on_disk).unwrap();
    }

    /// Kept by the computer, its bytes not here.
    fn free(&mut self, path: &str, bytes: &[u8]) {
        let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
        self.store.db().note_replica(&blake3::hash(bytes), &computer).unwrap();
        self.store.free_local(path).unwrap();
    }

    fn files(&self) -> Vec<String> {
        self.store.db().own_vault_paths_under("").unwrap()
    }
}

#[test]
fn a_folder_is_copied_its_freed_files_without_a_byte() {
    let mut phone = Phone::new();
    phone.add("Notes/a.txt", b"on the phone");
    phone.add("Notes/deep/b.txt", b"freed to the computer");
    phone.free("Notes/deep/b.txt", b"freed to the computer");

    assert_eq!(phone.store.copy_in_vault("Notes", "Archive/Notes").unwrap(), "Archive/Notes");
    assert_eq!(
        phone.files(),
        vec!["Archive/Notes/a.txt", "Archive/Notes/deep/b.txt", "Notes/a.txt", "Notes/deep/b.txt"]
    );
    assert_eq!(std::fs::read(phone.root.join("Archive/Notes/a.txt")).unwrap(), b"on the phone");
    assert!(!phone.root.join("Archive/Notes/deep/b.txt").exists(), "a freed file's bytes written here");
    let me = phone.store.device_id().unwrap();
    let copy = phone.store.db().live_row_in("Archive/Notes/deep/b.txt", Some(&me)).unwrap().unwrap();
    assert_eq!(copy.content_hash, blake3::hash(b"freed to the computer"));

    // Pasted again where it is: numbered.
    assert_eq!(phone.store.copy_in_vault("Notes/a.txt", "Notes/a.txt").unwrap(), "Notes/a (2).txt");
    assert_eq!(std::fs::read(phone.root.join("Notes/a (2).txt")).unwrap(), b"on the phone");
}

#[test]
fn a_folder_is_moved_and_renamed_whole() {
    let mut phone = Phone::new();
    phone.add("Trips/2025/sea.jpg", b"the sea");
    phone.add("Trips/2025/hills.jpg", b"the hills");
    phone.free("Trips/2025/hills.jpg", b"the hills");
    phone.add("Trips2/keep.txt", b"a folder whose name only starts the same");

    assert_eq!(phone.store.move_in_vault("Trips/2025", "Holidays/2025").unwrap(), 2);
    assert_eq!(phone.files(), vec!["Holidays/2025/hills.jpg", "Holidays/2025/sea.jpg", "Trips2/keep.txt"]);

    // Into itself, or onto something there: refused, nothing moved.
    assert!(phone.store.move_in_vault("Holidays", "Holidays/inside").is_err());
    assert!(phone.store.move_in_vault("Holidays/2025/sea.jpg", "Trips2/keep.txt").is_err());
    assert_eq!(phone.files().len(), 3);
}
