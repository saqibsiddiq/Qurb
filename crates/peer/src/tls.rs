//! Mutual authentication by pinned fingerprint.
//!
//! Both sides present a certificate and both sides check the other against a
//! list they were given in advance. There is no certificate authority and no
//! name checking — a device's identity *is* its certificate, so the only
//! question is whether this is the certificate we were told to expect.
//!
//! # Why the signature check matters
//!
//! Comparing fingerprints alone would prove nothing: certificates are public,
//! and anyone who has seen one can replay it. What proves the peer is genuine
//! is that it signs the handshake with the matching private key, which is why
//! the verifiers below hand the signature to the crypto provider rather than
//! waving it through. Skipping that is the standard way pinned TLS is got
//! wrong, and it fails open.

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use std::sync::Arc;

pub const ALPN: &[u8] = b"qurb/0";

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The peers a listener will accept, as a live set.
///
/// Shared rather than copied, because the guest list changes while the program
/// runs: a device paired by `qurb pair` writes to the trust store, and a
/// listener holding a snapshot from startup would refuse it until restarted.
/// "Pair once" has to mean once.
///
/// A read lock is taken per handshake, which is a handful of fingerprint
/// comparisons against a list of one person's own devices — not a hot path.
#[derive(Debug, Clone, Default)]
pub struct TrustList(Arc<std::sync::RwLock<Vec<Fingerprint>>>);

impl TrustList {
    pub fn new(allowed: Vec<Fingerprint>) -> Self {
        Self(Arc::new(std::sync::RwLock::new(allowed)))
    }

    /// Replace the set wholesale.
    ///
    /// Wholesale rather than adding one at a time because the trust store is
    /// the authority: a device forgotten there must stop being accepted here,
    /// and a set that only ever grows would keep letting it in.
    pub fn replace(&self, allowed: Vec<Fingerprint>) {
        if let Ok(mut current) = self.0.write() {
            *current = allowed;
        }
    }

    pub fn contains(&self, fingerprint: &Fingerprint) -> bool {
        self.0.read().map(|a| a.contains(fingerprint)).unwrap_or(false)
    }

    pub fn is_empty(&self) -> bool {
        self.0.read().map(|a| a.is_empty()).unwrap_or(true)
    }

    fn snapshot(&self) -> Vec<Fingerprint> {
        self.0.read().map(|a| a.clone()).unwrap_or_default()
    }
}

/// The set of peers a connection will accept, by fingerprint.
#[derive(Debug, Clone)]
struct Pinned {
    allowed: TrustList,
    provider: Arc<CryptoProvider>,
    /// Whether a rejection here is expected.
    ///
    /// Hole punching opens a handshake it *wants* to fail: the packets are the
    /// point, and it pins its own fingerprint so nothing can complete. Without
    /// this flag every punch logs `WARN rejected an unrecognised peer`, which
    /// reads exactly like a device being refused for real — and sent one
    /// investigation chasing a trust bug that did not exist while the actual
    /// failure sat two lines above in DEBUG.
    expect_rejection: bool,
}

impl Pinned {
    fn check(&self, presented: &CertificateDer<'_>) -> std::result::Result<(), rustls::Error> {
        let fingerprint = Fingerprint::from_bytes(*blake3::hash(presented.as_ref()).as_bytes());
        if self.allowed.contains(&fingerprint) {
            Ok(())
        } else {
            if self.expect_rejection {
                tracing::trace!(peer = %fingerprint.short(), "punch handshake refused, as intended");
            } else {
                // The full values, not the short form. A short form that matches
                // one in the list while the full fingerprints differ is exactly
                // the case this message has to be able to show.
                tracing::warn!(
                    peer = %fingerprint.short(),
                    presented = %hex(fingerprint.as_bytes()),
                    allowed = ?self.allowed.snapshot().iter().map(|f| hex(f.as_bytes())).collect::<Vec<_>>(),
                    "rejected an unrecognised peer"
                );
            }
            Err(rustls::Error::General("peer certificate is not pinned".into()))
        }
    }

    fn verify_signature(
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

    fn schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

impl ServerCertVerifier for Pinned {
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

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        // Both ends are configured for TLS 1.3 only, so reaching this means
        // something is not what it claims to be.
        Err(rustls::Error::General("TLS 1.2 is not accepted".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.verify_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.schemes()
    }
}

impl ClientCertVerifier for Pinned {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        self.check(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not accepted".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.verify_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.schemes()
    }

    /// A connection without a client certificate is refused outright. The
    /// server has to know who it is talking to before it serves anyone's files.
    fn client_auth_mandatory(&self) -> bool {
        true
    }
}

/// Accept a certificate from anyone, for pairing only.
///
/// Every other listener refuses a peer it does not recognise. A pairing
/// listener cannot: the device joining is, by definition, not yet known.
///
/// What keeps that from being a hole is what the listener *does*. It answers
/// nothing but a pairing request, only one carrying a token that came from an
/// out-of-band invite, and it stops as soon as one device succeeds. The
/// certificate is still required and the handshake signature still verified, so
/// the fingerprint recorded for the joiner is proven rather than claimed.
#[derive(Debug)]
struct AnyCertificate {
    provider: Arc<CryptoProvider>,
}

impl ClientCertVerifier for AnyCertificate {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not accepted".into()))
    }

    /// Still verified. Accepting any *identity* is not the same as accepting an
    /// unproven one: the joiner must hold the key behind the certificate it
    /// presents, or the fingerprint we record would mean nothing.
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

    fn client_auth_mandatory(&self) -> bool {
        true
    }
}

/// A listener for pairing: any certificate, but nothing served except pairing.
pub fn pairing_server_config(identity: &Identity) -> Result<quinn::ServerConfig> {
    let verifier = AnyCertificate { provider: provider() };

    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| Error::Tls(e.to_string()))?
        .with_client_cert_verifier(Arc::new(verifier))
        .with_single_cert(vec![identity.cert_der()], identity.key_der()?)
        .map_err(|e| Error::Tls(e.to_string()))?;
    tls.alpn_protocols = vec![ALPN.to_vec()];

    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|e| Error::Tls(e.to_string()))?;
    Ok(quinn::ServerConfig::with_crypto(Arc::new(quic)))
}

/// Accept connections only from `allowed`.
pub fn server_config(identity: &Identity, allowed: &TrustList) -> Result<quinn::ServerConfig> {
    let pinned =
        Pinned { allowed: allowed.clone(), provider: provider(), expect_rejection: false };

    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| Error::Tls(e.to_string()))?
        .with_client_cert_verifier(Arc::new(pinned))
        .with_single_cert(vec![identity.cert_der()], identity.key_der()?)
        .map_err(|e| Error::Tls(e.to_string()))?;
    tls.alpn_protocols = vec![ALPN.to_vec()];

    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|e| Error::Tls(e.to_string()))?;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(quic));

    let mut transport = quinn::TransportConfig::default();
    // Each request is its own stream, and a sync can want many chunks at once.
    transport.max_concurrent_bidi_streams(256u32.into());
    // Close a connection that has gone quiet rather than holding resources for
    // a peer whose laptop was shut.
    transport.max_idle_timeout(Some(std::time::Duration::from_secs(30).try_into().unwrap()));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(10)));
    config.transport_config(Arc::new(transport));

    Ok(config)
}

/// Connect only to the peer with this fingerprint.
pub fn client_config(identity: &Identity, expected: Fingerprint) -> Result<quinn::ClientConfig> {
    client_config_inner(identity, expected, false)
}

/// A client config for a handshake that is meant to fail.
///
/// Used only by hole punching, which connects in order to send packets and
/// pins a fingerprint nothing can present. Identical to
/// [`client_config`] except that it does not report the refusal as a problem.
pub fn punch_config(identity: &Identity, expected: Fingerprint) -> Result<quinn::ClientConfig> {
    client_config_inner(identity, expected, true)
}

fn client_config_inner(
    identity: &Identity,
    expected: Fingerprint,
    expect_rejection: bool,
) -> Result<quinn::ClientConfig> {
    let pinned = Pinned {
        allowed: TrustList::new(vec![expected]),
        provider: provider(),
        expect_rejection,
    };

    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| Error::Tls(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(pinned))
        .with_client_auth_cert(vec![identity.cert_der()], identity.key_der()?)
        .map_err(|e| Error::Tls(e.to_string()))?;
    tls.alpn_protocols = vec![ALPN.to_vec()];

    let quic = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
        .map_err(|e| Error::Tls(e.to_string()))?;
    let mut config = quinn::ClientConfig::new(Arc::new(quic));

    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(std::time::Duration::from_secs(30).try_into().unwrap()));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(10)));
    config.transport_config(Arc::new(transport));

    Ok(config)
}

#[cfg(test)]
mod trust_tests {
    use super::*;

    fn fp(byte: u8) -> Fingerprint {
        Fingerprint::from_bytes([byte; 32])
    }

    /// A device paired after a listener started must be accepted without a
    /// restart. The verifier holds the list by reference, not by value.
    #[test]
    fn a_device_added_later_is_accepted() {
        let trust = TrustList::new(vec![fp(1)]);
        let held = trust.clone();

        assert!(!held.contains(&fp(2)), "not trusted yet");
        trust.replace(vec![fp(1), fp(2)]);
        assert!(held.contains(&fp(2)), "a device paired later was still refused");
    }

    /// And a device forgotten must stop being accepted. A set that only grew
    /// would keep letting in a device the user had removed, which is worse
    /// than needing a restart.
    #[test]
    fn a_device_removed_later_is_refused() {
        let trust = TrustList::new(vec![fp(1), fp(2)]);
        let held = trust.clone();

        assert!(held.contains(&fp(2)));
        trust.replace(vec![fp(1)]);
        assert!(!held.contains(&fp(2)), "a forgotten device is still accepted");
    }

    /// Clones share one list. If they did not, the daemon would be updating a
    /// copy nobody consults — which is exactly the bug this replaced, in a
    /// harder-to-see form.
    #[test]
    fn clones_share_one_list() {
        let a = TrustList::new(vec![]);
        let b = a.clone();
        a.replace(vec![fp(7)]);
        assert!(b.contains(&fp(7)), "the clone kept its own list");
    }
}
