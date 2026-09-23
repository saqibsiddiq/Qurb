//! Bringing a device into existence, and the phrase that is its key.
//!
//! These run against the same functions the window and the terminal both call,
//! which is the point of there being only one set: a device created by one
//! route and missing something the other writes is a difference that shows up
//! much later, on the device that was set up the unusual way.

use qurb_keys::{Purpose, RecoveryPhrase, Vault};
use qurb_storage::{ChunkKey, Store};
use std::path::Path;
use std::sync::Once;

/// Keep the list of known folders out of the real one.
///
/// `setup::create` records the folder so that later commands need no path, and
/// that list lives under `XDG_CONFIG_HOME`. A test that wrote to the actual
/// one would leave temporary directories in somebody's application for good —
/// which has happened, and is why this exists.
///
/// Set once for the whole binary, before any test touches anything, because
/// tests share a process and an environment.
fn isolated() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let scratch = std::env::temp_dir().join(format!("qurb-setup-tests-{}", std::process::id()));
        std::fs::create_dir_all(&scratch).expect("scratch config directory");
        std::env::set_var("XDG_CONFIG_HOME", &scratch);
    });
}

fn scratch() -> tempfile::TempDir {
    isolated();
    tempfile::tempdir().unwrap()
}

/// The chunk key a folder's stored master key derives, which is the thing two
/// devices must agree on for either to read the other's content.
fn chunk_key(root: &Path) -> [u8; 32] {
    let key = Vault::at(&root.join(".qurb")).unlock(None).unwrap();
    key.derive(Purpose::ChunkEncryption).to_bytes()
}

#[test]
fn a_created_device_is_usable_immediately() {
    let dir = scratch();
    let root = dir.path().join("sync");

    let phrase = qurb_cli::setup::create(&root).unwrap();
    assert_eq!(phrase.words().len(), 24);
    assert!(qurb_cli::is_set_up(&root));

    // Everything a device has, not just a key: an identity to be recognised by,
    // a config to be found by, and an index to put files in.
    assert!(root.join(".qurb/identity.key").exists(), "no identity");
    assert!(root.join(".qurb/config").exists(), "no config");
    assert!(root.join(".qurb/index.db").exists(), "no index");

    // And the index really opens with the key the vault kept.
    let mut store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes(chunk_key(&root)))
        .unwrap()
        .in_tree(&root);
    std::fs::write(root.join("hello.txt"), b"it works").unwrap();
    store.put_file("hello.txt", &root.join("hello.txt")).unwrap();
    assert_eq!(store.list().unwrap(), vec!["hello.txt"]);
}

/// The whole promise of the 24 words: a second device with the same phrase can
/// read the first one's content.
#[test]
fn the_phrase_makes_a_second_device_the_same_device() {
    let dir = scratch();
    let first = dir.path().join("one");
    let second = dir.path().join("two");

    let phrase = qurb_cli::setup::create(&first).unwrap();
    qurb_cli::setup::enrol(&second, &phrase).unwrap();

    assert_eq!(chunk_key(&first), chunk_key(&second), "the two cannot read each other");
}

/// Written down on paper and typed back in, which is the only form it is ever
/// in when it matters.
#[test]
fn a_phrase_survives_being_written_out_and_read_back() {
    let dir = scratch();
    let first = dir.path().join("one");
    let second = dir.path().join("two");

    let phrase = qurb_cli::setup::create(&first).unwrap();
    let on_paper = phrase.words().join(" ");
    let retyped = RecoveryPhrase::parse(&on_paper).unwrap();

    qurb_cli::setup::enrol(&second, &retyped).unwrap();
    assert_eq!(chunk_key(&first), chunk_key(&second));
}

/// It was never stored, so it can only be derived. That it comes back the same
/// is what makes showing it again honest rather than a second phrase.
#[test]
fn the_phrase_can_be_derived_again_from_the_stored_key() {
    let dir = scratch();
    let root = dir.path().join("sync");

    let phrase = qurb_cli::setup::create(&root).unwrap();
    let key = Vault::at(&root.join(".qurb")).unlock(None).unwrap();

    assert_eq!(qurb_cli::setup::reveal(&key).words(), phrase.words());
}

/// Two devices made separately are *not* the same device, however similar their
/// folders look. Worth asserting, because a bug that made every device share a
/// key would look like everything working.
#[test]
fn two_devices_created_separately_have_different_keys() {
    let dir = scratch();
    let a = dir.path().join("a");
    let b = dir.path().join("b");

    qurb_cli::setup::create(&a).unwrap();
    qurb_cli::setup::create(&b).unwrap();

    assert_ne!(chunk_key(&a), chunk_key(&b));
}

#[test]
fn creating_over_an_existing_device_is_refused() {
    let dir = scratch();
    let root = dir.path().join("sync");

    qurb_cli::setup::create(&root).unwrap();
    let before = chunk_key(&root);

    assert!(qurb_cli::setup::create(&root).is_err(), "a second key was created over the first");
    assert_eq!(chunk_key(&root), before, "the existing key was disturbed");
}

#[test]
fn a_folder_that_does_not_exist_yet_can_still_be_described() {
    let dir = scratch();
    let root = dir.path().join("not/here/yet");

    let looked = qurb_cli::setup::inspect(&root);
    assert!(!looked.exists);
    assert!(!looked.set_up);
    // Judged by the nearest ancestor that does exist, which is what has to be
    // writable for the rest to be creatable.
    assert!(looked.writable);
    assert!(looked.disk > 0, "a filesystem should have a size");
}

#[test]
fn a_folder_with_things_in_it_says_how_many() {
    let dir = scratch();
    let root = dir.path().join("sync");
    std::fs::create_dir_all(&root).unwrap();
    for i in 0..5 {
        std::fs::write(root.join(format!("file-{i}.txt")), b"x").unwrap();
    }

    let looked = qurb_cli::setup::inspect(&root);
    assert!(looked.exists);
    assert!(!looked.empty);
    assert_eq!(looked.existing_files, 5);
}

/// The store is qurb's own and is not one of the person's files. Counting it
/// would tell somebody reopening their own folder that it has one thing in it.
#[test]
fn the_store_does_not_count_as_something_already_there() {
    let dir = scratch();
    let root = dir.path().join("sync");

    qurb_cli::setup::create(&root).unwrap();
    let looked = qurb_cli::setup::inspect(&root);

    assert!(looked.set_up);
    assert!(looked.empty, "the store counted as a file: {:?}", looked);
}

#[test]
fn somewhere_unwritable_says_so() {
    isolated();
    let looked = qurb_cli::setup::inspect(Path::new("/proc/sys/kernel/qurb"));
    assert!(!looked.writable);
}
