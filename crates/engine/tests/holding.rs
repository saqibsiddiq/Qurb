//! A phone keeps its own files, and a desktop holds them for it.
//!
//! Decision 0036, end to end in one process: the phone's photo goes into its
//! own vault, the desktop keeps a copy it never shows, the phone frees its
//! local copy and fetches it back, and edits and deletions reach the desktop.
//! And the two things that must not happen: a phone that lost its index
//! deleting its own backup, and any device but a holder seeing the vault.

use qurb_engine::{Engine, PlanStats, StoreSource};
use qurb_storage::db::Audience;
use qurb_storage::{ChunkKey, Store};
use qurb_sync::{Area, DeviceId};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;

const KEY: [u8; 32] = [44; 32];

struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("folder");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");
        let store = Store::open(&store_dir, ChunkKey::from_bytes(KEY)).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn phone() -> Self {
        let mut device = Self::new();
        device.engine.store_mut().set_new_files_private(true);
        device
    }

    fn id(&self) -> DeviceId {
        self.engine.store().device_id().unwrap()
    }

    fn write(&mut self, name: &str, bytes: &[u8]) {
        fs::write(self.root.join(name), bytes).unwrap();
        self.engine.reconcile().unwrap();
    }

    /// Sync from `peer` as the daemon does: its tree as it would show it to
    /// this device, planned knowing who it came from.
    fn sync_from(&mut self, peer: &Device) -> PlanStats {
        let me = self.id();
        let offered = peer.engine.store().tree_for(Audience::Device(&me)).unwrap();
        let plan = self.engine.plan_with(&offered, Some(&peer.id())).unwrap();
        let reader = Store::open(&peer.root.join(".qurb"), ChunkKey::from_bytes(KEY))
            .unwrap()
            .in_tree(&peer.root);
        let stats = self.engine.apply_plan(&plan, &mut StoreSource::new(&reader)).unwrap();
        assert!(stats.failures.is_empty(), "{:?}", stats.failures);
        stats
    }

    /// Whether this device keeps `bytes` for `owner`, as a live held row.
    fn holds(&self, owner: &DeviceId, path: &str, bytes: &[u8]) -> bool {
        let db = self.engine.store().db();
        match db.live_row_in(path, Some(owner)).unwrap() {
            Some(row) => {
                row.content_hash == blake3::hash(bytes) && db.is_held(path, owner).unwrap()
            }
            None => false,
        }
    }
}

/// Pair two devices the way pairing leaves them: each knows the other.
fn introduce(a: &Device, b: &Device) {
    for (one, other) in [(a, b), (b, a)] {
        let fingerprint = *blake3::hash(other.id().as_bytes()).as_bytes();
        one.engine.store().db().trust_peer(&other.id(), &fingerprint, "the other one").unwrap();
    }
}

fn phone_and_desktop() -> (Device, Device) {
    let phone = Device::phone();
    let desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();
    (phone, desktop)
}

#[test]
fn the_desktop_keeps_the_phones_photo_out_of_sight() {
    let (mut phone, mut desktop) = phone_and_desktop();
    phone.write("IMG_0001.jpg", b"a photo from the phone");

    let stats = desktop.sync_from(&phone);
    assert_eq!(stats.held, 1);

    assert!(desktop.holds(&phone.id(), "IMG_0001.jpg", b"a photo from the phone"));
    assert!(
        !desktop.root.join("IMG_0001.jpg").exists(),
        "the photo was put in the desktop's folder"
    );
    assert!(desktop.engine.tree().unwrap().is_empty(), "the desktop advertises the photo");
    let history = desktop.engine.store().db().activity(50, None).unwrap();
    assert!(
        history.iter().all(|a| a.path.as_deref() != Some("IMG_0001.jpg")),
        "the photo's name is in the desktop's history: {history:?}"
    );
    assert!(desktop.engine.store().pending_deliveries().unwrap().is_empty());
}

/// The acceptance test's steps 20 to 23: free the phone's copy, still know the
/// file, and have it back byte for byte.
#[test]
fn the_phone_frees_its_copy_and_gets_it_back() {
    let (mut phone, mut desktop) = phone_and_desktop();
    let photo: Vec<u8> =
        (0..400_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
    phone.write("IMG_0001.jpg", &photo);
    desktop.sync_from(&phone);

    // The desktop's Got, as its server would record it on the phone.
    phone.engine.store().note_replica(&blake3::hash(&photo), &desktop.id()).unwrap();
    phone.engine.store_mut().evict("IMG_0001.jpg").unwrap();
    assert!(!phone.root.join("IMG_0001.jpg").exists());
    phone.engine.reconcile().unwrap();
    assert!(
        desktop.holds(&phone.id(), "IMG_0001.jpg", &photo),
        "freeing reached the desktop as a deletion"
    );

    phone.engine.store().db().want("IMG_0001.jpg").unwrap();
    phone.sync_from(&desktop);

    assert_eq!(fs::read(phone.root.join("IMG_0001.jpg")).unwrap(), photo);
    assert_eq!(phone.engine.store().is_materialised("IMG_0001.jpg").unwrap(), Some(true));
    assert!(phone.engine.store().db().wanted_paths().unwrap().is_empty());
    let (_, area) = phone.engine.store().db().folder_row("IMG_0001.jpg").unwrap().unwrap();
    assert_eq!(area, Some(phone.id()), "fetched back into the wrong area");
    assert!(phone.engine.tree().unwrap().is_empty(), "the phone's own file became shared");
}

/// The phone never collects its own files from the desktop as deliveries.
#[test]
fn the_phone_does_not_receive_its_own_files() {
    let (mut phone, mut desktop) = phone_and_desktop();
    phone.write("IMG_0001.jpg", b"a photo");
    desktop.sync_from(&phone);

    let stats = phone.sync_from(&desktop);
    assert_eq!(stats.adopted, 0);
    let files: Vec<_> = fs::read_dir(&phone.root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n != ".qurb")
        .collect();
    assert_eq!(files, vec!["IMG_0001.jpg"], "a second copy was filed");
}

#[test]
fn an_edit_and_a_deletion_reach_the_desktop() {
    let (mut phone, mut desktop) = phone_and_desktop();
    phone.write("notes.txt", b"first draft");
    desktop.sync_from(&phone);

    phone.write("notes.txt", b"second draft");
    desktop.sync_from(&phone);
    assert!(desktop.holds(&phone.id(), "notes.txt", b"second draft"));

    fs::remove_file(phone.root.join("notes.txt")).unwrap();
    phone.engine.reconcile().unwrap();
    desktop.sync_from(&phone);
    let db = desktop.engine.store().db();
    let row = db.live_row_in("notes.txt", Some(&phone.id())).unwrap();
    assert!(row.is_none(), "still held after it was deleted");
}

/// A phone that lost its index -- wiped, reinstalled over the same identity --
/// shows the desktop an empty list. That is not an instruction to delete, and
/// the backup must survive it.
#[test]
fn a_wiped_phone_does_not_delete_its_own_backup() {
    let (mut phone, mut desktop) = phone_and_desktop();
    phone.write("IMG_0001.jpg", b"the only photo of something");
    desktop.sync_from(&phone);

    // What a wiped phone shows its holder: its own vault, now empty.
    let me = desktop.id();
    let shown: Vec<_> = phone
        .engine
        .store()
        .tree_for(Audience::Device(&me))
        .unwrap()
        .into_iter()
        .filter(|v| v.area != Area::Hold)
        .collect();
    let plan = desktop.engine.plan_with(&shown, Some(&phone.id())).unwrap();
    assert!(plan.is_empty(), "an empty list became a plan: {plan:?}");
    assert!(desktop.holds(&phone.id(), "IMG_0001.jpg", b"the only photo of something"));
}

/// Only a device the phone named sees its vault: not in the tree, and not by
/// asking for the bytes directly.
#[test]
fn a_device_that_is_not_a_holder_sees_nothing() {
    let (mut phone, _desktop) = phone_and_desktop();
    let tablet = Device::new();
    introduce(&phone, &tablet);
    phone.write("IMG_0001.jpg", b"a private photo");

    let tree = phone.engine.store().tree_for(Audience::Device(&tablet.id())).unwrap();
    assert!(tree.is_empty(), "shown to a device that is not a holder: {tree:?}");

    let content = blake3::hash(b"a private photo");
    let db = phone.engine.store().db();
    assert!(!db.content_visible_to(&content, Audience::Device(&tablet.id())).unwrap());
    let chunks = phone.engine.store().chunk_hashes_for_content(&content).unwrap().unwrap();
    assert!(!db.chunk_visible_to(&chunks[0], Audience::Device(&tablet.id())).unwrap());

    // And stops seeing it the moment it is no longer one.
    let desktop_id = phone.engine.store().db().holders().unwrap()[0];
    phone.engine.store().db().remove_holder(&desktop_id).unwrap();
    let tree = phone.engine.store().tree_for(Audience::Device(&desktop_id)).unwrap();
    assert!(tree.is_empty());
}

/// A deletion undone at the desktop (decision 0073, item 3): restored from
/// its Recently deleted, the desktop keeps it although the phone, not yet
/// told, offers its deletion again; then the phone takes it back, from the
/// bytes its own Recently deleted kept, and the two agree.
#[test]
fn a_deletion_undone_at_the_desktop_is_not_taken_again_and_the_phone_takes_it_back() {
    let (mut phone, mut desktop) = phone_and_desktop();
    let bytes = b"a note the phone should not have deleted";
    phone.write("note.txt", bytes);
    desktop.sync_from(&phone);
    assert!(desktop.holds(&phone.id(), "note.txt", bytes));

    phone.engine.store_mut().delete_to_trash("note.txt", None).unwrap();
    desktop.sync_from(&phone);
    assert!(!desktop.holds(&phone.id(), "note.txt", bytes));
    let listed = desktop.engine.store().recently_deleted().unwrap();
    assert_eq!(listed.len(), 1, "not in the desktop's Recently deleted");
    desktop.engine.store_mut().restore_from_trash(listed[0].id).unwrap();

    // The phone's deletion, offered again: the restore is after it.
    desktop.sync_from(&phone);
    assert!(desktop.holds(&phone.id(), "note.txt", bytes), "deleted again by the deletion it undid");

    let stats = phone.sync_from(&desktop);
    assert_eq!(stats.adopted, 1);
    assert_eq!(fs::read(phone.root.join("note.txt")).unwrap(), bytes);
    assert!(phone.engine.store().recently_deleted().unwrap().is_empty());
    let history = phone.engine.store().db().activity(10, None).unwrap();
    assert!(history.iter().any(|a| a.path.as_deref() == Some("note.txt") && a.device == Some(desktop.id())));

    // Settled: nothing more either way.
    let stats = desktop.sync_from(&phone);
    assert_eq!((stats.held, stats.adopted), (0, 0));
    assert!(desktop.holds(&phone.id(), "note.txt", bytes));
    assert!(desktop.engine.store().recently_deleted().unwrap().is_empty());
    let stats = phone.sync_from(&desktop);
    assert_eq!(stats.adopted, 0);
}
