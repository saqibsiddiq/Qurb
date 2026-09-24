//! The questions an interface asks, and whether the answers are the ones a
//! person would act on.
//!
//! The distinction these tests exist for is availability. "Here", "not here"
//! and "here and nowhere else" look the same to a listing that only knows
//! whether a file is on disk — and a storage screen that offered to free the
//! third would be offering to delete somebody's only copy.

use qurb_cli::{Availability, View};
use qurb_storage::db::Event;
use qurb_storage::{ChunkKey, Store};
use qurb_sync::DeviceId;
use std::fs;
use std::path::PathBuf;

struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    store: Store,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([19; 32]))
            .unwrap()
            .in_tree(&root);
        Self { _dir: dir, root, store }
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        self.store.put_file(rel, &path).unwrap();
    }

    fn view(&self) -> View<'_> {
        View::new(&self.store, 0)
    }
}

#[test]
fn a_file_nobody_else_has_is_not_merely_available() {
    let mut device = Device::new();
    device.write("alone.txt", b"the only copy in the world");
    device.write("shared.txt", b"this one is elsewhere too");

    device
        .store
        .note_replica(&blake3::hash(b"this one is elsewhere too"), &DeviceId::from_bytes([4; 32]))
        .unwrap();

    let files = device.view().files(None, 100, 0).unwrap();
    let by_path = |name: &str| {
        files.iter().find(|f| f.path == name).unwrap_or_else(|| panic!("{name} missing")).availability
    };

    assert_eq!(by_path("alone.txt"), Availability::OnlyHere);
    assert_eq!(by_path("shared.txt"), Availability::Here);
}

#[test]
fn a_file_this_device_dropped_reads_as_somewhere_else() {
    let mut device = Device::new();
    device.write("big.bin", &vec![9u8; 40_000]);
    device
        .store
        .note_replica(&blake3::hash(&vec![9u8; 40_000]), &DeviceId::from_bytes([4; 32]))
        .unwrap();

    let freed = device.store.evict("big.bin").unwrap();
    assert!(freed > 0);

    let files = device.view().files(None, 100, 0).unwrap();
    assert_eq!(files[0].availability, Availability::Elsewhere);
    assert_eq!(files[0].size, 40_000, "an evicted file still knows how big it is");
}

#[test]
fn a_listing_can_be_narrowed_to_one_folder() {
    let mut device = Device::new();
    device.write("notes.txt", b"top level");
    device.write("photos/cat.png", b"a cat");
    device.write("photos/holiday/beach.jpg", b"a beach");

    let under = device.view().files(Some("photos"), 100, 0).unwrap();
    assert_eq!(
        under.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        vec!["photos/cat.png", "photos/holiday/beach.jpg"]
    );

    // And a trailing slash means the same thing, because people type both.
    assert_eq!(device.view().files(Some("photos/"), 100, 0).unwrap().len(), 2);
}

#[test]
fn a_listing_pages_in_path_order() {
    let mut device = Device::new();
    for i in 0..10 {
        device.write(&format!("file-{i:02}.txt"), b"x");
    }

    let first = device.view().files(None, 4, 0).unwrap();
    let second = device.view().files(None, 4, 4).unwrap();
    assert_eq!(first.last().unwrap().path, "file-03.txt");
    assert_eq!(second.first().unwrap().path, "file-04.txt");
    assert_eq!(device.view().storage().unwrap().file_count, 10);
}

/// What somebody typed is text, not a pattern. A search for `report_final`
/// must not match `reportXfinal`.
#[test]
fn search_treats_wildcards_as_ordinary_characters() {
    let mut device = Device::new();
    device.write("report_final.txt", b"the real one");
    device.write("reportXfinal.txt", b"not the one");
    device.write("100%.txt", b"nor this");

    let hits = device.view().search("report_final", 20).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "report_final.txt");

    let hits = device.view().search("100%", 20).unwrap();
    assert_eq!(hits.len(), 1, "a literal percent should match only itself");
}

#[test]
fn search_folds_case_for_ascii() {
    let mut device = Device::new();
    device.write("Holiday/BEACH.jpg", b"sand");

    assert_eq!(device.view().search("beach", 20).unwrap().len(), 1);
    assert_eq!(device.view().search("HOLIDAY", 20).unwrap().len(), 1);
}

#[test]
fn storage_separates_what_the_folder_costs_from_what_the_store_costs() {
    let mut device = Device::new();
    device.write("in-folder.bin", &vec![1u8; 30_000]);

    let storage = device.view().storage().unwrap();
    assert!(storage.files >= 30_000, "the file in the folder must be counted");
    assert_eq!(storage.chunks, 0, "a materialised file is not also in the chunk store");
    assert_eq!(storage.only_here, 1);
    assert_eq!(storage.evicted, 0);
    assert!(!storage.over(), "no limit means never over");
}

#[test]
fn over_the_limit_is_a_property_of_the_store_not_of_a_running_daemon() {
    let mut device = Device::new();
    device.write("big.bin", &vec![2u8; 100_000]);

    let tight = View::new(&device.store, 1_000);
    assert!(tight.storage().unwrap().over());
    assert_eq!(tight.resting_state().unwrap(), qurb_cli::State::Problem);

    let roomy = View::new(&device.store, 10_000_000);
    assert!(!roomy.storage().unwrap().over());
    assert_eq!(roomy.resting_state().unwrap(), qurb_cli::State::Starting);
}

#[test]
fn a_send_shows_up_as_outgoing_until_it_is_collected() {
    let device = Device::new();
    let phone = DeviceId::from_bytes([0x7C; 32]);

    let source = device.root.parent().unwrap().join("outgoing.bin");
    fs::write(&source, vec![3u8; 5_000]).unwrap();
    let mut store = device.store;
    store.send_to_vault("tickets.pdf", &source, &phone).unwrap();

    let view = View::new(&store, 0);
    let outgoing = view.outgoing().unwrap();
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].path, "tickets.pdf");
    assert_eq!(outgoing[0].to, phone);

    // And a send is not part of the folder listing: it is not the user's file.
    assert!(view.files(None, 100, 0).unwrap().is_empty());

    store.note_replica_in_vault(&blake3::hash(&vec![3u8; 5_000]), &phone).unwrap();
    assert!(View::new(&store, 0).outgoing().unwrap().is_empty());
}

#[test]
fn history_answers_why_a_file_is_not_here() {
    let mut device = Device::new();
    device.write("gone.bin", &vec![5u8; 20_000]);
    device
        .store
        .note_replica(&blake3::hash(&vec![5u8; 20_000]), &DeviceId::from_bytes([6; 32]))
        .unwrap();
    device.store.evict("gone.bin").unwrap();

    let history = device.view().history_of("gone.bin", 10).unwrap();
    assert_eq!(history[0].kind, Event::Evicted);
    assert!(history[0].detail.as_deref().unwrap().contains("qurb fetch"));
}

#[test]
fn devices_reports_what_pairing_recorded_and_nothing_it_could_not_know() {
    let device = Device::new();
    let phone = DeviceId::from_bytes([0xAB; 32]);
    device.store.db().trust_peer(&phone, &[0xCD; 32], "phone").unwrap();

    let listed = device.view().devices().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, phone);
    assert_eq!(listed[0].name, "phone");
    assert_eq!(listed[0].fingerprint, "cdcdcdcd");
    // Reachability is live state and deliberately absent here.
    assert_eq!(listed[0].last_seen, None);
}

/// Naming a device is how both `qurb send` and the window's send screen decide
/// where a file is going. Getting it wrong sends somebody's file to the wrong
/// person, so the two failures are distinguished rather than collapsed.
#[test]
fn a_device_can_be_named_the_way_any_screen_prints_it() {
    let device = Device::new();
    let phone = DeviceId::from_bytes([0xAB; 32]);
    device.store.db().trust_peer(&phone, &[0xCD; 32], "phone").unwrap();

    let view = device.view();
    for spelling in ["phone", "PHONE", "  phone  ", "cdcdcdcd", "CDCDCDCD", &phone.short()] {
        match view.device_named(spelling).unwrap() {
            qurb_cli::Recipient::One(found) => assert_eq!(found.id, phone, "for {spelling:?}"),
            other => panic!("{spelling:?} did not resolve: {other:?}"),
        }
    }
}

#[test]
fn a_name_nobody_has_comes_back_with_the_names_that_exist() {
    let device = Device::new();
    device.store.db().trust_peer(&DeviceId::from_bytes([1; 32]), &[1; 32], "phone").unwrap();
    device.store.db().trust_peer(&DeviceId::from_bytes([2; 32]), &[2; 32], "tablet").unwrap();

    match device.view().device_named("laptop").unwrap() {
        qurb_cli::Recipient::Unknown { known } => {
            let names: Vec<&str> = known.iter().map(|d| d.name.as_str()).collect();
            assert_eq!(names, vec!["phone", "tablet"]);
        }
        other => panic!("expected nothing to match, got {other:?}"),
    }
}

/// Two devices called "phone" is an ordinary thing to have done. The answer is
/// to ask which, not to pick one.
#[test]
fn two_devices_sharing_a_name_are_reported_rather_than_guessed_between() {
    let device = Device::new();
    device.store.db().trust_peer(&DeviceId::from_bytes([1; 32]), &[0x11; 32], "phone").unwrap();
    device.store.db().trust_peer(&DeviceId::from_bytes([2; 32]), &[0x22; 32], "phone").unwrap();

    match device.view().device_named("phone").unwrap() {
        qurb_cli::Recipient::Several(found) => {
            assert_eq!(found.len(), 2);
            // And the fingerprints are what tells them apart, which is what the
            // caller will print.
            let prints: Vec<&str> = found.iter().map(|d| d.fingerprint.as_str()).collect();
            assert_eq!(prints, vec!["11111111", "22222222"]);
        }
        other => panic!("expected an ambiguity, got {other:?}"),
    }

    // The fingerprint still resolves exactly one of them, which is the way out.
    match device.view().device_named("22222222").unwrap() {
        qurb_cli::Recipient::One(found) => assert_eq!(found.fingerprint, "22222222"),
        other => panic!("a fingerprint should be unambiguous, got {other:?}"),
    }
}

#[test]
fn a_device_with_nothing_paired_says_nothing_is_paired() {
    let device = Device::new();
    match device.view().device_named("phone").unwrap() {
        qurb_cli::Recipient::Unknown { known } => assert!(known.is_empty()),
        other => panic!("expected nothing, got {other:?}"),
    }
}
