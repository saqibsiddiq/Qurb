//! Keeping another device's own files for it, and a phone keeping its own.
//!
//! Decision 0036. A phone's files go into its own vault, and a device it
//! chooses holds a copy for it: in the chunk store, never in the folder, and
//! never released -- so the phone can free its local copy and have it back.
//!
//! Most of what can go wrong here is one rule leaking into another. A held file
//! sits in the owner's vault exactly like a send does, and the rules for sends
//! -- release once collected, list as waiting -- would destroy a backup if they
//! applied to it. These tests are mostly that line.

use qurb_storage::{ChunkKey, Store};
use qurb_sync::{Area, Content, DeviceId, FileVersion, VersionVector};

fn noisy(size: usize, seed: u32) -> Vec<u8> {
    let mut out = vec![0u8; size];
    let mut x = seed;
    for byte in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *byte = x as u8;
    }
    out
}

struct Device {
    dir: tempfile::TempDir,
    store: Store,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("folder");
        std::fs::create_dir_all(&root).unwrap();
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([13; 32]))
            .unwrap()
            .in_tree(&root);
        Self { dir, store }
    }

    /// A phone: its new files are its own.
    fn phone() -> Self {
        let mut device = Self::new();
        device.store.set_new_files_private(true);
        device
    }

    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("folder")
    }

    fn id(&self) -> DeviceId {
        self.store.device_id().unwrap()
    }

    /// A file added on this device, the way the scan adds one.
    fn add(&mut self, name: &str, bytes: &[u8]) {
        let path = self.root().join(name);
        std::fs::write(&path, bytes).unwrap();
        self.store.put_file(name, &path).unwrap();
    }

    /// Where a path's live row sits: "shared", "mine", or another vault.
    fn area(&self, path: &str) -> &'static str {
        match self.store.db().folder_row(path).unwrap() {
            Some((_, None)) => "shared",
            Some((_, Some(_))) => "mine",
            None => "nowhere",
        }
    }

    fn outgoing(&self) -> Vec<String> {
        self.store.pending_deliveries().unwrap().into_iter().map(|(p, _, _)| p).collect()
    }
}

/// The owner's file as the holder would be told about it.
fn owners_file(owner: &DeviceId, path: &str, bytes: &[u8]) -> FileVersion {
    let mut vector = VersionVector::new();
    vector.increment(*owner);
    FileVersion {
        path: path.to_string(),
        content: Content::File { hash: *blake3::hash(bytes).as_bytes(), size: bytes.len() as u64 },
        vector,
        modified_by: *owner,
        modified_at: 1_790_000_000,
        area: Area::Hold,
    }
}

/// Hold `bytes` for `owner` on `holder`, staged outside the folder as the
/// engine stages them.
fn hold(holder: &mut Device, owner: &DeviceId, path: &str, bytes: &[u8]) {
    let staged = holder.dir.path().join("staged");
    std::fs::write(&staged, bytes).unwrap();
    holder.store.hold_file(&owners_file(owner, path, bytes), owner, &staged).unwrap();
    std::fs::remove_file(&staged).unwrap();
}

#[test]
fn a_phones_new_files_are_its_own() {
    let mut phone = Device::phone();
    phone.add("IMG_0001.jpg", b"a photo");
    assert_eq!(phone.area("IMG_0001.jpg"), "mine");

    // And they are not on their way to anybody.
    assert!(phone.outgoing().is_empty(), "{:?}", phone.outgoing());
}

/// An edit does not move a file between areas, in either direction.
#[test]
fn a_shared_file_edited_on_a_phone_stays_shared() {
    let mut phone = Device::new();
    phone.add("notes.txt", b"everybody's");
    phone.store.set_new_files_private(true);
    phone.add("notes.txt", b"everybody's, edited");
    assert_eq!(phone.area("notes.txt"), "shared");
}

#[test]
fn a_desktops_new_files_are_still_shared() {
    let mut desktop = Device::new();
    desktop.add("report.pdf", b"a report");
    assert_eq!(desktop.area("report.pdf"), "shared");
}

#[test]
fn a_held_file_is_kept_out_of_sight() {
    let phone = Device::phone();
    let mut desktop = Device::new();
    let photo = noisy(300_000, 1);
    hold(&mut desktop, &phone.id(), "IMG_0001.jpg", &photo);

    // Readable, for when the phone asks for it back...
    assert_eq!(desktop.store.read_content(&blake3::hash(&photo)).unwrap().unwrap(), photo);
    // ...and nowhere a person on the desktop would find it.
    assert!(!desktop.root().join("IMG_0001.jpg").exists());
    assert!(desktop.store.db().listing(None, 100, 0).unwrap().is_empty());
    assert!(desktop.store.db().search("IMG", 10).unwrap().is_empty());
    assert!(desktop.outgoing().is_empty(), "a held file is not a send");
}

/// The rule that makes holding worth anything. A send is released once the
/// recipient has it; a held file is kept however much the owner says it has
/// its own copy, because the owner is about to free it.
#[test]
fn a_held_file_is_never_released() {
    let phone = Device::phone();
    let mut desktop = Device::new();
    let photo = noisy(300_000, 2);
    hold(&mut desktop, &phone.id(), "IMG_0001.jpg", &photo);

    let content = blake3::hash(&photo);
    desktop.store.note_replica_in_vault(&content, &phone.id()).unwrap();
    desktop.store.note_replica(&content, &phone.id()).unwrap();
    let released = desktop.store.release_held_payloads().unwrap();

    assert_eq!(released.chunks_removed, 0, "the backup was released");
    assert_eq!(desktop.store.read_content(&content).unwrap().unwrap(), photo);
}

/// Something this device sent the phone, which the phone then keeps as its
/// own: the same bytes, now held rather than on their way, and no longer
/// releasable.
#[test]
fn a_collected_send_becomes_held() {
    let phone = Device::phone();
    let mut desktop = Device::new();
    let project = noisy(200_000, 3);
    let loose = desktop.dir.path().join("project.zip");
    std::fs::write(&loose, &project).unwrap();
    desktop.store.send_to_vault("project.zip", &loose, &phone.id()).unwrap();
    assert_eq!(desktop.outgoing(), vec!["project.zip".to_string()]);

    let content = blake3::hash(&project);
    desktop.store.note_replica_in_vault(&content, &phone.id()).unwrap();
    hold(&mut desktop, &phone.id(), "project.zip", &project);

    assert!(desktop.outgoing().is_empty());
    assert_eq!(desktop.store.release_held_payloads().unwrap().chunks_removed, 0);
    assert_eq!(desktop.store.read_content(&content).unwrap().unwrap(), project);
}

#[test]
fn a_held_file_goes_when_the_owner_deletes_it_and_not_before() {
    let phone = Device::phone();
    let mut desktop = Device::new();
    hold(&mut desktop, &phone.id(), "IMG_0001.jpg", b"a photo");

    // Absent from a list is not an instruction. Only a tombstone is.
    let mut vector = VersionVector::new();
    vector.increment(phone.id());
    vector.increment(phone.id());
    let deleted = FileVersion {
        path: "IMG_0001.jpg".into(),
        content: Content::Deleted,
        vector,
        modified_by: phone.id(),
        modified_at: 1_790_000_100,
        area: Area::Hold,
    };
    desktop.store.unhold(&deleted, &phone.id()).unwrap();

    let row = desktop.store.db().live_row_in("IMG_0001.jpg", Some(&phone.id())).unwrap();
    assert!(row.is_none(), "still held after the owner deleted it");
    // A second time, or for something never held, is nothing.
    desktop.store.unhold(&deleted, &phone.id()).unwrap();
}

/// On the owner, the holder's copy is one it can ask back, so the file can be
/// freed. Without a holder it cannot.
#[test]
fn a_phone_frees_its_own_file_only_when_somebody_holds_it() {
    let mut phone = Device::phone();
    let photo = noisy(250_000, 4);
    phone.add("IMG_0001.jpg", &photo);

    assert!(phone.store.evict("IMG_0001.jpg").is_err(), "freed the only copy");
    assert!(phone.root().join("IMG_0001.jpg").exists());

    let desktop = DeviceId::from_bytes([0xD0; 32]);
    phone.store.note_replica(&blake3::hash(&photo), &desktop).unwrap();
    let candidates: Vec<String> =
        phone.store.evictable().unwrap().into_iter().map(|(p, _, _)| p).collect();
    assert_eq!(candidates, vec!["IMG_0001.jpg".to_string()]);

    phone.store.evict("IMG_0001.jpg").unwrap();
    assert!(!phone.root().join("IMG_0001.jpg").exists());
    assert_eq!(phone.store.is_materialised("IMG_0001.jpg").unwrap(), Some(false));
    assert_eq!(phone.store.evicted().unwrap(), vec!["IMG_0001.jpg".to_string()]);
    assert_eq!(phone.area("IMG_0001.jpg"), "mine", "freeing moved it between areas");
}

#[test]
fn only_the_listed_devices_are_holders() {
    let phone = Device::phone();
    let desktop = DeviceId::from_bytes([0xD0; 32]);
    let tablet = DeviceId::from_bytes([0x7A; 32]);

    phone.store.db().add_holder(&desktop).unwrap();
    phone.store.db().add_holder(&desktop).unwrap();
    assert_eq!(phone.store.db().holders().unwrap(), vec![desktop]);
    assert!(phone.store.db().is_holder(&desktop).unwrap());
    assert!(!phone.store.db().is_holder(&tablet).unwrap());

    assert!(phone.store.db().remove_holder(&desktop).unwrap());
    assert!(phone.store.db().holders().unwrap().is_empty());
}

/// A file somebody sent here sits in this device's own vault. It is not a send
/// waiting for this device to collect it -- which is what the list of things on
/// their way used to say about it, because it looked at every vault.
#[test]
fn a_file_sent_here_is_not_waiting_for_this_device() {
    let mut phone = Device::new();
    let bytes = b"tickets";
    let path = phone.root().join("tickets.pdf");
    std::fs::write(&path, bytes).unwrap();
    let sender = DeviceId::from_bytes([0x5E; 32]);
    let mut version = owners_file(&sender, "tickets.pdf", bytes);
    version.area = Area::Sent;
    phone.store.adopt_file_privately(&version, &path, 0).unwrap();

    assert!(phone.outgoing().is_empty(), "{:?}", phone.outgoing());
    assert!(phone.store.db().awaiting_collection().unwrap().is_empty());
}

/// A phone's folder read once as a tree answers every folder as reading it
/// folder by folder does (`qurb_storage::person::tree`, which a mounted
/// folder answers from).
#[test]
fn a_phones_folder_read_once_is_the_same_as_folder_by_folder() {
    let mut desktop = Device::new();
    let phone = Device::phone();
    let owner = phone.id();
    for (i, path) in [
        ".qurb-outside/whatsapp/Media/WhatsApp Images/IMG-1.jpg",
        ".qurb-outside/whatsapp/Media/WhatsApp Images/IMG-2.jpg",
        ".qurb-outside/whatsapp/Databases/msgstore.db.crypt14",
        ".qurb-outside/storage/DCIM/Camera/PXL_1.jpg",
        "notes.txt",
    ]
    .iter()
    .enumerate()
    {
        hold(&mut desktop, &owner, path, &noisy(1_000 + i * 10, i as u32 + 1));
    }
    let folder = ("sakib".to_string(), vec![owner]);
    let tree = qurb_storage::person::tree(desktop.store.db(), &folder).unwrap();
    for dir in tree.keys() {
        let inside = if dir.is_empty() { "sakib".to_string() } else { format!("sakib/{dir}") };
        let (folders, files) = qurb_storage::person::listing(desktop.store.db(), std::slice::from_ref(&folder), &inside).unwrap();
        let (tree_folders, tree_files) = &tree[dir];
        assert_eq!(&folders, tree_folders, "folders in {dir:?}");
        assert_eq!(&files, tree_files, "files in {dir:?}");
    }
    assert!(tree.len() > 3, "too few folders: {:?}", tree.keys());
}
