//! Keeping the master key somewhere better than a file.
//!
//! On a phone the keystore is Keychain or the Android Keystore, and neither is
//! reachable from Rust — the app implements [`KeyStore`] and passes it in.
//! These tests stand in for the app, which means they check the *contract*:
//! that the key goes where it was told, comes back, and that opening without
//! the keystore fails in a way an app can act on.
//!
//! What they cannot check is whether Keychain and the Android Keystore behave
//! as documented. That needs a device and an app.

use qurb_mobile::{create_protected, protection_of, KeyStore, Qurb, QurbError, Settings};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A keystore that remembers, like the real ones, and counts, unlike them.
#[derive(Default)]
struct FakeKeyStore {
    items: Mutex<HashMap<String, Vec<u8>>>,
    puts: Mutex<usize>,
    gets: Mutex<usize>,
}

impl FakeKeyStore {
    fn stored(&self) -> usize {
        self.items.lock().unwrap().len()
    }
    fn only_value(&self) -> Vec<u8> {
        self.items.lock().unwrap().values().next().cloned().expect("something stored")
    }
    fn only_label(&self) -> String {
        self.items.lock().unwrap().keys().next().cloned().expect("something stored")
    }
}

impl KeyStore for FakeKeyStore {
    fn put(&self, label: String, secret: Vec<u8>) -> Result<(), QurbError> {
        *self.puts.lock().unwrap() += 1;
        self.items.lock().unwrap().insert(label, secret);
        Ok(())
    }
    fn get(&self, label: String) -> Result<Option<Vec<u8>>, QurbError> {
        *self.gets.lock().unwrap() += 1;
        Ok(self.items.lock().unwrap().get(&label).cloned())
    }
    fn remove(&self, label: String) -> Result<(), QurbError> {
        self.items.lock().unwrap().remove(&label);
        Ok(())
    }
}

/// A keystore that refuses everything, as a locked or broken one would.
struct BrokenKeyStore;

impl KeyStore for BrokenKeyStore {
    fn put(&self, _: String, _: Vec<u8>) -> Result<(), QurbError> {
        Err(QurbError::Other { detail: "the keystore is unavailable".into() })
    }
    fn get(&self, _: String) -> Result<Option<Vec<u8>>, QurbError> {
        Err(QurbError::Other { detail: "the keystore is unavailable".into() })
    }
    fn remove(&self, _: String) -> Result<(), QurbError> {
        Ok(())
    }
}

/// The key goes to the platform, and the file left behind does not contain it.
#[test]
fn the_key_goes_to_the_keystore_and_not_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    let keystore = Arc::new(FakeKeyStore::default());

    let setup = create_protected(root.clone(), Some(keystore.clone())).unwrap();
    assert_eq!(setup.recovery_phrase.split_whitespace().count(), 24);

    assert_eq!(keystore.stored(), 1, "nothing reached the keystore");
    assert_eq!(keystore.only_value().len(), 32, "a master key is 32 bytes");
    assert!(keystore.only_label().starts_with("qurb:"), "{}", keystore.only_label());

    assert_eq!(protection_of(root).unwrap(), "platform");

    // The decisive check. The file must be a marker, not a key: the whole point
    // is that reading the disk is not enough.
    let vault = std::fs::read(dir.path().join(".qurb/master.key")).unwrap();
    assert_eq!(vault.len(), 5, "the vault file still holds something key-sized");
    assert!(
        !vault.windows(32).any(|w| w == keystore.only_value()),
        "the key is in the file as well as the keystore"
    );
}

/// Set up, close, reopen: the key comes back and the files are readable.
#[test]
fn a_key_from_the_keystore_opens_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    let keystore = Arc::new(FakeKeyStore::default());

    create_protected(root.clone(), Some(keystore.clone())).unwrap();

    {
        let qurb =
            Qurb::open_protected(root.clone(), keystore.clone(), Settings::default()).unwrap();
        std::fs::write(dir.path().join("note.txt"), b"kept behind the keystore").unwrap();
        qurb.scan().unwrap();
    }

    // A second open, as a relaunched app would do.
    let qurb = Qurb::open_protected(root, keystore.clone(), Settings::default()).unwrap();
    assert_eq!(qurb.list().unwrap().len(), 1);

    let out = dir.path().join("out.txt");
    qurb.export("note.txt".into(), out.display().to_string()).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"kept behind the keystore");

    assert!(*keystore.gets.lock().unwrap() >= 2, "the keystore was not consulted on reopen");
}

/// Opening without the keystore must fail, and say why.
///
/// The easy mistake — calling `open` where the app meant `open_protected` —
/// must not look like a corrupt store or a wrong passphrase, because the fix is
/// entirely different.
#[test]
fn opening_without_the_keystore_says_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();
    let keystore = Arc::new(FakeKeyStore::default());

    create_protected(root.clone(), Some(keystore)).unwrap();

    match Qurb::open(root, None) {
        Err(QurbError::Locked { detail }) => {
            assert!(
                detail.contains("keystore"),
                "the message must name the keystore, not just fail: {detail}"
            );
        }
        Err(other) => panic!("expected Locked, got {other:?}"),
        Ok(_) => panic!("it opened without the keystore the key is in"),
    }
}

/// A keystore that refuses must surface as an error, not a panic across the
/// FFI boundary — which on some platforms is undefined behaviour.
#[test]
fn a_broken_keystore_is_an_error_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();

    let err = create_protected(root, Some(Arc::new(BrokenKeyStore))).unwrap_err();
    assert!(matches!(err, QurbError::Locked { .. } | QurbError::Other { .. }), "{err:?}");
}

/// Two stores on one device must not share a slot, or setting up the second
/// would silently overwrite the first's key and orphan everything it holds.
#[test]
fn two_stores_get_separate_slots() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let keystore = Arc::new(FakeKeyStore::default());

    create_protected(first.path().display().to_string(), Some(keystore.clone())).unwrap();
    create_protected(second.path().display().to_string(), Some(keystore.clone())).unwrap();

    assert_eq!(keystore.stored(), 2, "the second setup overwrote the first's key");
}

/// Without a keystore the behaviour is unchanged: a key in an owner-only file.
#[test]
fn no_keystore_still_works_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().display().to_string();

    create_protected(root.clone(), None).unwrap();
    assert_eq!(protection_of(root.clone()).unwrap(), "file");
    assert!(Qurb::open(root, None).is_ok());
}
