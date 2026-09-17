//! Where the root secret lives on this device.
//!
//! # Three ways to keep it, and what each defends against
//!
//! - [`Protection::File`] — owner-only permissions. Defends against other users
//!   of the machine and a backup that excludes it. Does **not** defend against
//!   anything that can read the disk.
//! - [`Protection::Keystore`] — the operating system's own store. Encrypted at
//!   rest, and on a locked machine unreadable.
//! - [`Protection::Passphrase`] — wrapped with something only the user knows.
//!   The only option that survives someone taking the disk *and* the session,
//!   and the only one that cannot start unattended.
//!
//! The file remains the default because a headless machine may have neither a
//! keystore nor anybody to type a passphrase, and a device that cannot unlock
//! itself is worse than one whose key sits in a file. `qurb key protect`
//! changes it.
//!
//! None of them help while the daemon is running and holding the key in memory.
//! That is what it means to be a program that can decrypt your files.

use crate::error::{Error, Result};
use crate::master::MasterKey;
use crate::phrase::RecoveryPhrase;
use crate::protection::{
    self, Protection, SecretStore, FORMAT_FILE, FORMAT_KEYSTORE, FORMAT_PASSPHRASE, FORMAT_PLATFORM,
    MAGIC,
};
use std::path::{Path, PathBuf};

const FILE_LEN: usize = 4 + 1 + 32;

/// The result of opening a vault.
///
/// Two cases rather than one, because they are not the same event. A newly
/// created key comes with a phrase that **cannot be produced again later** —
/// the type makes the caller confront that at the moment it is true, instead of
/// offering a `phrase()` method that would imply it can be asked for any time.
pub enum Opened {
    /// A key that already existed.
    Existing(MasterKey),
    /// A key created just now. Show the phrase to the user before going on.
    Created { key: MasterKey, phrase: RecoveryPhrase },
}

impl Opened {
    pub fn key(&self) -> &MasterKey {
        match self {
            Opened::Existing(key) => key,
            Opened::Created { key, .. } => key,
        }
    }

    pub fn is_new(&self) -> bool {
        matches!(self, Opened::Created { .. })
    }
}

pub struct Vault {
    path: PathBuf,
    /// Supplied by the caller, for [`Protection::Platform`].
    ///
    /// Absent on a desktop, where the keystore is reachable directly. Present
    /// on a phone, where it is the app's implementation of Keychain or the
    /// Android Keystore.
    store: Option<std::sync::Arc<dyn SecretStore>>,
}

impl Vault {
    /// The vault at `<dir>/master.key`.
    pub fn at(dir: &Path) -> Self {
        Self { path: dir.join("master.key"), store: None }
    }

    /// The same vault, able to use a platform keystore.
    ///
    /// Required before [`Protection::Platform`] will work, and harmless
    /// otherwise: a vault kept any other way ignores it.
    pub fn using(mut self, store: std::sync::Arc<dyn SecretStore>) -> Self {
        self.store = Some(store);
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Load the key, generating one on first use.
    pub fn open_or_create(&self) -> Result<Opened> {
        self.open_or_create_with(Protection::File, None)
    }

    /// Load or create, keeping the key the way `protection` says.
    ///
    /// The passphrase is only consulted when one is called for, and is required
    /// then — a vault that silently fell back to an unprotected file because
    /// nobody supplied one would be worse than refusing.
    pub fn open_or_create_with(
        &self,
        protection: Protection,
        passphrase: Option<&str>,
    ) -> Result<Opened> {
        if self.exists() {
            return Ok(Opened::Existing(self.unlock(passphrase)?));
        }
        let key = MasterKey::generate();
        let phrase = key.to_phrase();
        self.write_with(&key, protection, passphrase)?;
        Ok(Opened::Created { key, phrase })
    }

    /// How this vault's key is kept.
    pub fn protection(&self) -> Result<Protection> {
        let bytes = std::fs::read(&self.path)
            .map_err(|e| Error::Io { path: self.path.clone(), source: e })?;
        if bytes.len() < 5 || &bytes[..4] != MAGIC {
            return Err(Error::NotAKeyFile { path: self.path.clone() });
        }
        match bytes[4] {
            FORMAT_FILE => Ok(Protection::File),
            FORMAT_KEYSTORE => Ok(Protection::Keystore),
            FORMAT_PASSPHRASE => Ok(Protection::Passphrase),
            FORMAT_PLATFORM => Ok(Protection::Platform),
            found => Err(Error::UnsupportedFormat { path: self.path.clone(), found }),
        }
    }

    /// Open the key, supplying a passphrase if this vault needs one.
    pub fn unlock(&self, passphrase: Option<&str>) -> Result<MasterKey> {
        match self.protection()? {
            Protection::File => self.load(),
            Protection::Keystore => protection::keystore_get(&self.path),
            Protection::Passphrase => {
                let passphrase = passphrase.ok_or(Error::PassphraseRequired)?;
                let bytes = std::fs::read(&self.path)
                    .map_err(|e| Error::Io { path: self.path.clone(), source: e })?;
                protection::unwrap(&bytes, passphrase)
            }
            Protection::Platform => protection::platform_get(self.store()?, &self.path),
        }
    }

    /// The platform store, or a message saying which call was missing.
    ///
    /// A vault kept this way is unreadable without it, and the mistake -- opening
    /// with `Vault::at` where the app meant `Vault::at(..).using(..)` -- is easy
    /// and produces a key that cannot be found rather than one that is wrong.
    fn store(&self) -> Result<&dyn SecretStore> {
        self.store.as_deref().ok_or_else(|| Error::Keystore {
            detail: "this vault is kept in the platform keystore, but none was supplied \
                     -- open it with `Vault::at(dir).using(store)`"
                .into(),
        })
    }

    /// Change how the key is kept, without changing the key.
    ///
    /// The key is read out first and written back the new way, so the data it
    /// protects is untouched — this changes the lock, not the contents. The old
    /// copy is removed only once the new one is in place, because a device that
    /// loses its key in the middle of being made safer has been made
    /// catastrophically less safe.
    pub fn protect(
        &self,
        to: Protection,
        current_passphrase: Option<&str>,
        new_passphrase: Option<&str>,
    ) -> Result<()> {
        let from = self.protection()?;
        let key = self.unlock(current_passphrase)?;

        self.write_with(&key, to, new_passphrase)?;

        // Only now that the key is safely somewhere else.
        if from == Protection::Keystore && to != Protection::Keystore {
            protection::keystore_remove(&self.path)?;
        }
        if from == Protection::Platform && to != Protection::Platform {
            protection::platform_remove(self.store()?, &self.path)?;
        }
        Ok(())
    }

    /// Install a key recovered from its phrase, setting up a replacement device.
    ///
    /// Refuses to overwrite an existing key. Doing so would orphan every chunk
    /// already stored here — they would still be on disk, encrypted under a key
    /// that no longer exists anywhere. Deleting the file is a deliberate act and
    /// should stay one.
    pub fn restore(&self, phrase: &RecoveryPhrase) -> Result<MasterKey> {
        if self.exists() {
            return Err(Error::Io {
                path: self.path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "a key is already installed; remove it deliberately to replace it",
                ),
            });
        }
        let key = MasterKey::from_phrase(phrase)?;
        self.write(&key)?;
        Ok(key)
    }

    /// Install a recovered key, kept the way `protection` says.
    pub fn restore_with(
        &self,
        phrase: &RecoveryPhrase,
        protection: Protection,
        passphrase: Option<&str>,
    ) -> Result<MasterKey> {
        if self.exists() {
            return Err(Error::Io {
                path: self.path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "a key is already installed; remove it deliberately to replace it",
                ),
            });
        }
        let key = MasterKey::from_phrase(phrase)?;
        self.write_with(&key, protection, passphrase)?;
        Ok(key)
    }

    fn load(&self) -> Result<MasterKey> {
        let bytes = std::fs::read(&self.path)
            .map_err(|e| Error::Io { path: self.path.clone(), source: e })?;

        if bytes.len() != FILE_LEN || &bytes[..4] != MAGIC {
            return Err(Error::NotAKeyFile { path: self.path.clone() });
        }
        if bytes[4] != FORMAT_FILE {
            return Err(Error::UnsupportedFormat { path: self.path.clone(), found: bytes[4] });
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[5..]);
        Ok(MasterKey::from_bytes(key))
    }

    fn write(&self, key: &MasterKey) -> Result<()> {
        self.write_with(key, Protection::File, None)
    }

    fn write_with(
        &self,
        key: &MasterKey,
        protection: Protection,
        passphrase: Option<&str>,
    ) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }

        let bytes = match protection {
            Protection::File => {
                let mut bytes = Vec::with_capacity(FILE_LEN);
                bytes.extend_from_slice(MAGIC);
                bytes.push(FORMAT_FILE);
                bytes.extend_from_slice(key.as_bytes());
                bytes
            }
            Protection::Keystore => {
                // Written first: a marker file with no key in it is harmless,
                // whereas a key in the keystore that nothing points at is
                // litter nobody will ever clean up.
                protection::keystore_put(&self.path, key)?;
                let mut bytes = Vec::with_capacity(5);
                bytes.extend_from_slice(MAGIC);
                bytes.push(FORMAT_KEYSTORE);
                bytes
            }
            Protection::Passphrase => {
                let passphrase = passphrase.ok_or(Error::PassphraseRequired)?;
                protection::wrap(key, passphrase)?
            }
            Protection::Platform => {
                // Stored first, for the same reason as the desktop keystore: a
                // marker file with no key behind it is a clear failure, while a
                // key in a keystore that nothing points at is litter nobody
                // will ever find to clean up.
                protection::platform_put(self.store()?, &self.path, key)?;
                let mut bytes = Vec::with_capacity(5);
                bytes.extend_from_slice(MAGIC);
                bytes.push(FORMAT_PLATFORM);
                bytes
            }
        };

        // Replaced rather than appended to, and still owner-only: the file is a
        // marker under keystore protection, but under the others it is the key.
        let _ = std::fs::remove_file(&self.path);
        write_private(&self.path, &bytes)?;
        Ok(())
    }

}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
    file.write_all(bytes).map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
    file.sync_all().map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    // Windows inherits the directory's ACL. Restricting it properly needs the
    // platform keystore, which is the real fix and is not built.
    std::fs::write(path, bytes).map_err(|e| Error::Io { path: path.to_path_buf(), source: e })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master::Purpose;

    #[test]
    fn a_key_is_created_once_and_loaded_after() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::at(dir.path());

        let first = vault.open_or_create().unwrap();
        assert!(first.is_new());
        let created = first.key().clone();

        let second = vault.open_or_create().unwrap();
        assert!(!second.is_new(), "the second open must not create a new key");
        assert_eq!(second.key(), &created);
    }

    #[test]
    fn the_phrase_from_creation_rebuilds_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let Opened::Created { key, phrase } = Vault::at(dir.path()).open_or_create().unwrap()
        else {
            panic!("expected a new key");
        };
        assert_eq!(MasterKey::from_phrase(&phrase).unwrap(), key);
    }

    #[test]
    fn restoring_onto_a_fresh_device_gives_the_same_derived_keys() {
        // What "recovery" has to mean: the new device can decrypt what the old
        // one wrote. Equality of the master key is not the point -- equality of
        // what it derives is.
        let old = tempfile::tempdir().unwrap();
        let Opened::Created { key, phrase } = Vault::at(old.path()).open_or_create().unwrap()
        else {
            panic!("expected a new key");
        };

        let new = tempfile::tempdir().unwrap();
        let restored = Vault::at(new.path()).restore(&phrase).unwrap();

        assert_eq!(
            restored.derive(Purpose::ChunkEncryption),
            key.derive(Purpose::ChunkEncryption)
        );
    }

    #[test]
    fn restore_refuses_to_overwrite() {
        // Overwriting would orphan every chunk already stored: still on disk,
        // encrypted under a key that no longer exists anywhere.
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::at(dir.path());
        vault.open_or_create().unwrap();

        let other = MasterKey::generate().to_phrase();
        assert!(vault.restore(&other).is_err());
    }

    #[test]
    fn a_corrupt_key_file_is_reported_not_guessed() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::at(dir.path());
        std::fs::write(vault.path(), b"this is not a key file").unwrap();
        assert!(matches!(vault.open_or_create(), Err(Error::NotAKeyFile { .. })));
    }

    #[test]
    fn a_newer_format_is_refused_rather_than_misread() {
        // A file written by a future version must not be guessed at. Reading a
        // key out of a layout we do not understand would produce a key that is
        // wrong, and a device that then cannot read its own data.
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::at(dir.path());
        // Anchored past the last format rather than to one of them, so adding a
        // format moves the "future" and this keeps testing what it names. It
        // was `FORMAT_PASSPHRASE + 1` and started failing the day
        // `FORMAT_PLATFORM` took that number -- which is the test working.
        let mut bytes = MAGIC.to_vec();
        bytes.push(FORMAT_PLATFORM + 1);
        bytes.extend_from_slice(&[0u8; 32]);
        std::fs::write(vault.path(), &bytes).unwrap();

        assert!(matches!(vault.protection(), Err(Error::UnsupportedFormat { .. })));
        assert!(matches!(vault.open_or_create(), Err(Error::UnsupportedFormat { .. })));
    }

    #[cfg(unix)]
    #[test]
    fn the_key_file_is_not_readable_by_anyone_else() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::at(dir.path());
        vault.open_or_create().unwrap();

        let mode = std::fs::metadata(vault.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "the master key must not be readable by others");
    }
}
