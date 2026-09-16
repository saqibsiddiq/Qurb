//! Where the root secret lives on this device.
//!
//! # The gap, stated plainly
//!
//! The key is written to a file readable only by its owner. That protects it
//! from other users on the machine and from a stolen backup that excludes it.
//! It does **not** protect it from anyone who can read the disk — malware
//! running as the user, an unencrypted drive that is stolen, a filesystem
//! backup that includes it.
//!
//! The real answer is the operating system's keystore: Keychain on macOS, DPAPI
//! or the Credential Manager on Windows, the Secret Service on Linux. Each is a
//! separate platform integration, and none is built. Until they are, this is
//! what protects the key, and the threat model should say so out loud rather
//! than implying more.

use crate::error::{Error, Result};
use crate::master::MasterKey;
use crate::phrase::RecoveryPhrase;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"QRBK";
const FORMAT: u8 = 1;
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
}

impl Vault {
    /// The vault at `<dir>/master.key`.
    pub fn at(dir: &Path) -> Self {
        Self { path: dir.join("master.key") }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Load the key, generating one on first use.
    pub fn open_or_create(&self) -> Result<Opened> {
        if self.exists() {
            return Ok(Opened::Existing(self.load()?));
        }
        let key = MasterKey::generate();
        let phrase = key.to_phrase();
        self.write(&key)?;
        Ok(Opened::Created { key, phrase })
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

    fn load(&self) -> Result<MasterKey> {
        let bytes = std::fs::read(&self.path)
            .map_err(|e| Error::Io { path: self.path.clone(), source: e })?;

        if bytes.len() != FILE_LEN || &bytes[..4] != MAGIC {
            return Err(Error::NotAKeyFile { path: self.path.clone() });
        }
        if bytes[4] != FORMAT {
            return Err(Error::UnsupportedFormat { path: self.path.clone(), found: bytes[4] });
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[5..]);
        Ok(MasterKey::from_bytes(key))
    }

    fn write(&self, key: &MasterKey) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }

        let mut bytes = Vec::with_capacity(FILE_LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.push(FORMAT);
        bytes.extend_from_slice(key.as_bytes());

        // Create with the right permissions from the start. Writing first and
        // tightening after leaves a window in which the key is world-readable,
        // and that window is enough.
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
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::at(dir.path());
        let mut bytes = MAGIC.to_vec();
        bytes.push(FORMAT + 1);
        bytes.extend_from_slice(&[0u8; 32]);
        std::fs::write(vault.path(), &bytes).unwrap();

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
