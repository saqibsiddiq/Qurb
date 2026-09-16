//! Does the recovery phrase actually recover anything?
//!
//! The unit tests prove a phrase rebuilds a key. That is not the same claim.
//! What a user needs is that the words on their paper turn back into *their
//! files*, through every layer between: derivation, the chunk cipher, the
//! on-disk format. This tests the whole path.
//!
//! It is the highest-stakes property in the product. There is no reset and no
//! support ticket — if this is wrong, someone's data is gone and nobody finds
//! out until it is far too late.

use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_storage::{ChunkKey, Store};

/// Everything the user would still have after losing a device: their synced
/// data, and a piece of paper.
struct Backup {
    dir: tempfile::TempDir,
}

impl Backup {
    /// Set up a device, store a file, and walk away with the phrase.
    fn create(contents: &[u8]) -> (Self, String) {
        let dir = tempfile::tempdir().unwrap();

        let Opened::Created { key, phrase } = Vault::at(dir.path()).open_or_create().unwrap()
        else {
            panic!("expected a fresh key");
        };

        let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
        let mut store = Store::open(&dir.path().join("store"), chunk_key).unwrap();
        store.put_bytes("important.txt", contents, 0).unwrap();

        (Self { dir }, phrase.to_string())
    }

    /// The device is gone. The chunks survive in a backup; the key does not.
    fn lose_the_key(&self) {
        std::fs::remove_file(Vault::at(self.dir.path()).path()).unwrap();
    }

    /// Open the surviving store with whatever key we can produce.
    fn open_with(&self, key: &MasterKey) -> Store {
        let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
        Store::open(&self.dir.path().join("store"), chunk_key).unwrap()
    }
}

#[test]
fn a_phrase_written_on_paper_brings_the_files_back() {
    let contents = b"the thing the user cares about";
    let (backup, written_down) = Backup::create(contents);

    backup.lose_the_key();

    // A new device: nothing but the backup and the words.
    let phrase = RecoveryPhrase::parse(&written_down).unwrap();
    let recovered = Vault::at(backup.dir.path()).restore(&phrase).unwrap();

    let store = backup.open_with(&recovered);
    assert_eq!(store.read_file("important.txt").unwrap(), contents);
    assert!(store.verify(true).unwrap().is_healthy(), "the store must verify under the new key");
}

#[test]
fn recovery_survives_how_people_actually_write_things_down() {
    // Copied off paper: line breaks, inconsistent case, a trailing space.
    let contents = b"content";
    let (backup, written_down) = Backup::create(contents);
    backup.lose_the_key();

    let as_transcribed = format!("  {}\n", written_down.replace(' ', "\n").to_uppercase());
    let phrase = RecoveryPhrase::parse(&as_transcribed).unwrap();
    let recovered = Vault::at(backup.dir.path()).restore(&phrase).unwrap();

    assert_eq!(backup.open_with(&recovered).read_file("important.txt").unwrap(), contents);
}

#[test]
fn the_wrong_phrase_cannot_read_the_data() {
    // The other half of the promise. If a different phrase could open the
    // store, the encryption would not be doing anything.
    let (backup, _written_down) = Backup::create(b"private");
    backup.lose_the_key();

    let someone_elses = MasterKey::generate().to_phrase();
    let wrong = Vault::at(backup.dir.path()).restore(&someone_elses).unwrap();

    let store = backup.open_with(&wrong);
    assert!(store.read_file("important.txt").is_err(), "the wrong key opened the data");

    // And the failure is detected rather than producing plausible rubbish.
    let report = store.verify(true).unwrap();
    assert!(!report.is_healthy());
    assert!(!report.corrupt.is_empty(), "chunks should fail to decrypt, not silently misread");
}

#[test]
fn a_mistyped_phrase_is_refused_before_it_can_do_harm() {
    // The dangerous version of this bug: a wrong phrase is accepted, installed,
    // and only later turns out to open nothing -- by which time the correct
    // phrase may be gone. The checksum has to catch it at the door.
    let (backup, written_down) = Backup::create(b"content");
    backup.lose_the_key();

    let mut words: Vec<&str> = written_down.split(' ').collect();
    words.swap(7, 8);
    let mistyped = words.join(" ");

    if mistyped != written_down {
        assert!(RecoveryPhrase::parse(&mistyped).is_err(), "a transposition was accepted");
    }

    let mut wrong_word: Vec<&str> = written_down.split(' ').collect();
    wrong_word[2] = "abandon";
    assert!(
        RecoveryPhrase::parse(&wrong_word.join(" ")).is_err()
            || wrong_word.join(" ") == written_down
    );
}

#[test]
fn every_purpose_survives_recovery_independently() {
    let dir = tempfile::tempdir().unwrap();
    let Opened::Created { key, phrase } = Vault::at(dir.path()).open_or_create().unwrap() else {
        panic!("expected a fresh key");
    };

    let before: Vec<_> = [Purpose::ChunkEncryption, Purpose::DeviceIdentity, Purpose::MetadataAuth]
        .iter()
        .map(|p| key.derive(*p))
        .collect();

    let parsed = RecoveryPhrase::parse(&phrase.to_string()).unwrap();
    let recovered = MasterKey::from_phrase(&parsed).unwrap();

    for (i, purpose) in
        [Purpose::ChunkEncryption, Purpose::DeviceIdentity, Purpose::MetadataAuth].iter().enumerate()
    {
        assert_eq!(recovered.derive(*purpose), before[i], "{purpose:?} did not survive recovery");
    }
}

#[test]
fn two_devices_from_one_phrase_can_read_each_others_data() {
    // What makes a user's devices a set rather than strangers: the same phrase
    // on a second device derives the same chunk key, so data written by one is
    // readable by the other.
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();

    let Opened::Created { key, phrase } = Vault::at(first.path()).open_or_create().unwrap() else {
        panic!("expected a fresh key");
    };

    let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
    let mut a = Store::open(&first.path().join("store"), chunk_key).unwrap();
    a.put_bytes("shared.txt", b"written on the first device", 0).unwrap();

    let parsed = RecoveryPhrase::parse(&phrase.to_string()).unwrap();
    let second_key = Vault::at(second.path()).restore(&parsed).unwrap();

    // Point the second device at the first's chunk store, as a sync would.
    let chunk_key = ChunkKey::from_bytes(second_key.derive(Purpose::ChunkEncryption).to_bytes());
    let b = Store::open(&first.path().join("store"), chunk_key).unwrap();
    assert_eq!(b.read_file("shared.txt").unwrap(), b"written on the first device");
}
