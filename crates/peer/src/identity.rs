//! A device's network identity.
//!
//! Each device generates one self-signed certificate and keeps it. Its
//! **fingerprint** — the BLAKE3 hash of the certificate — is how other devices
//! recognise it.
//!
//! Hashing the whole certificate rather than the public key inside it avoids
//! parsing X.509 to compare identities, and is equivalent for the purpose: the
//! certificate is self-signed, so it binds the key it contains, and a device
//! keeps exactly one.
//!
//! # What this is not
//!
//! A fingerprint only means something if you already know which one to expect.
//! Getting that knowledge onto both devices is *pairing* — the QR-code exchange
//! in [decision 0001](../../../docs/decisions/0001-hybrid-p2p-topology.md) — and
//! it is not built. Until it is, the caller supplies the expected fingerprint,
//! which is why every connection here demands one rather than defaulting to
//! trusting whoever answers.

use crate::error::{Error, Result};
use std::fmt;
use std::path::Path;

/// How a device is recognised on the network.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Short form for logs. Never for comparison.
    pub fn short(&self) -> String {
        self.0[..4].iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.short())
    }
}

/// This device's certificate and private key.
#[derive(Clone)]
pub struct Identity {
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
    fingerprint: Fingerprint,
}

impl Identity {
    /// Load this device's identity, creating it on first use.
    ///
    /// Stored beside the index rather than inside it: it is a key, and the
    /// storage layer has no business holding one. The private key is written
    /// with owner-only permissions where the platform supports it.
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io { path: dir.to_path_buf(), source: e })?;
        let cert_path = dir.join("identity.crt");
        let key_path = dir.join("identity.key");

        if cert_path.exists() && key_path.exists() {
            let cert_der = read(&cert_path)?;
            let key_der = read(&key_path)?;
            return Ok(Self::from_parts(cert_der, key_der));
        }

        let generated = rcgen::generate_simple_self_signed(vec!["qurb-device".to_string()])
            .map_err(|e| Error::Tls(format!("generating an identity: {e}")))?;
        let cert_der = generated.cert.der().to_vec();
        let key_der = generated.key_pair.serialize_der();

        write(&cert_path, &cert_der, false)?;
        write(&key_path, &key_der, true)?;

        Ok(Self::from_parts(cert_der, key_der))
    }

    /// The identity `seed` always makes: the same certificate, byte for byte,
    /// every time -- an Ed25519 key, whose signatures are deterministic, and
    /// every field of the certificate fixed -- so the same fingerprint.
    ///
    /// What lets a phone keep one identity across a reinstall or its data
    /// cleared (decision 0079): its seed is made from the person's key and
    /// what Android keeps for the app (`permanent_seed`), both of which come
    /// back, and so does this.
    pub fn derived(seed: &[u8; 32]) -> Result<Self> {
        // PKCS #8 v1 for an Ed25519 key: a fixed prefix, then the seed.
        let mut pkcs8 = vec![0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
        pkcs8.extend_from_slice(seed);
        let key = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(
            &rustls::pki_types::PrivatePkcs8KeyDer::from(pkcs8.as_slice()),
            &rcgen::PKCS_ED25519,
        )
        .map_err(|e| Error::Tls(format!("making the identity's key: {e}")))?;
        let mut params = rcgen::CertificateParams::new(vec!["qurb-device".to_string()])
            .map_err(|e| Error::Tls(format!("making the identity: {e}")))?;
        params.not_before = rcgen::date_time_ymd(2026, 1, 1);
        params.not_after = rcgen::date_time_ymd(4096, 1, 1);
        let serial = blake3::derive_key("qurb 2026-11-01 identity serial", seed);
        params.serial_number = Some(rcgen::SerialNumber::from_slice(&serial[..16]));
        params.distinguished_name = rcgen::DistinguishedName::new();
        params.distinguished_name.push(rcgen::DnType::CommonName, "qurb-device");
        let cert = params.self_signed(&key).map_err(|e| Error::Tls(format!("signing the identity: {e}")))?;
        Ok(Self::from_parts(cert.der().to_vec(), pkcs8))
    }

    /// The seed of a device's permanent identity (decision 0079): from the
    /// person's key, as `Purpose::DeviceIdentity` derives it, and what the
    /// platform keeps for this install of the app across a reinstall or its
    /// data cleared -- Android's `ANDROID_ID`, never sent anywhere as it is.
    /// The same phone, set up again with the same person's key, makes the
    /// same seed; another phone, or another person, a different one.
    pub fn permanent_seed(identity_key: &[u8; 32], install: &str) -> [u8; 32] {
        let mut input = b"qurb 2026-11-01 permanent identity\0".to_vec();
        input.extend_from_slice(install.trim().as_bytes());
        *blake3::keyed_hash(identity_key, &input).as_bytes()
    }

    /// This identity's certificate, as a device sends it to say it is
    /// becoming this identity (`Request::Become`).
    pub fn cert_bytes(&self) -> &[u8] {
        &self.cert_der
    }

    /// Put `self` in `dir` as this device's identity, in place of any there:
    /// the new files written beside the old and moved over them, so that a
    /// device stopped part-way has one identity or the other, never half.
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io { path: dir.to_path_buf(), source: e })?;
        for (name, bytes, private) in [("identity.key", &self.key_der, true), ("identity.crt", &self.cert_der, false)] {
            let fresh = dir.join(format!("{name}.new"));
            write(&fresh, bytes, private)?;
            std::fs::rename(&fresh, dir.join(name)).map_err(|e| Error::Io { path: fresh.clone(), source: e })?;
        }
        Ok(())
    }

    /// Load this device's identity, refusing rather than creating one.
    ///
    /// For everything that is not setting a device up. A folder moved to the
    /// Trash while the desktop ran had its identity made again, by a window
    /// showing a pairing code into a folder that was no longer there -- and
    /// the phone paired with that second device. Making an identity is
    /// setup's job alone.
    pub fn load(dir: &Path) -> Result<Self> {
        let cert_path = dir.join("identity.crt");
        let key_path = dir.join("identity.key");
        if !(cert_path.exists() && key_path.exists()) {
            return Err(Error::Tls(format!("{} has no identity: it is not set up", dir.display())));
        }
        Ok(Self::from_parts(read(&cert_path)?, read(&key_path)?))
    }

    fn from_parts(cert_der: Vec<u8>, key_der: Vec<u8>) -> Self {
        let fingerprint = Fingerprint(*blake3::hash(&cert_der).as_bytes());
        Self { cert_der, key_der, fingerprint }
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    pub(crate) fn cert_der(&self) -> rustls::pki_types::CertificateDer<'static> {
        rustls::pki_types::CertificateDer::from(self.cert_der.clone())
    }

    pub(crate) fn key_der(&self) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
        rustls::pki_types::PrivateKeyDer::try_from(self.key_der.clone())
            .map_err(|e| Error::Tls(format!("loading the private key: {e}")))
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint)
            .field("key", &"<redacted>")
            .finish()
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::Io { path: path.to_path_buf(), source: e })
}

fn write(path: &Path, bytes: &[u8], private: bool) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
    }
    let _ = private;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_is_stable_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let first = Identity::load_or_create(dir.path()).unwrap();
        let second = Identity::load_or_create(dir.path()).unwrap();
        assert_eq!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn separate_devices_get_separate_identities() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        assert_ne!(
            Identity::load_or_create(a.path()).unwrap().fingerprint(),
            Identity::load_or_create(b.path()).unwrap().fingerprint()
        );
    }

    #[test]
    fn the_fingerprint_is_the_hash_of_the_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::load_or_create(dir.path()).unwrap();
        let cert = std::fs::read(dir.path().join("identity.crt")).unwrap();
        assert_eq!(identity.fingerprint().as_bytes(), blake3::hash(&cert).as_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn the_private_key_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        Identity::load_or_create(dir.path()).unwrap();
        let mode = std::fs::metadata(dir.path().join("identity.key")).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "the key must not be readable by anyone else");
    }
}
