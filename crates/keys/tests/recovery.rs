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

// -- how the key is kept -----------------------------------------------------

use qurb_keys::Protection;

/// Set up a device whose key is kept a particular way, and store a file.
fn device_with(protection: Protection, passphrase: Option<&str>) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let Opened::Created { key, phrase } =
        Vault::at(dir.path()).open_or_create_with(protection, passphrase).unwrap()
    else {
        panic!("expected a fresh key");
    };

    let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
    let mut store = Store::open(&dir.path().join("store"), chunk_key).unwrap();
    store.put_bytes("kept.txt", b"the thing being protected", 0).unwrap();

    (dir, phrase.to_string())
}

fn can_read(dir: &std::path::Path, key: &MasterKey) -> bool {
    let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
    match Store::open(&dir.join("store"), chunk_key) {
        Ok(store) => store.read_file("kept.txt").is_ok(),
        Err(_) => false,
    }
}

#[test]
fn a_passphrase_protected_key_opens_the_data() {
    let (dir, _) = device_with(Protection::Passphrase, Some("a long passphrase"));

    let vault = Vault::at(dir.path());
    assert_eq!(vault.protection().unwrap(), Protection::Passphrase);

    let key = vault.unlock(Some("a long passphrase")).unwrap();
    assert!(can_read(dir.path(), &key), "the right passphrase did not open the data");
}

#[test]
fn the_key_is_not_in_the_file_when_a_passphrase_protects_it() {
    // The whole point. A stolen disk must not carry the key on it.
    let dir = tempfile::tempdir().unwrap();
    let Opened::Created { key, .. } =
        Vault::at(dir.path()).open_or_create_with(Protection::Passphrase, Some("secret")).unwrap()
    else {
        panic!("expected a fresh key");
    };

    let on_disk = std::fs::read(Vault::at(dir.path()).path()).unwrap();
    assert!(
        !on_disk.windows(32).any(|w| w == key.derive(Purpose::ChunkEncryption).as_bytes()),
        "a derived key was written out in the clear"
    );
}

#[test]
fn a_passphrase_protected_key_will_not_open_without_one() {
    let (dir, _) = device_with(Protection::Passphrase, Some("passphrase"));
    let vault = Vault::at(dir.path());

    assert!(matches!(vault.unlock(None), Err(qurb_keys::Error::PassphraseRequired)));
    assert!(matches!(vault.unlock(Some("wrong")), Err(qurb_keys::Error::WrongPassphrase)));
}

#[test]
fn creating_a_passphrase_vault_without_one_is_refused() {
    // Falling back to an unprotected file because nobody supplied a passphrase
    // would be worse than refusing, and silent.
    let dir = tempfile::tempdir().unwrap();
    assert!(Vault::at(dir.path()).open_or_create_with(Protection::Passphrase, None).is_err());
    assert!(!Vault::at(dir.path()).exists(), "a vault was created anyway");
}

#[test]
fn changing_the_protection_keeps_the_same_key() {
    // Changing the lock, not the contents. If this were wrong the data would be
    // unreadable afterwards, which is the worst outcome an operation meant to
    // improve security could have.
    let (dir, _) = device_with(Protection::File, None);
    let before = Vault::at(dir.path()).unlock(None).unwrap();

    Vault::at(dir.path()).protect(Protection::Passphrase, None, Some("now protected")).unwrap();

    let vault = Vault::at(dir.path());
    assert_eq!(vault.protection().unwrap(), Protection::Passphrase);
    let after = vault.unlock(Some("now protected")).unwrap();

    assert_eq!(before, after, "the key changed when the lock did");
    assert!(can_read(dir.path(), &after), "the data became unreadable");
}

#[test]
fn a_passphrase_can_be_changed() {
    let (dir, _) = device_with(Protection::Passphrase, Some("first"));
    let vault = Vault::at(dir.path());
    let before = vault.unlock(Some("first")).unwrap();

    vault.protect(Protection::Passphrase, Some("first"), Some("second")).unwrap();

    assert!(matches!(vault.unlock(Some("first")), Err(qurb_keys::Error::WrongPassphrase)));
    assert_eq!(vault.unlock(Some("second")).unwrap(), before, "the key changed with the passphrase");
    assert!(can_read(dir.path(), &before));
}

#[test]
fn a_recovery_phrase_still_works_under_a_passphrase() {
    // Two different secrets protecting the same key, and neither may interfere
    // with the other: the phrase recovers the key, the passphrase guards the
    // copy on this disk.
    let (dir, phrase) = device_with(Protection::Passphrase, Some("local passphrase"));

    let elsewhere = tempfile::tempdir().unwrap();
    let recovered = Vault::at(elsewhere.path())
        .restore(&RecoveryPhrase::parse(&phrase).unwrap())
        .unwrap();

    let here = Vault::at(dir.path()).unlock(Some("local passphrase")).unwrap();
    assert_eq!(recovered, here, "the phrase did not recover the protected key");
}

/// The operating system's keystore, where one is usable.
///
/// Skipped rather than failed where there is none — a headless build machine
/// has no keyring, and a test that cannot run there is not a test that failed.
#[test]
fn a_keystore_protected_key_opens_the_data() {
    if !qurb_keys::keystore_available() {
        eprintln!("skipped: no usable keystore on this machine");
        return;
    }

    let (dir, _) = device_with(Protection::Keystore, None);
    let vault = Vault::at(dir.path());
    assert_eq!(vault.protection().unwrap(), Protection::Keystore);

    let key = vault.unlock(None).unwrap();
    assert!(can_read(dir.path(), &key), "the keystore did not give back the key");

    // And the file left behind is a marker, not the key.
    let on_disk = std::fs::read(vault.path()).unwrap();
    assert!(on_disk.len() < 32, "the key file still holds something key-sized");

    // Leave the machine's keystore as we found it.
    vault.protect(Protection::File, None, None).unwrap();
}

#[test]
fn moving_a_key_into_the_keystore_takes_it_out_of_the_file() {
    if !qurb_keys::keystore_available() {
        eprintln!("skipped: no usable keystore on this machine");
        return;
    }

    let (dir, _) = device_with(Protection::File, None);
    let vault = Vault::at(dir.path());
    let before = vault.unlock(None).unwrap();
    assert!(std::fs::read(vault.path()).unwrap().len() >= 32);

    vault.protect(Protection::Keystore, None, None).unwrap();

    assert!(std::fs::read(vault.path()).unwrap().len() < 32, "the key is still in the file");
    assert_eq!(vault.unlock(None).unwrap(), before, "the key changed");
    assert!(can_read(dir.path(), &before));

    vault.protect(Protection::File, None, None).unwrap();
}
