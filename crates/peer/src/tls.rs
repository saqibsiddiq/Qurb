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

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The set of peers a connection will accept, by fingerprint.
#[derive(Debug, Clone)]
struct Pinned {
    allowed: Vec<Fingerprint>,
    provider: Arc<CryptoProvider>,
}

impl Pinned {
    fn check(&self, presented: &CertificateDer<'_>) -> std::result::Result<(), rustls::Error> {
        let fingerprint = Fingerprint::from_bytes(*blake3::hash(presented.as_ref()).as_bytes());
        if self.allowed.contains(&fingerprint) {
            Ok(())
        } else {
            tracing::warn!(peer = %fingerprint.short(), "rejected an unrecognised peer");
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
pub fn server_config(identity: &Identity, allowed: &[Fingerprint]) -> Result<quinn::ServerConfig> {
    let pinned = Pinned { allowed: allowed.to_vec(), provider: provider() };

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
    let pinned = Pinned { allowed: vec![expected], provider: provider() };

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
