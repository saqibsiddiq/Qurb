//! A rendezvous service on an address no certificate authority will vouch for.
//!
//! This is what a small deployment actually looks like: a host with an IP
//! address, no domain name, and nobody willing to sign a certificate for
//! `203.0.113.5`. The connection still has to be encrypted, because the
//! identifiers devices announce under are bearer secrets — so the certificate
//! is self-signed and the device is told its fingerprint in advance.

use qurb_keys::MasterKey;
use qurb_signal::{Certificate, Endpoints, GroupId, MemberId, SignalClient, SignalServer};
use std::sync::Arc;

fn endpoints() -> Endpoints {
    Endpoints { public: None, local: vec!["127.0.0.1:41935".parse().unwrap()] }
}

/// A certificate in a directory of its own, as the service keeps one.
fn certificate(dir: &std::path::Path) -> Certificate {
    Certificate::kept_in(dir, vec!["localhost".into()]).unwrap()
}

async fn serving(dir: &std::path::Path) -> (Arc<SignalServer>, String) {
    let cert = certificate(dir);
    let fingerprint = cert.fingerprint();

    let server = SignalServer::bind("127.0.0.1:0".parse().unwrap())
        .await
        .unwrap()
        .behind(cert)
        .unwrap();
    let address = server.local_addr().unwrap();
    let server = Arc::new(server);

    let running = Arc::clone(&server);
    tokio::spawn(async move { running.serve().await });

    (server, format!("wss://localhost:{}#{fingerprint}", address.port()))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_connects_to_a_service_it_was_told_the_fingerprint_of() {
    let dir = tempfile::tempdir().unwrap();
    let (server, url) = serving(dir.path()).await;

    let master = MasterKey::from_bytes([21; 32]);
    let client = SignalClient::connect(
        &url,
        GroupId::derive(&master),
        MemberId::derive(&master, &[1; 32]),
        endpoints(),
    )
    .await
    .expect("a pinned wss:// connection should be accepted");

    drop(client);
    assert!(server.connection_count() <= 1);
}

/// The whole point. A service presenting a different certificate is refused,
/// which is what stops somebody who can redirect the address from reading the
/// identifiers that cross it.
#[tokio::test(flavor = "multi_thread")]
async fn a_different_certificate_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (_server, url) = serving(dir.path()).await;

    // The fingerprint of some other service entirely.
    let elsewhere = tempfile::tempdir().unwrap();
    let wrong = certificate(elsewhere.path()).fingerprint();
    let (address, _) = url.split_once('#').unwrap();
    let misleading = format!("{address}#{wrong}");

    let master = MasterKey::from_bytes([22; 32]);
    let outcome = SignalClient::connect(
        &misleading,
        GroupId::derive(&master),
        MemberId::derive(&master, &[1; 32]),
        endpoints(),
    )
    .await;

    assert!(outcome.is_err(), "a service with the wrong certificate was accepted");
}

/// The fingerprint is what every device has been told to expect, so a service
/// that generated a new certificate on each restart would lock out every device
/// it had.
#[test]
fn a_restart_keeps_the_same_certificate() {
    let dir = tempfile::tempdir().unwrap();
    let first = certificate(dir.path()).fingerprint();
    let second = certificate(dir.path()).fingerprint();
    assert_eq!(first, second);
}

/// Two services are two identities. Worth asserting, because a bug that made
/// every deployment share a certificate would look like everything working.
#[test]
fn two_services_have_different_fingerprints() {
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    assert_ne!(certificate(one.path()).fingerprint(), certificate(two.path()).fingerprint());
}

/// It is printed to be copied, so it has to be a whole setting rather than
/// something to assemble.
#[test]
fn the_printed_url_is_the_setting() {
    let dir = tempfile::tempdir().unwrap();
    let cert = certificate(dir.path());
    let url = cert.url_for("203.0.113.5", 9000);

    assert_eq!(url, format!("wss://203.0.113.5:9000#{}", cert.fingerprint()));
    assert_eq!(cert.fingerprint().len(), 64, "a sha-256 fingerprint is 64 hex characters");
}

/// People paste fingerprints from `openssl` and from browsers, which print them
/// with colons and in capitals. Refusing those would be refusing a correct
/// answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_fingerprint_is_accepted_however_it_was_copied() {
    let dir = tempfile::tempdir().unwrap();
    let (_server, url) = serving(dir.path()).await;
    let (address, plain) = url.split_once('#').unwrap();

    let colons: String = plain
        .as_bytes()
        .chunks(2)
        .map(|pair| std::str::from_utf8(pair).unwrap().to_uppercase())
        .collect::<Vec<_>>()
        .join(":");

    let master = MasterKey::from_bytes([23; 32]);
    SignalClient::connect(
        &format!("{address}#{colons}"),
        GroupId::derive(&master),
        MemberId::derive(&master, &[1; 32]),
        endpoints(),
    )
    .await
    .expect("a fingerprint with colons and capitals should be accepted");
}
