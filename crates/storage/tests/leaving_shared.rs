//! A phone leaving the shared area (decision 0073, item 2), as the owner
//! chose: what a computer keeps is let go of on the phone without a word to
//! anyone -- no tombstone, so nothing is deleted anywhere else -- and what
//! only the phone has moves into its own vault, so nothing is lost. Bytes on
//! the phone go only once the computer, asked, has said it holds them.

use qurb_storage::{ChunkKey, LeftShared, Store};
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
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([3; 32])).unwrap().in_tree(&root);
        store.db().set_local_kind("phone").unwrap();
        Self { _dir: dir, root, store }
    }

    fn add(&mut self, path: &str, bytes: &[u8]) -> blake3::Hash {
        let on_disk = self.root.join(path);
        std::fs::create_dir_all(on_disk.parent().unwrap()).unwrap();
        std::fs::write(&on_disk, bytes).unwrap();
        self.store.put_file(path, &on_disk).unwrap();
        blake3::hash(bytes)
    }
}

#[test]
fn what_a_computer_keeps_goes_and_the_rest_moves_into_the_vault() {
    let mut phone = Phone::new();
    let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
    phone.store.db().set_peer_kind(&computer, "computer").unwrap();
    let other_phone = qurb_sync::DeviceId::from_bytes([9; 32]);
    phone.store.db().set_peer_kind(&other_phone, "phone").unwrap();

    let kept = phone.add("Photos/kept-on-the-laptop.jpg", b"the laptop has this");
    phone.store.db().note_replica(&kept, &computer).unwrap();
    phone.store.db().note_held(&computer, &kept).unwrap();
    let only_phones = phone.add("only-on-phones.txt", b"only another phone has this");
    phone.store.db().note_replica(&only_phones, &other_phone).unwrap();
    phone.add("only-here.txt", b"nobody else has this");
    phone.add("gone.txt", b"deleted before");
    phone.store.delete_file("gone.txt").unwrap();

    let left = phone.store.leave_shared_area().unwrap();
    assert_eq!(left, LeftShared { dropped: 1, moved: 2, kept: 0 });

    // Let go of: no row, no file, no tombstone for anyone to take.
    assert!(phone.store.db().shared_rows().unwrap().is_empty(), "the shared area is still here");
    assert!(!phone.root.join("Photos/kept-on-the-laptop.jpg").exists());
    assert!(!phone.root.join("Photos").exists(), "the folder it emptied left behind");
    assert!(phone.store.shared_tree().unwrap().is_empty(), "something of it offered to the computers");

    // Moved: still here, in the phone's own vault -- a phone's copy is not a
    // safe last one (decision 0053), so a file another phone has moves too.
    let me = phone.store.device_id().unwrap();
    for path in ["only-here.txt", "only-on-phones.txt"] {
        assert!(matches!(phone.store.db().folder_row(path).unwrap(), Some((_, Some(owner))) if owner == me), "{path} not in the vault");
        assert!(phone.root.join(path).is_file(), "{path} lost");
    }

    // And again: nothing left to do.
    assert_eq!(phone.store.leave_shared_area().unwrap(), LeftShared::default());
}

/// A computer recorded as keeping a file whose bytes are on the phone, not
/// confirmed: the record may be stale -- the owner's laptop freed its shared
/// files before decision 0053 on the strength of the S23's copies -- so the
/// phone keeps the file until the computer, asked, answers. Asked at the next
/// sync; a yes lets it go, a no moves it into the vault. A row whose bytes
/// are not here goes on the record alone: forgetting it loses nothing.
#[test]
fn bytes_here_go_only_on_the_computers_word() {
    let mut phone = Phone::new();
    let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
    phone.store.db().set_peer_kind(&computer, "computer").unwrap();
    let held = phone.add("held.jpg", b"the laptop still has this");
    let freed_there = phone.add("freed-there.jpg", b"the laptop freed this long ago");
    for content in [&held, &freed_there] {
        phone.store.db().note_replica(content, &computer).unwrap();
    }
    let listed = phone.add("listed-only.txt", b"never on this phone");
    phone.store.db().note_replica(&listed, &computer).unwrap();
    phone.store.db().set_materialised("listed-only.txt", false).unwrap();
    std::fs::remove_file(phone.root.join("listed-only.txt")).unwrap();

    let left = phone.store.leave_shared_area().unwrap();
    assert_eq!(left, LeftShared { dropped: 1, moved: 0, kept: 2 });
    assert!(phone.root.join("held.jpg").is_file() && phone.root.join("freed-there.jpg").is_file());

    // What the next sync asks the computer.
    let asked = phone.store.db().unconfirmed_holdings(&computer, 16).unwrap();
    assert!(asked.contains(&held) && asked.contains(&freed_there), "{asked:?}");
    phone.store.db().note_held(&computer, &held).unwrap();
    phone.store.db().note_not_held(&computer, &freed_there).unwrap();

    let left = phone.store.leave_shared_area().unwrap();
    assert_eq!(left, LeftShared { dropped: 1, moved: 1, kept: 0 });
    assert!(!phone.root.join("held.jpg").exists());
    let me = phone.store.device_id().unwrap();
    assert!(matches!(phone.store.db().folder_row("freed-there.jpg").unwrap(), Some((_, Some(owner))) if owner == me));
    assert!(phone.root.join("freed-there.jpg").is_file(), "the last copy lost");
}
