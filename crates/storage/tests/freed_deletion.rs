//! A file of a phone's own vault deleted while its bytes are not on the phone
//! -- freed to the computer keeping them, or a backup read where it lay --
//! kept in Recently deleted all the same, and put back from the chunks the
//! deleted row still lists (decision 0073). Before this, such a deletion kept
//! nothing to restore, and the computer let go of the bytes within two weeks.

use qurb_storage::{ChunkKey, OutsideEntry, Store};
use std::time::Duration;

const DAY: i64 = 24 * 3600;

struct Phone {
    dir: tempfile::TempDir,
    root: std::path::PathBuf,
    store: Store,
}

impl Phone {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("folder");
        std::fs::create_dir_all(&root).unwrap();
        let mut store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([6; 32])).unwrap().in_tree(&root);
        store.set_new_files_private(true);
        let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
        store.db().set_peer_kind(&computer, "computer").unwrap();
        Self { dir, root, store }
    }

    /// A file of the vault, kept by the computer and freed here.
    fn freed(&mut self, path: &str, bytes: &[u8]) {
        std::fs::create_dir_all(self.root.join(path).parent().unwrap()).unwrap();
        std::fs::write(self.root.join(path), bytes).unwrap();
        self.store.put_file(path, &self.root.join(path)).unwrap();
        let computer = qurb_sync::DeviceId::from_bytes([8; 32]);
        self.store.db().note_replica(&blake3::hash(bytes), &computer).unwrap();
        self.store.free_local(path).unwrap();
        assert!(!self.root.join(path).exists());
    }

    /// Move a deletion `days` into the past.
    fn age(&self, path: &str, days: i64) {
        self.store
            .db()
            .conn()
            .execute(
                "UPDATE files SET deleted_at = deleted_at - ?1 WHERE path = ?2",
                rusqlite::params![days * DAY, path],
            )
            .unwrap();
    }
}

#[test]
fn a_freed_file_deleted_is_kept_and_put_back_on_its_chunks() {
    let mut phone = Phone::new();
    let bytes = vec![9u8; 3 << 20];
    phone.freed("Notes/diary.txt", &bytes);

    let id = phone.store.delete_to_trash("Notes/diary.txt", None).unwrap().expect("nothing kept");
    let listed = phone.store.recently_deleted().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, "Notes/diary.txt");
    assert_eq!(listed[0].size, 0, "counted as costing space here");
    assert_eq!(phone.store.db().trash_bytes().unwrap(), 0);
    // Nothing to free when space is short: it is not here.
    assert_eq!(phone.store.empty_trash_by(1).unwrap(), 0);
    assert_eq!(phone.store.recently_deleted().unwrap().len(), 1, "let go of to free space it does not use");

    // Past the week everything else deleted is kept for: still listed with
    // its chunks, on the phone as on the computer keeping it.
    phone.age("Notes/diary.txt", 10);
    phone.store.gc(Duration::from_secs(7 * DAY as u64)).unwrap();

    let at = phone.store.restore_from_trash(id).unwrap();
    assert_eq!(at, "Notes/diary.txt");
    let me = phone.store.device_id().unwrap();
    let (row, scope) = phone.store.db().folder_row(&at).unwrap().expect("not back");
    assert_eq!(scope, Some(me), "not back in the vault");
    assert_eq!(row.content_hash, blake3::hash(&bytes));
    assert_eq!(phone.store.db().is_materialised(&at).unwrap(), Some(false));
    assert!(!phone.root.join(&at).exists(), "bytes written that were not here");
    assert!(phone.store.recently_deleted().unwrap().is_empty());
}

#[test]
fn a_vault_deletion_goes_with_recently_deleted_and_not_before() {
    let mut phone = Phone::new();
    phone.freed("old.txt", b"deleted long ago");
    phone.freed("recent.txt", b"deleted three weeks ago");
    phone.store.delete_to_trash("old.txt", None).unwrap();
    phone.store.delete_to_trash("recent.txt", None).unwrap();
    phone.age("old.txt", 31);
    phone.age("recent.txt", 21);

    let stats = phone.store.gc(Duration::from_secs(7 * DAY as u64)).unwrap();
    assert_eq!(stats.tombstones_expired, 1);
    let me = phone.store.device_id().unwrap();
    let old = blake3::hash(b"deleted long ago");
    let recent = blake3::hash(b"deleted three weeks ago");
    assert!(phone.store.db().deleted_row_in("old.txt", &me, &old).unwrap().is_none());
    assert!(phone.store.db().deleted_row_in("recent.txt", &me, &recent).unwrap().is_some());
}

#[test]
fn a_backup_deleted_comes_back_into_the_vault_by_its_name() {
    let mut phone = Phone::new();
    let outside = phone.dir.path().join("whatsapp/Media/IMG-1.jpg");
    std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
    std::fs::write(&outside, b"a photo, backed up").unwrap();
    let entry = OutsideEntry {
        path: "Media/IMG-1.jpg".into(),
        source: outside.to_string_lossy().into_owned(),
        size: 18,
        mtime_ns: 1,
    };
    phone.store.keep_outside_folder(".qurb-outside/whatsapp", &[entry], &|_| true, None, &|_| false).unwrap();

    let id = phone
        .store
        .delete_to_trash(".qurb-outside/whatsapp/Media/IMG-1.jpg", None)
        .unwrap()
        .expect("nothing kept");
    assert_eq!(phone.store.restore_from_trash(id).unwrap(), "IMG-1.jpg");
    let me = phone.store.device_id().unwrap();
    assert!(matches!(phone.store.db().folder_row("IMG-1.jpg").unwrap(), Some((_, Some(owner))) if owner == me));
}
