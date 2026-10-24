//! Changes a phone asks of a computer's shared folder, made by the computer
//! (decision 0073, item 6): the rules, without a network -- what a name may
//! be, where it may go, that a file is put only once its bytes are what was
//! said, and who a deletion is recorded as.

use qurb_storage::shared_folder::{delete_path, make_folder, move_path, place};
use qurb_storage::{ChunkKey, Error, Store};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

struct Computer {
    dir: tempfile::TempDir,
    root: PathBuf,
    store: Store,
}

impl Computer {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("folder");
        std::fs::create_dir_all(&root).unwrap();
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([21; 32])).unwrap().in_tree(&root);
        Self { dir, root, store }
    }

    fn add(&mut self, path: &str, bytes: &[u8]) {
        let on_disk = self.root.join(path);
        std::fs::create_dir_all(on_disk.parent().unwrap()).unwrap();
        std::fs::write(&on_disk, bytes).unwrap();
        self.store.put_file(path, &on_disk).unwrap();
    }

    /// A file as a phone's pieces leave it, outside the folder.
    fn staged(&self, bytes: &[u8]) -> PathBuf {
        let path = self.dir.path().join(format!("staged-{}", blake3::hash(bytes).to_hex()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn live(&self, path: &str) -> bool {
        matches!(self.store.db().folder_row(path).unwrap(), Some((_, None)))
    }
}

fn phone() -> qurb_sync::DeviceId {
    qurb_sync::DeviceId::from_bytes([7; 32])
}

fn refusal(result: qurb_storage::Result<impl std::fmt::Debug>) -> String {
    match result {
        Err(Error::Sharing { why }) => why,
        other => panic!("not refused with a reason: {other:?}"),
    }
}

#[test]
fn a_file_is_put_only_when_its_bytes_are_what_was_said() {
    let mut computer = Computer::new();
    let staged = computer.staged(b"the photo");
    let wrong = blake3::hash(b"another photo");
    assert!(refusal(place(&mut computer.store, &phone(), &staged, "beach.jpg", &wrong)).contains("damaged"));
    assert!(!computer.root.join("beach.jpg").exists(), "a damaged file put in the folder");

    let right = blake3::hash(b"the photo");
    let placed = place(&mut computer.store, &phone(), &staged, "Trips/beach.jpg", &right).unwrap();
    assert_eq!(placed, "Trips/beach.jpg");
    assert_eq!(std::fs::read(computer.root.join("Trips/beach.jpg")).unwrap(), b"the photo");
    assert!(computer.live("Trips/beach.jpg"), "not indexed as a shared file");

    // The same name again is numbered, never put over the first.
    let again = computer.staged(b"the photo, again");
    let placed = place(&mut computer.store, &phone(), &again, "Trips/beach.jpg", &blake3::hash(b"the photo, again")).unwrap();
    assert_eq!(placed, "Trips/beach (2).jpg");
    assert_eq!(std::fs::read(computer.root.join("Trips/beach.jpg")).unwrap(), b"the photo");
}

#[test]
fn nothing_hidden_nothing_outside_and_no_folder_shared_without_it() {
    let mut computer = Computer::new();
    computer.add("notes.txt", b"shopping");
    computer.add("Private/salary.txt", b"nobody else's");
    let rules = BTreeMap::from([("Private".to_string(), BTreeSet::from([qurb_sync::DeviceId::from_bytes([9; 32])]))]);
    computer.store.db().replace_shares(&qurb_sync::sharing::Rules::new(rules), "test").unwrap();

    for path in [".qurb/chunks", "../outside", "/etc/passwd", "Photos/.hidden", ".qurbshare", ""] {
        refusal(make_folder(&mut computer.store, &phone(), path));
    }
    refusal(move_path(&mut computer.store, &phone(), "notes.txt", ".qurb/notes.txt"));
    refusal(move_path(&mut computer.store, &phone(), "notes.txt", "../notes.txt"));
    assert!(refusal(make_folder(&mut computer.store, &phone(), "Private/Mine")).contains("not in the shared folder"));
    refusal(delete_path(&mut computer.store, &phone(), "Private/salary.txt"));
    refusal(move_path(&mut computer.store, &phone(), "notes.txt", "Private/notes.txt"));
    assert!(computer.live("Private/salary.txt") && computer.live("notes.txt"), "something changed anyway");
}

#[test]
fn a_taken_name_is_refused_and_a_folder_does_not_go_inside_itself() {
    let mut computer = Computer::new();
    computer.add("Trips/beach.jpg", b"sand");
    computer.add("notes.txt", b"shopping");
    assert!(refusal(make_folder(&mut computer.store, &phone(), "Trips")).contains("already called"));
    assert!(refusal(move_path(&mut computer.store, &phone(), "notes.txt", "Trips/beach.jpg")).contains("already called"));
    assert!(refusal(move_path(&mut computer.store, &phone(), "Trips", "Trips/Older")).contains("inside itself"));
}

#[test]
fn a_folder_moves_and_is_deleted_file_by_file_into_recently_deleted() {
    let mut computer = Computer::new();
    computer.add("Trips/beach.jpg", b"sand");
    computer.add("Trips/2025/hills.jpg", b"green");
    make_folder(&mut computer.store, &phone(), "Trips/Empty").unwrap();

    move_path(&mut computer.store, &phone(), "Trips", "Holidays/Trips").unwrap();
    assert!(computer.live("Holidays/Trips/beach.jpg") && computer.live("Holidays/Trips/2025/hills.jpg"));
    assert!(!computer.live("Trips/beach.jpg"));
    assert!(!computer.root.join("Trips/beach.jpg").exists());

    delete_path(&mut computer.store, &phone(), "Holidays").unwrap();
    assert!(!computer.live("Holidays/Trips/beach.jpg"));
    assert!(!computer.root.join("Holidays").exists(), "the folder left behind");
    let trash = computer.store.recently_deleted().unwrap();
    let mut paths: Vec<&str> = trash.iter().map(|t| t.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, vec!["Holidays/Trips/2025/hills.jpg", "Holidays/Trips/beach.jpg"]);
    assert!(trash.iter().all(|t| t.deleted_by == Some(phone())), "not recorded as the phone's deletion");
}
