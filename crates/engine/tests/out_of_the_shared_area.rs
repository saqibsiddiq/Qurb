//! A device out of the shared area -- a phone, since decision 0073 -- takes
//! none of another device's shared files, nor changes to them, while what is
//! sent to it still arrives.

use qurb_engine::{Engine, StoreSource};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;

struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");
        let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore) }
    }

    fn write(&mut self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn id(&self) -> qurb_sync::DeviceId {
        self.engine.store().device_id().unwrap()
    }
}

/// `into` takes what it would from `from`, as a sync pass does.
fn take(into: &mut Device, from: &Device) {
    let plan = into.engine.plan_with(&from.engine.tree_for(&into.id()).unwrap(), Some(&from.id())).unwrap();
    let mut source = StoreSource::new(from.engine.store());
    let stats = into.engine.apply_plan(&plan, &mut source).unwrap();
    assert!(stats.is_clean(), "{:?}", stats.failures);
}

#[test]
fn a_phone_out_of_the_shared_area_takes_none_of_it_and_still_gets_what_is_sent() {
    let mut computer = Device::new();
    let mut phone = Device::new();
    phone.engine.set_shared_area(false);
    computer.write("Family/photo.jpg", "a photo");
    computer.write("notes.txt", "shopping");

    take(&mut phone, &computer);
    assert!(!phone.root.join("Family/photo.jpg").exists(), "a shared file taken");
    assert!(phone.engine.store().db().live_paths().unwrap().is_empty());

    // Nor a change to one, nor a deletion.
    computer.write("notes.txt", "shopping, and bread");
    fs::remove_file(computer.root.join("Family/photo.jpg")).unwrap();
    computer.engine.reconcile().unwrap();
    take(&mut phone, &computer);
    assert!(phone.engine.store().db().live_paths().unwrap().is_empty());

    // What is sent to it arrives as before.
    let outgoing = computer.root.parent().unwrap().join("for-the-phone.txt");
    fs::write(&outgoing, "for the phone only").unwrap();
    computer.engine.store_mut().send_to_vault("for-the-phone.txt", &outgoing, &phone.id()).unwrap();
    take(&mut phone, &computer);
    assert_eq!(fs::read_to_string(phone.root.join("for-the-phone.txt")).unwrap(), "for the phone only");

    // And a device in it, as a computer is, takes the shared area as ever.
    let mut laptop = Device::new();
    take(&mut laptop, &computer);
    assert_eq!(fs::read_to_string(laptop.root.join("notes.txt")).unwrap(), "shopping, and bread");
}
