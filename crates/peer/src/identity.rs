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
