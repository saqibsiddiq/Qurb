//! A rendezvous service you can run on a bare IP address.
//!
//! The identifiers devices announce under are bearer secrets, so a connection
//! across anybody else's network has to be encrypted. That means `wss://`,
//! which normally means a certificate, which normally means a domain name and
//! a certificate authority.
//!
//! Buying a domain to run a rendezvous is a requirement this product should not
//! impose. So the service can present a self-signed certificate and the client
//! checks it against a fingerprint given in advance — exactly how every other
//! identity in qurb is checked, and for the same reason: an authority adds a
//! third party to a decision two devices can make between themselves. See
//! [decision 0011](../../../docs/decisions/0011-peer-identity-pinning.md).
//!
//! # Where the fingerprint travels
//!
//! In the URL, after a `#`:
//!
//! ```text
//! wss://203.0.113.5:9000#4f3a...c1
//! ```
//!
//! One string to copy from the server that printed it into the setting on each
//! device, which is the whole of the deployment. A fragment because it is not
//! sent to the server — it is a fact *about* the server, and putting it in the
//! query string would send the thing being checked to the thing being checked.
//!
//! # A certificate authority still works
//!
//! A URL with no fragment is verified against the usual public roots, so a
//! deployment that does have a domain and a real certificate needs none of
//! this. Both are supported because both are reasonable; what is not supported
//! is an unauthenticated connection across a network you do not own.

use crate::error::{Error, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A certificate this service presents, and the fingerprint devices check.
pub struct Certificate {
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    fingerprint: [u8; 32],
}

impl Certificate {
    /// Load a real certificate and its key, in PEM.
    ///
    /// For a deployment with a domain. The fingerprint is still computed and
    /// still printed, because it costs nothing and a device may as well pin a
    /// certificate that an authority also vouches for.
    pub fn load(cert: &Path, key: &Path) -> Result<Self> {
        let cert_pem = std::fs::read(cert)
            .map_err(|e| Error::Tls(format!("reading {}: {e}", cert.display())))?;
        let key_pem = std::fs::read(key)
            .map_err(|e| Error::Tls(format!("reading {}: {e}", key.display())))?;

        let chain: Vec<CertificateDer<'static>> =
            rustls_pemfile::certs(&mut cert_pem.as_slice())
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| Error::Tls(format!("reading certificates: {e}")))?;
        if chain.is_empty() {
            return Err(Error::Tls(format!("{} holds no certificate", cert.display())));
        }

        let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
            .map_err(|e| Error::Tls(format!("reading the private key: {e}")))?
            .ok_or_else(|| Error::Tls(format!("{} holds no private key", key.display())))?;

        let fingerprint = fingerprint_of(&chain[0]);
        Ok(Self { chain, key, fingerprint })
    }

    /// Use the certificate in `dir`, making one if it is not there.
    ///
    /// Kept rather than regenerated on every start, because the fingerprint is
    /// what every device has been told to expect: a service that made a new
    /// certificate each time it restarted would lock out every device it had.
    ///
    /// `names` are what the certificate is issued for. They are not checked by
    /// a pinning client — the fingerprint is the identity — but they matter to
    /// anything else that might look, and to a future deployment that puts a
    /// real name in front of this one.
    pub fn kept_in(dir: &Path, names: Vec<String>) -> Result<Self> {
        let cert_path = dir.join("rendezvous.crt");
        let key_path = dir.join("rendezvous.key");

        if cert_path.exists() && key_path.exists() {
            return Self::load(&cert_path, &key_path);
        }

        std::fs::create_dir_all(dir)
            .map_err(|e| Error::Tls(format!("creating {}: {e}", dir.display())))?;

        let generated = rcgen::generate_simple_self_signed(names)
            .map_err(|e| Error::Tls(format!("generating a certificate: {e}")))?;

        write_private(&key_path, generated.key_pair.serialize_pem().as_bytes())?;
        std::fs::write(&cert_path, generated.cert.pem())
            .map_err(|e| Error::Tls(format!("writing {}: {e}", cert_path.display())))?;

        Self::load(&cert_path, &key_path)
    }

    /// What a device must be told to expect, as hex.
    pub fn fingerprint(&self) -> String {
        hex(&self.fingerprint)
    }

    /// The whole setting, ready to paste into a device.
    pub fn url_for(&self, host: &str, port: u16) -> String {
        format!("wss://{host}:{port}#{}", self.fingerprint())
    }

    pub fn acceptor(self) -> Result<tokio_rustls::TlsAcceptor> {
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(self.chain, self.key)
            .map_err(|e| Error::Tls(format!("building the server configuration: {e}")))?;
        Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
    }
}

/// Owner-only, because it is a private key.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes)
        .map_err(|e| Error::Tls(format!("writing {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| Error::Tls(format!("securing {}: {e}", path.display())))?;
    }
    Ok(())
}

/// SHA-256 of the certificate, which is what everything else calls a
/// certificate fingerprint.
///
/// Not BLAKE3, though the rest of qurb uses it and it is faster. This is the
/// one value a person may have to compare against something printed by
/// `openssl` or shown by a browser, and being able to check it with a tool
/// that already exists is worth more here than consistency.
fn fingerprint_of(cert: &CertificateDer<'_>) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(cert.as_ref());
    hasher.finalize().into()
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Split a rendezvous URL into the part to dial and the fingerprint to expect.
///
/// A URL with no fragment gets `None`, and is verified the ordinary way against
/// public roots.
pub fn split_pin(url: &str) -> (&str, Option<&str>) {
    match url.split_once('#') {
        Some((address, pin)) if !pin.is_empty() => (address, Some(pin)),
        Some((address, _)) => (address, None),
        None => (url, None),
    }
}

/// Accept exactly one certificate, by fingerprint, and nothing else.
#[derive(Debug)]
struct Pinned {
    expected: [u8; 32],
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl Pinned {
    fn check(&self, offered: &CertificateDer<'_>) -> std::result::Result<(), rustls::Error> {
        // Constant-time, because a fingerprint comparison that returns early
        // leaks where it stopped matching. The value is public, so this is
        // belt and braces rather than a defence anything depends on — and it
        // is one line.
        let found = fingerprint_of(offered);
        let same = found.iter().zip(self.expected.iter()).fold(0u8, |a, (x, y)| a | (x ^ y));
        match same == 0 {
            true => Ok(()),
            false => Err(rustls::Error::General(format!(
                "the rendezvous service offered a certificate with fingerprint {}, \
                 and this device was told to expect {}",
                hex(&found),
                hex(&self.expected)
            ))),
        }
    }
}

impl ServerCertVerifier for Pinned {
    /// No chain, no name, no expiry. The fingerprint is the identity.
    ///
    /// Each of those checks exists to answer "is this the server I meant", and
    /// the fingerprint answers it directly. A self-signed certificate has no
    /// chain to build, its name is whatever it was generated with, and its
    /// expiry would lock out every device on a date nobody chose.
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        self.check(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    /// Verified properly, and this is the half that matters.
    ///
    /// Pinning the certificate alone would accept anybody who could replay a
    /// copy of it, which is public. Checking the handshake signature is what
    /// proves the other end holds the matching private key.
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// A client configuration that trusts exactly one certificate.
pub fn pinned_to(fingerprint: &str) -> Result<rustls::ClientConfig> {
    let expected = parse_fingerprint(fingerprint)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut config = rustls::ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Tls(format!("building the client configuration: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { expected, provider }))
        .with_no_client_auth();

    // Nothing here speaks anything else, and leaving it unset makes the
    // handshake advertise protocols this service does not serve.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

/// Hex, with or without the colons people paste from other tools.
fn parse_fingerprint(text: &str) -> Result<[u8; 32]> {
    let cleaned: String =
        text.chars().filter(|c| !matches!(c, ':' | ' ' | '-')).flat_map(|c| c.to_lowercase()).collect();

    if cleaned.len() != 64 {
        return Err(Error::Tls(format!(
            "a certificate fingerprint is 64 hex characters; this one has {}",
            cleaned.len()
        )));
    }

    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16)
            .map_err(|_| Error::Tls("a certificate fingerprint must be hex".into()))?;
    }
    Ok(out)
}

/// Where the state directory is, for a service that has to keep its
/// certificate across restarts.
///
/// `STATE_DIRECTORY` is what systemd sets, so the packaged unit needs no path
/// of its own; the fallback is for running it by hand.
pub fn default_state_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("STATE_DIRECTORY") {
        return PathBuf::from(dir);
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".config/qurb/rendezvous"),
        None => PathBuf::from("qurb-rendezvous"),
    }
}
