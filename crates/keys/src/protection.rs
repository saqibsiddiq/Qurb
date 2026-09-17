//! Where the master key actually lives.
//!
//! The key itself is 32 bytes that decrypt everything a person owns. Until now
//! it sat in a file readable only by its owner, which defends against other
//! users of the machine and against nothing that can read the disk — a stolen
//! laptop, an unencrypted backup, malware running as the user.
//!
//! Three options now, which defend against different things:
//!
//! - [`Protection::File`] — owner-only permissions. What was there before, kept
//!   because a headless machine may have neither a keystore nor anyone to type
//!   a passphrase.
//! - [`Protection::Keystore`] — the operating system's own store: Keychain,
//!   the Windows Credential Manager, or the Secret Service. Encrypted at rest
//!   and, on a locked machine, unreadable.
//! - [`Protection::Passphrase`] — the key wrapped with one only the user knows.
//!   The only option that survives someone taking the disk *and* the session.
//!
//! None of them help while the daemon is running and holding the key in memory.
//! That is what it means to be a program that can decrypt your files.

use crate::error::{Error, Result};
use crate::master::MasterKey;
use std::path::Path;
use zeroize::Zeroize;

/// How this device's key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    /// A file only its owner can read.
    File,
    /// The operating system's keystore.
    Keystore,
    /// Wrapped with a passphrase.
    Passphrase,
}

impl Protection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protection::File => "file",
            Protection::Keystore => "keystore",
            Protection::Passphrase => "passphrase",
        }
    }

    /// Whether unlocking needs something typed.
    pub fn needs_passphrase(&self) -> bool {
        matches!(self, Protection::Passphrase)
    }
}

impl std::str::FromStr for Protection {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        match text {
            "file" => Ok(Protection::File),
            "keystore" => Ok(Protection::Keystore),
            "passphrase" => Ok(Protection::Passphrase),
            other => Err(Error::UnknownProtection { name: other.to_string() }),
        }
    }
}

/// What the file on disk says, whatever it holds.
pub(crate) const MAGIC: &[u8; 4] = b"QRBK";

pub(crate) const FORMAT_FILE: u8 = 1;
pub(crate) const FORMAT_KEYSTORE: u8 = 2;
pub(crate) const FORMAT_PASSPHRASE: u8 = 3;

/// What the key is filed under in the operating system's store.
const KEYSTORE_SERVICE: &str = "qurb";

/// Salt and nonce lengths for the passphrase format.
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;

/// Argon2id parameters.
///
/// 64 MiB and three passes: enough that guessing a weak passphrase costs real
/// hardware time, and little enough that unlocking on a phone does not feel
/// broken. These are baked into the format, so a file written today must still
/// open in five years — changing them means a new format version rather than an
/// edit here.
const MEMORY_KIB: u32 = 64 * 1024;
const PASSES: u32 = 3;
const LANES: u32 = 4;

// -- the operating system's keystore -----------------------------------------

/// Put the key in the OS keystore, keyed by the vault's location.
///
/// Keyed by path so that two stores on one machine — a personal one and a test
/// one — do not fight over the same entry.
pub(crate) fn keystore_put(vault: &Path, key: &MasterKey) -> Result<()> {
    let entry = keystore_entry(vault)?;
    let encoded = hex(key.as_bytes());
    let result = entry.set_password(&encoded).map_err(|e| Error::Keystore {
        detail: format!("storing the key: {e}"),
    });
    // The encoded copy is as sensitive as the key.
    let mut encoded = encoded;
    encoded.zeroize();
    result
}

pub(crate) fn keystore_get(vault: &Path) -> Result<MasterKey> {
    let entry = keystore_entry(vault)?;
    let mut encoded = entry.get_password().map_err(|e| Error::Keystore {
        detail: format!("reading the key: {e}"),
    })?;

    let bytes = unhex(&encoded).ok_or_else(|| Error::Keystore {
        detail: "the stored key is not what we wrote".into(),
    });
    encoded.zeroize();
    Ok(MasterKey::from_bytes(bytes?))
}

pub(crate) fn keystore_remove(vault: &Path) -> Result<()> {
    let entry = keystore_entry(vault)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        // Already gone is the state we wanted.
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Keystore { detail: format!("removing the key: {e}") }),
    }
}

fn keystore_entry(vault: &Path) -> Result<keyring::Entry> {
    let account = vault.to_string_lossy().to_string();
    keyring::Entry::new(KEYSTORE_SERVICE, &account)
        .map_err(|e| Error::Keystore { detail: format!("opening the keystore: {e}") })
}

/// Whether this machine has a keystore that works.
///
/// Worth checking before offering it: a headless server, or a desktop whose
/// keyring daemon is not running, has one in name only, and a device that
/// cannot unlock itself is worse than one whose key sits in a file.
pub fn keystore_available() -> bool {
    let probe = match keyring::Entry::new(KEYSTORE_SERVICE, "availability-probe") {
        Ok(entry) => entry,
        Err(_) => return false,
    };
    if probe.set_password("probe").is_err() {
        return false;
    }
    let readable = probe.get_password().is_ok();
    let _ = probe.delete_credential();
    readable
}

// -- passphrase --------------------------------------------------------------

/// Wrap the key with a passphrase.
///
/// Argon2id rather than a plain hash, because a passphrase a person can
/// remember is guessable at speed otherwise. The salt is stored beside the
/// result: it is not secret, and its job is to stop one precomputed table
/// opening everybody's key.
pub(crate) fn wrap(key: &MasterKey, passphrase: &str) -> Result<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    use rand::RngCore;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let mut wrapping = derive_wrapping_key(passphrase, &salt)?;

    let mut header = Vec::with_capacity(5 + SALT_LEN + NONCE_LEN);
    header.extend_from_slice(MAGIC);
    header.push(FORMAT_PASSPHRASE);
    header.extend_from_slice(&salt);
    header.extend_from_slice(&nonce);

    let cipher = XChaCha20Poly1305::new((&wrapping).into());
    // The header is authenticated, so the salt and nonce cannot be swapped for
    // ones from another file.
    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: key.as_bytes(), aad: &header })
        .map_err(|_| Error::Keystore { detail: "wrapping the key failed".into() })?;
    wrapping.zeroize();

    let mut out = header;
    out.extend_from_slice(&sealed);
    Ok(out)
}

pub(crate) fn unwrap(bytes: &[u8], passphrase: &str) -> Result<MasterKey> {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};

    let header_len = 5 + SALT_LEN + NONCE_LEN;
    if bytes.len() < header_len {
        return Err(Error::Keystore { detail: "the key file is truncated".into() });
    }
    let (header, sealed) = bytes.split_at(header_len);
    let salt = &header[5..5 + SALT_LEN];
    let nonce = &header[5 + SALT_LEN..];

    let mut wrapping = derive_wrapping_key(passphrase, salt)?;
    let cipher = XChaCha20Poly1305::new((&wrapping).into());
    let opened = cipher
        .decrypt(XNonce::from_slice(nonce), Payload { msg: sealed, aad: header })
        // A wrong passphrase and a damaged file are indistinguishable here, and
        // saying "wrong passphrase" is the answer that is right almost always
        // and helpful when it is.
        .map_err(|_| Error::WrongPassphrase);
    wrapping.zeroize();

    let opened = opened?;
    let bytes: [u8; 32] = opened
        .try_into()
        .map_err(|_| Error::Keystore { detail: "the unwrapped key is the wrong size".into() })?;
    Ok(MasterKey::from_bytes(bytes))
}

fn derive_wrapping_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(MEMORY_KIB, PASSES, LANES, Some(32))
        .map_err(|e| Error::Keystore { detail: format!("argon2 parameters: {e}") })?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut out = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| Error::Keystore { detail: format!("deriving from the passphrase: {e}") })?;
    Ok(out)
}

// -- odds and ends -----------------------------------------------------------

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passphrase_round_trips() {
        let key = MasterKey::generate();
        let wrapped = wrap(&key, "correct horse battery staple").unwrap();
        assert_eq!(unwrap(&wrapped, "correct horse battery staple").unwrap(), key);
    }

    #[test]
    fn the_wrong_passphrase_is_refused() {
        let wrapped = wrap(&MasterKey::generate(), "right").unwrap();
        assert!(matches!(unwrap(&wrapped, "wrong"), Err(Error::WrongPassphrase)));
        assert!(matches!(unwrap(&wrapped, ""), Err(Error::WrongPassphrase)));
    }

    #[test]
    fn the_key_does_not_appear_in_the_wrapped_file() {
        // The point of wrapping. A regression here would be silent.
        let key = MasterKey::from_bytes([0xAB; 32]);
        let wrapped = wrap(&key, "passphrase").unwrap();
        assert!(
            !wrapped.windows(32).any(|w| w == key.as_bytes()),
            "the key was written out in the clear"
        );
    }

    #[test]
    fn every_wrapping_differs_even_with_one_passphrase() {
        // A fresh salt and nonce each time, so two devices using the same
        // passphrase do not produce files that can be compared.
        let key = MasterKey::from_bytes([7; 32]);
        assert_ne!(wrap(&key, "same").unwrap(), wrap(&key, "same").unwrap());
    }

    #[test]
    fn tampering_with_the_salt_is_detected() {
        // The header is authenticated, so a salt from another file cannot be
        // substituted to make two keys derive alike.
        let mut wrapped = wrap(&MasterKey::generate(), "passphrase").unwrap();
        wrapped[6] ^= 0xFF;
        assert!(unwrap(&wrapped, "passphrase").is_err());
    }

    #[test]
    fn truncation_is_refused() {
        // A sample of cut points rather than all of them. Argon2id is
        // deliberately expensive, and every attempt pays for it -- checking
        // every offset turned this file into two minutes of work to prove
        // something the boundaries already demonstrate.
        let wrapped = wrap(&MasterKey::generate(), "passphrase").unwrap();
        let interesting = [0, 1, 4, 5, 20, 21, 44, 45, wrapped.len() / 2, wrapped.len() - 1];
        for cut in interesting {
            assert!(unwrap(&wrapped[..cut], "passphrase").is_err(), "accepted {cut} bytes");
        }
    }

    #[test]
    fn protection_names_round_trip() {
        for protection in [Protection::File, Protection::Keystore, Protection::Passphrase] {
            assert_eq!(protection.as_str().parse::<Protection>().unwrap(), protection);
        }
        assert!("nonsense".parse::<Protection>().is_err());
    }
}
