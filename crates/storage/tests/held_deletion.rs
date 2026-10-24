//! A phone's Private Vault file deleted on the phone, in the Recently deleted
//! of the computer keeping it as well, and undone there (decision 0073, item
//! 3): a mistake, or a lost phone in the wrong hands, put right at the
//! computer. The computer puts the file back in the phone's folder there; the
//! phone takes it back, and only a file it deleted, exactly as it was.

use qurb_storage::db::Relation;
use qurb_storage::{ChunkKey, Store};
use qurb_sync::{Area, Causality, Content, DeviceId, FileVersion, VersionVector};
use std::time::Duration;

struct Device {
    dir: tempfile::TempDir,
    store: Store,
}

impl Device {
    fn new(key: u8) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("folder");
        std::fs::create_dir_all(&root).unwrap();
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([key; 32])).unwrap().in_tree(&root);
        Self { dir, store }
    }

    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("folder")
    }

    fn id(&self) -> DeviceId {
        self.store.device_id().unwrap()
    }
}

/// A computer keeping `phone`'s vault, the phone paired as one of its own.
fn computer_of(phone: &DeviceId) -> Device {
    let computer = Device::new(21);
    computer.store.db().trust_peer(phone, &[3; 32], "the phone").unwrap();
    computer
}

/// The phone's file as the computer is told about it, at `count` of the
/// phone's changes.
fn phones_file(phone: &DeviceId, path: &str, bytes: &[u8], count: u64) -> FileVersion {
    let mut vector = VersionVector::new();
    for _ in 0..count {
        vector.increment(*phone);
    }
    FileVersion {
        path: path.into(),
        content: Content::File { hash: *blake3::hash(bytes).as_bytes(), size: bytes.len() as u64 },
        vector,
        modified_by: *phone,
        // Now: a deletion's row is dated by the change, and one a month old
        // is past keeping.
        modified_at: now(),
        area: Area::Hold,
    }
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

/// Its deletion, the phone's next change.
fn phones_deletion(of: &FileVersion) -> FileVersion {
    let mut vector = of.vector.clone();
    vector.increment(of.modified_by);
    FileVersion { content: Content::Deleted, vector, ..of.clone() }
}

fn hold(computer: &mut Device, phone: &DeviceId, version: &FileVersion, bytes: &[u8]) {
    let staged = computer.dir.path().join("staged");
    std::fs::write(&staged, bytes).unwrap();
    computer.store.hold_file(version, phone, &staged).unwrap();
    std::fs::remove_file(&staged).unwrap();
}

#[test]
fn a_phones_deletion_is_listed_and_put_back_at_the_computer() {
    let phone = DeviceId::from_bytes([1; 32]);
    let mut computer = computer_of(&phone);
    let bytes = vec![5u8; 300_000];
    let kept = phones_file(&phone, "Notes/diary.txt", &bytes, 1);
    hold(&mut computer, &phone, &kept, &bytes);

    let deletion = phones_deletion(&kept);
    computer.store.unhold(&deletion, &phone).unwrap();
    let listed = computer.store.recently_deleted().unwrap();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].path, "Notes/diary.txt");
    assert_eq!(listed[0].scope, Some(phone));
    assert_eq!(listed[0].deleted_by, Some(phone));
    assert_eq!(listed[0].size, 0, "counted as bytes in trash/, which it has none of");

    // Past the week the computer keeps other deletions for.
    computer
        .store
        .db()
        .conn()
        .execute("UPDATE files SET deleted_at = deleted_at - ?1", [10 * 24 * 3600])
        .unwrap();
    computer.store.gc(Duration::from_secs(7 * 24 * 3600)).unwrap();

    assert_eq!(computer.store.restore_from_trash(listed[0].id).unwrap(), "Notes/diary.txt");
    assert!(computer.store.recently_deleted().unwrap().is_empty());
    let row = computer.store.db().live_row_in("Notes/diary.txt", Some(&phone)).unwrap().expect("not back");
    assert_eq!(row.content_hash, blake3::hash(&bytes));
    assert!(computer.store.db().is_held("Notes/diary.txt", &phone).unwrap(), "back, but not kept for the phone");
    // After the deletion, so that the deletion offered again is not taken
    // for a newer one; and this computer's doing.
    assert_eq!(row.vector.compare(&deletion.vector), Causality::After);
    assert_eq!(row.modified_by, Some(computer.id()));
    assert!(computer.store.can_read_content(&row.content_hash).unwrap(), "its bytes not here");
    assert!(!computer.root().join("Notes/diary.txt").exists(), "written into the computer's own folder");
}

#[test]
fn a_backup_or_a_guests_deletion_is_not_listed() {
    let phone = DeviceId::from_bytes([1; 32]);
    let mut computer = computer_of(&phone);
    let backup = phones_file(&phone, ".qurb-outside/whatsapp/Media/IMG-1.jpg", b"a photo", 1);
    hold(&mut computer, &phone, &backup, b"a photo");
    computer.store.unhold(&phones_deletion(&backup), &phone).unwrap();

    let guest = DeviceId::from_bytes([2; 32]);
    computer.store.db().trust_visitor(&guest, &[4; 32], "a guest", Relation::Guest, &[9; 32]).unwrap();
    let sealed = phones_file(&guest, "sealed-name", b"sealed bytes", 1);
    hold(&mut computer, &guest, &sealed, b"sealed bytes");
    computer.store.unhold(&phones_deletion(&sealed), &guest).unwrap();

    assert!(computer.store.recently_deleted().unwrap().is_empty());
}

#[test]
fn deleted_for_good_at_the_computer_its_bytes_go() {
    let phone = DeviceId::from_bytes([1; 32]);
    let mut computer = computer_of(&phone);
    let bytes = vec![6u8; 200_000];
    let kept = phones_file(&phone, "old.txt", &bytes, 1);
    hold(&mut computer, &phone, &kept, &bytes);
    computer.store.unhold(&phones_deletion(&kept), &phone).unwrap();

    let id = computer.store.recently_deleted().unwrap()[0].id;
    computer.store.forget_deleted(id).unwrap();
    computer.store.gc(Duration::ZERO).unwrap();
    assert!(!computer.store.can_read_content(&blake3::hash(&bytes)).unwrap(), "kept the 30 days all the same");
}

#[test]
fn the_phone_putting_it_back_itself_empties_the_entry() {
    let phone = DeviceId::from_bytes([1; 32]);
    let mut computer = computer_of(&phone);
    let kept = phones_file(&phone, "a.txt", b"the phone's", 1);
    hold(&mut computer, &phone, &kept, b"the phone's");
    let deletion = phones_deletion(&kept);
    computer.store.unhold(&deletion, &phone).unwrap();

    // Restored from the phone's own Recently deleted: its next change.
    let mut again = kept.clone();
    again.vector = deletion.vector.clone();
    again.vector.increment(phone);
    hold(&mut computer, &phone, &again, b"the phone's");
    assert!(computer.store.recently_deleted().unwrap().is_empty());
}

/// A phone keeping its vault on a computer, as `freed_deletion.rs` has it.
fn phone() -> Device {
    let mut phone = Device::new(22);
    phone.store.set_new_files_private(true);
    let computer = DeviceId::from_bytes([8; 32]);
    phone.store.db().set_peer_kind(&computer, "computer").unwrap();
    phone
}

/// What the computer offers once it has restored `path`: the phone's deleted
/// version, one change of the computer's after it.
fn restored_at_the_computer(phone: &Device, path: &str, bytes: &[u8]) -> FileVersion {
    let me = phone.id();
    let deleted = phone.store.db().deleted_row_in(path, &me, &blake3::hash(bytes)).unwrap().expect("not deleted");
    let mut vector = deleted.vector.clone();
    let computer = DeviceId::from_bytes([8; 32]);
    vector.increment(computer);
    FileVersion {
        path: path.into(),
        content: Content::File { hash: *blake3::hash(bytes).as_bytes(), size: bytes.len() as u64 },
        vector,
        modified_by: computer,
        modified_at: now(),
        area: Area::Held,
    }
}

#[test]
fn a_phone_takes_back_a_freed_file_restored_at_the_computer() {
    let mut phone = phone();
    let bytes = b"freed to the computer".to_vec();
    std::fs::create_dir_all(phone.root().join("Notes")).unwrap();
    std::fs::write(phone.root().join("Notes/a.txt"), &bytes).unwrap();
    phone.store.put_file("Notes/a.txt", &phone.root().join("Notes/a.txt")).unwrap();
    phone.store.db().note_replica(&blake3::hash(&bytes), &DeviceId::from_bytes([8; 32])).unwrap();
    phone.store.free_local("Notes/a.txt").unwrap();
    phone.store.delete_to_trash("Notes/a.txt", None).unwrap();
    let restored = restored_at_the_computer(&phone, "Notes/a.txt", &bytes);

    assert!(phone.store.take_back_restored(&restored).unwrap());
    let me = phone.id();
    let row = phone.store.db().live_row_in("Notes/a.txt", Some(&me)).unwrap().expect("not back");
    assert_eq!(row.vector, restored.vector, "not as the computer restored it");
    assert_eq!(phone.store.db().is_materialised("Notes/a.txt").unwrap(), Some(false));
    assert!(phone.store.recently_deleted().unwrap().is_empty());
    // Once only.
    assert!(!phone.store.take_back_restored(&restored).unwrap());
}

#[test]
fn a_phone_takes_back_a_file_restored_there_with_its_own_bytes() {
    let mut phone = phone();
    let bytes = b"still on the phone when deleted".to_vec();
    std::fs::write(phone.root().join("b.txt"), &bytes).unwrap();
    phone.store.put_file("b.txt", &phone.root().join("b.txt")).unwrap();
    phone.store.delete_to_trash("b.txt", None).unwrap();
    assert!(!phone.root().join("b.txt").exists());
    let restored = restored_at_the_computer(&phone, "b.txt", &bytes);

    assert!(phone.store.take_back_restored(&restored).unwrap());
    assert_eq!(std::fs::read(phone.root().join("b.txt")).unwrap(), bytes, "not back from trash/");
    assert!(phone.store.recently_deleted().unwrap().is_empty());
}

#[test]
fn a_phone_takes_back_nothing_else() {
    let mut phone = phone();
    let bytes = b"what was deleted".to_vec();
    std::fs::write(phone.root().join("c.txt"), &bytes).unwrap();
    phone.store.put_file("c.txt", &phone.root().join("c.txt")).unwrap();
    phone.store.delete_to_trash("c.txt", None).unwrap();
    let restored = restored_at_the_computer(&phone, "c.txt", &bytes);

    // Other bytes at that path.
    let mut other = restored.clone();
    other.content = Content::File { hash: *blake3::hash(b"something else").as_bytes(), size: 14 };
    assert!(!phone.store.restored_elsewhere(&other).unwrap());
    // Not after the deletion: the version from before it.
    let mut before = restored.clone();
    before.vector = VersionVector::new();
    before.vector.increment(phone.id());
    assert!(!phone.store.restored_elsewhere(&before).unwrap());
    // A path never deleted here.
    let mut elsewhere = restored.clone();
    elsewhere.path = "never-here.txt".into();
    assert!(!phone.store.restored_elsewhere(&elsewhere).unwrap());
    // Outside the folder.
    let mut escaping = restored.clone();
    escaping.path = "../c.txt".into();
    assert!(!phone.store.restored_elsewhere(&escaping).unwrap());

    assert!(phone.store.restored_elsewhere(&restored).unwrap());
}
