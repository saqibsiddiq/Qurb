//! Files of a device's own vault that live outside its folder, read where they
//! lie (decision 0062): on a phone, WhatsApp's folder.
//!
//! What matters is that nothing is copied, that a device keeping the vault can
//! still be given every byte, and that a file going from the outside folder
//! becomes the right thing -- kept by that device, or deleted -- and never
//! anything the folder scan mistakes for a deletion of its own.

use qurb_storage::db::Audience;
use qurb_storage::{ChunkKey, OutsideEntry, Store};
use qurb_sync::{Area, Content};

const PREFIX: &str = ".qurb-outside/whatsapp";

fn noisy(size: usize, seed: u32) -> Vec<u8> {
    let mut out = vec![0u8; size];
    let mut x = seed.max(1);
    for byte in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *byte = x as u8;
    }
    out
}

struct Phone {
    dir: tempfile::TempDir,
    store: Store,
}

impl Phone {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("folder");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(dir.path().join("whatsapp")).unwrap();
        let mut store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([21; 32]))
            .unwrap()
            .in_tree(&root);
        store.set_new_files_private(true);
        Self { dir, store }
    }

    /// The outside folder: WhatsApp's, in the real thing.
    fn outside(&self) -> std::path::PathBuf {
        self.dir.path().join("whatsapp")
    }

    /// Write a file into the outside folder, with a modification time of its
    /// own so that a listing can tell versions apart.
    fn write(&self, relative: &str, bytes: &[u8], mtime_ns: i64) -> OutsideEntry {
        let path = self.outside().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        OutsideEntry {
            path: relative.to_string(),
            source: path.to_string_lossy().into_owned(),
            size: bytes.len() as u64,
            mtime_ns,
        }
    }

    fn sync(&mut self, listing: &[OutsideEntry], keep: bool) -> qurb_storage::OutsideReport {
        self.store.keep_outside_folder(PREFIX, listing, &|_| keep, None, &|_| false).unwrap()
    }

    fn live(&self, relative: &str) -> Option<qurb_storage::db::FileRow> {
        let me = self.store.device_id().unwrap();
        self.store.db().live_row_in(&format!("{PREFIX}/{relative}"), Some(&me)).unwrap()
    }
}

/// A device to keep the phone's vault: a computer, which the phone may count
/// on (decision 0053).
fn computer() -> qurb_sync::DeviceId {
    qurb_sync::DeviceId::from_bytes([0x5a; 32])
}

#[test]
fn a_file_kept_outside_is_in_the_vault_and_read_where_it_lies() {
    let mut phone = Phone::new();
    let photo = noisy(400_000, 1);
    let entry = phone.write("Media/WhatsApp Images/IMG-1.jpg", &photo, 1);

    let report = phone.sync(&[entry], true);
    assert_eq!(report.added, 1);
    assert_eq!(report.bytes_read, photo.len() as u64);

    // In this device's own vault, with no copy anywhere qurb keeps.
    let row = phone.live("Media/WhatsApp Images/IMG-1.jpg").expect("not in the vault");
    assert_eq!(row.size, photo.len() as u64);
    assert_eq!(phone.store.usage().unwrap().chunks, 0, "the chunk store holds a copy");
    assert!(!phone.store.root().join(PREFIX).exists(), "copied into the folder");

    // Every byte can still be given to a device that keeps the vault.
    let content = blake3::hash(&photo);
    assert_eq!(phone.store.read_content(&content).unwrap().expect("unreadable"), photo);
    assert!(phone.store.held_chunks(&content).unwrap().is_some(), "not offered");
    assert!(phone.store.verify(false).unwrap().missing.is_empty());

    // And it is offered on a network that does not count what it carries: a
    // holder is shown it in the area it keeps -- and not on mobile data.
    phone.store.db().add_holder(&computer()).unwrap();
    let shown = phone.store.tree_for(Audience::Device(&computer())).unwrap();
    assert!(shown.iter().all(|v| !v.path.starts_with(".qurb-outside/")), "offered on mobile data");
    phone.store.db().set_outside_offered(true).unwrap();
    let shown = phone.store.tree_for(Audience::Device(&computer())).unwrap();
    let version = shown
        .iter()
        .find(|v| v.path == format!("{PREFIX}/Media/WhatsApp Images/IMG-1.jpg"))
        .expect("the holder is not shown it");
    assert_eq!(version.area, Area::Hold);
}

/// The scan of qurb's own folder deletes what the index has and the folder
/// does not -- except what it knows is not here, as a freed file is not. A
/// file kept outside must be one of those, or the first scan would delete the
/// whole backup.
#[test]
fn the_folder_scan_does_not_take_it_for_a_deletion() {
    let mut phone = Phone::new();
    let entry = phone.write("Databases/msgstore.db.crypt14", &noisy(50_000, 2), 1);
    phone.sync(&[entry], false);

    let path = format!("{PREFIX}/Databases/msgstore.db.crypt14");
    assert_eq!(phone.store.is_materialised(&path).unwrap(), Some(false));
}

#[test]
fn a_listing_adds_what_is_new_reads_what_changed_and_leaves_the_rest() {
    let mut phone = Phone::new();
    let a = phone.write("Media/a.jpg", &noisy(30_000, 3), 1);
    let b = phone.write("Media/b.jpg", &noisy(30_000, 4), 1);
    let c = phone.write("Databases/msgstore-increment-1.db.crypt14", &noisy(9_000, 5), 1);
    assert_eq!(phone.sync(&[a.clone(), b.clone(), c.clone()], true).added, 3);

    let again = phone.sync(&[a.clone(), b.clone(), c.clone()], true);
    assert_eq!((again.added, again.changed, again.unchanged), (0, 0, 3));
    assert_eq!(again.bytes_read, 0, "unchanged files were read again");

    let newer = noisy(12_000, 6);
    let c2 = phone.write("Databases/msgstore-increment-1.db.crypt14", &newer, 2);
    let changed = phone.sync(&[a, b, c2], true);
    assert_eq!((changed.added, changed.changed, changed.unchanged), (0, 1, 2));
    assert_eq!(phone.live("Databases/msgstore-increment-1.db.crypt14").unwrap().size, newer.len() as u64);
    assert_eq!(phone.store.read_content(&blake3::hash(&newer)).unwrap().unwrap(), newer);
}

/// Answer 2 of decision 0062: a photo deleted in WhatsApp stays on the
/// computer. On the phone it becomes what a freed file is.
#[test]
fn a_file_gone_from_the_folder_is_kept_when_a_computer_holds_it() {
    let mut phone = Phone::new();
    let photo = noisy(60_000, 7);
    let entry = phone.write("Media/old.jpg", &photo, 1);
    phone.sync(&[entry], true);
    let content = blake3::hash(&photo);
    phone.store.note_replica(&content, &computer()).unwrap();

    std::fs::remove_file(phone.outside().join("Media/old.jpg")).unwrap();
    let report = phone.sync(&[], true);

    assert_eq!((report.kept, report.deleted), (1, 0));
    assert!(phone.live("Media/old.jpg").is_some(), "deleted from the vault");
    let rows = phone.store.db().outside_under(PREFIX).unwrap();
    assert!(rows[0].source.is_none(), "still read from where it was");

    // A second listing has nothing more to say about it.
    let again = phone.sync(&[], true);
    assert_eq!((again.kept, again.deleted), (0, 0));
}

/// Kept only if it can be: a photo no computer ever got is gone.
#[test]
fn a_file_gone_that_nobody_holds_is_deleted() {
    let mut phone = Phone::new();
    let entry = phone.write("Media/unsent.jpg", &noisy(20_000, 8), 1);
    phone.sync(&[entry], true);

    std::fs::remove_file(phone.outside().join("Media/unsent.jpg")).unwrap();
    let report = phone.sync(&[], true);

    assert_eq!((report.kept, report.deleted), (0, 1));
    assert!(phone.live("Media/unsent.jpg").is_none());
}

/// A chat backup WhatsApp replaced is deleted, held or not, so that the
/// computer keeps one consistent set and nothing stale beside it.
#[test]
fn a_chat_backup_gone_from_the_folder_is_deleted_for_the_computer_too() {
    let mut phone = Phone::new();
    let increment = noisy(9_000, 9);
    let entry = phone.write("Databases/msgstore-increment-1.db.crypt14", &increment, 1);
    phone.sync(&[entry], false);
    phone.store.note_replica(&blake3::hash(&increment), &computer()).unwrap();
    phone.store.db().add_holder(&computer()).unwrap();
    phone.store.db().set_outside_offered(true).unwrap();

    std::fs::remove_file(phone.outside().join("Databases/msgstore-increment-1.db.crypt14")).unwrap();
    let report = phone.sync(&[], false);
    assert_eq!((report.kept, report.deleted), (0, 1));

    // The computer is told, the one instruction it acts on.
    let shown = phone.store.tree_for(Audience::Device(&computer())).unwrap();
    let version = shown
        .iter()
        .find(|v| v.path == format!("{PREFIX}/Databases/msgstore-increment-1.db.crypt14"))
        .expect("the holder is not told");
    assert!(matches!(version.content, Content::Deleted));
}

/// The first run on the S23 is 9.65 GB. One that runs out of time leaves the
/// rest for the next, which does not read again what is already kept.
#[test]
fn a_run_stops_at_its_deadline_and_the_next_carries_on() {
    let mut phone = Phone::new();
    let listing: Vec<OutsideEntry> =
        (0..4).map(|i| phone.write(&format!("Media/{i}.jpg"), &noisy(10_000, 10 + i), 1)).collect();

    let past = std::time::Instant::now();
    let stopped = phone.store.keep_outside_folder(PREFIX, &listing, &|_| true, Some(past), &|_| false).unwrap();
    assert_eq!((stopped.added, stopped.remaining), (0, 4));

    let finished = phone.sync(&listing, true);
    assert_eq!((finished.added, finished.remaining), (4, 0));
}

/// A file changed where it lies, with a listing that has not caught up: what
/// was recorded is not what is there, so nothing is served under the old
/// hashes -- the next listing makes it a new version instead.
#[test]
fn a_file_changed_behind_the_listing_is_not_served_as_the_old_one() {
    let mut phone = Phone::new();
    let original = noisy(40_000, 20);
    let entry = phone.write("Media/edited.jpg", &original, 1);
    phone.sync(&[entry], true);

    std::fs::write(phone.outside().join("Media/edited.jpg"), noisy(40_000, 21)).unwrap();
    assert!(phone.store.read_content(&blake3::hash(&original)).unwrap().is_none());
    assert!(phone.store.held_chunks(&blake3::hash(&original)).unwrap().is_some(), "setup");
}

/// A photo kept by the computer after WhatsApp deleted it, and then saved
/// into WhatsApp again: read from where it lies once more.
#[test]
fn a_file_that_comes_back_is_read_where_it_lies_again() {
    let mut phone = Phone::new();
    let photo = noisy(30_000, 30);
    let entry = phone.write("Media/back.jpg", &photo, 1);
    phone.sync(&[entry.clone()], true);
    phone.store.note_replica(&blake3::hash(&photo), &computer()).unwrap();
    std::fs::remove_file(phone.outside().join("Media/back.jpg")).unwrap();
    assert_eq!(phone.sync(&[], true).kept, 1);

    let entry = phone.write("Media/back.jpg", &photo, 2);
    let report = phone.sync(&[entry], true);
    assert_eq!(report.changed, 1);
    let rows = phone.store.db().outside_under(PREFIX).unwrap();
    assert!(rows[0].source.is_some());
    assert_eq!(phone.store.read_content(&blake3::hash(&photo)).unwrap().unwrap(), photo);
}

/// WhatsApp's files are a backup, not the person's files in qurb: not listed,
/// not found by a search, not counted, not "waiting to reach your devices",
/// and not among what clearing the app's data would lose -- it would lose
/// none of them, since they are WhatsApp's.
#[test]
fn a_folder_kept_outside_is_in_no_list_a_person_is_shown() {
    let mut phone = Phone::new();
    let entry = phone.write("Media/WhatsApp Images/IMG-2.jpg", &noisy(25_000, 40), 1);
    phone.sync(&[entry], true);
    let db = phone.store.db();

    assert!(db.folder_entries_under("").unwrap().is_empty(), "{:?}", db.folder_entries_under("").unwrap());
    assert!(db.folder_search("IMG", 10).unwrap().is_empty());
    assert_eq!(db.live_bytes().unwrap(), 0);
    assert!(db.undelivered().unwrap().is_empty());
    assert!(db.only_here().unwrap().is_empty());
}

/// What Settings says about the backup: how much, how much the computer has,
/// and how much is kept there only.
#[test]
fn the_backup_is_counted() {
    let mut phone = Phone::new();
    let held = noisy(10_000, 50);
    let a = phone.write("Media/held.jpg", &held, 1);
    let b = phone.write("Media/not-yet.jpg", &noisy(20_000, 51), 1);
    phone.sync(&[a, b.clone()], true);
    phone.store.note_replica(&blake3::hash(&held), &computer()).unwrap();
    std::fs::remove_file(phone.outside().join("Media/held.jpg")).unwrap();
    phone.sync(&[b], true);

    let summary = phone.store.db().outside_summary(PREFIX).unwrap();
    assert_eq!((summary.files, summary.bytes), (2, 30_000));
    assert_eq!((summary.held_files, summary.held_bytes), (1, 10_000));
    assert_eq!(summary.only_kept, 1);
}

/// Not a test: how long indexing takes per file, and whether that grows with
/// the number of files. `cargo test -p qurb-storage --test outside -- --ignored --nocapture`
#[test]
#[ignore]
fn timing_many_small_files() {
    let mut phone = Phone::new();
    let listing: Vec<OutsideEntry> =
        (0..6000).map(|i| phone.write(&format!("Media/{:05}.jpg", i), &noisy(40_000, 100 + i), 1)).collect();
    let started = std::time::Instant::now();
    let mut done = 0;
    for (n, chunk) in [1000usize, 2000, 3000, 4000, 5000, 6000].iter().enumerate() {
        let t = std::time::Instant::now();
        phone.store.keep_outside_folder(PREFIX, &listing[..*chunk], &|_| true, None, &|_| false).unwrap();
        println!("files {:5}..{:5}: {:6.2} ms per file", done, chunk, t.elapsed().as_secs_f64() * 1000.0 / (*chunk - done) as f64);
        done = *chunk;
        let _ = n;
    }
    println!("total {:?}", started.elapsed());
}
