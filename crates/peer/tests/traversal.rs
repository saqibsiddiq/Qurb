//! Hole punching, and the socket it depends on.
//!
//! # What these tests can and cannot show
//!
//! There is no NAT on loopback, so nothing here demonstrates traversal. What it
//! does demonstrate is the mechanism that traversal needs: that the socket which
//! did the discovery and the punching is the same one QUIC then runs on, and
//! that a connection survives having had unrelated packets arrive on it first.
//!
//! That distinction matters because getting it wrong is silent. Discovering an
//! address on one socket and connecting on another works perfectly in a lab and
//! fails on every real network, because the router mapping belongs to the port
//! that sent the packets, and the hole was punched for an address nobody is
//! listening on.
//!
//! Whether traversal itself works is the Phase 3 kill criterion, and it can only
//! be measured across real networks — see the `netcheck` example.

use qurb_peer::nat;
use qurb_peer::{Identity, PeerClient};
use qurb_storage::{ChunkKey, Store};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};

fn identity() -> (tempfile::TempDir, Identity) {
    let dir = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_create(dir.path()).unwrap();
    (dir, identity)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_socket_keeps_its_port_across_every_stage() {
    // The property the whole approach rests on. If any stage rebinds, the peer
    // was told about a mapping that leads nowhere.
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();

    nat::punch(&socket, "127.0.0.1:9".parse().unwrap(), 2).unwrap();
    assert_eq!(socket.local_addr().unwrap().port(), port, "punching moved the socket");

    let endpoint = nat::endpoint_from(socket, None).unwrap();
    assert_eq!(
        endpoint.local_addr().unwrap().port(),
        port,
        "handing the socket to QUIC moved it"
    );
}

#[test]
fn punching_a_peer_that_is_not_there_is_not_an_error() {
    // The first packets are supposed to be dropped -- that is the mechanism, not
    // a failure. Treating it as one would abort every connection before it
    // started.
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    assert!(nat::punch(&socket, "127.0.0.1:1".parse().unwrap(), 3).is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_connect_over_punched_sockets() {
    // Both sides punch towards each other, then run QUIC on the very sockets
    // that did it -- the sequence a real traversal follows, minus the NAT.
    let (_host_dir, host_identity) = identity();
    let (_joiner_dir, joiner_identity) = identity();

    let host_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let joiner_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let host_addr: SocketAddr = host_socket.local_addr().unwrap();
    let joiner_addr: SocketAddr = joiner_socket.local_addr().unwrap();

    // Simultaneous, as signalling would arrange.
    nat::punch(&host_socket, joiner_addr, 3).unwrap();
    nat::punch(&joiner_socket, host_addr, 3).unwrap();

    // The host serves from its punched socket.
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("store"), ChunkKey::from_bytes([42; 32])).unwrap();
    store.put_bytes("through-the-hole.txt", b"it arrived", 0).unwrap();
    let store = Arc::new(Mutex::new(store));

    let config =
        qurb_peer::tls::server_config(&host_identity, &qurb_peer::tls::TrustList::new(vec![joiner_identity.fingerprint()])).unwrap();
    let host_endpoint = nat::endpoint_from(host_socket, Some(config)).unwrap();

    let serving = Arc::clone(&store);
    tokio::spawn(async move {
        while let Some(incoming) = host_endpoint.accept().await {
            let store = Arc::clone(&serving);
            tokio::spawn(async move {
                if let Ok(connection) = incoming.await {
                    qurb_peer::server::serve_connection_for_test(connection, store).await;
                }
            });
        }
    });

    // The joiner connects from its own punched socket.
    let client = PeerClient::connect_on(
        joiner_socket,
        host_addr,
        &joiner_identity,
        host_identity.fingerprint(),
    )
    .await
    .expect("connect over punched sockets");

    let tree = client.tree().await.expect("serve over punched sockets");
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].path, "through-the-hole.txt");
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_survives_stray_packets_arriving_first() {
    // Knock packets, late STUN replies and internet background noise all land on
    // the same port. QUIC must ignore them rather than be confused by them.
    let (_host_dir, host_identity) = identity();
    let (_joiner_dir, joiner_identity) = identity();

    let host_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let host_addr: SocketAddr = host_socket.local_addr().unwrap();

    let noise = UdpSocket::bind("127.0.0.1:0").unwrap();
    for _ in 0..5 {
        noise.send_to(b"qurb-knock", host_addr).unwrap();
        noise.send_to(&[0xFF; 64], host_addr).unwrap();
    }

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Mutex::new(
        Store::open(&dir.path().join("store"), ChunkKey::from_bytes([42; 32])).unwrap(),
    ));

    let config =
        qurb_peer::tls::server_config(&host_identity, &qurb_peer::tls::TrustList::new(vec![joiner_identity.fingerprint()])).unwrap();
    let host_endpoint = nat::endpoint_from(host_socket, Some(config)).unwrap();
    let serving = Arc::clone(&store);
    tokio::spawn(async move {
        while let Some(incoming) = host_endpoint.accept().await {
            let store = Arc::clone(&serving);
            tokio::spawn(async move {
                if let Ok(connection) = incoming.await {
                    qurb_peer::server::serve_connection_for_test(connection, store).await;
                }
            });
        }
    });

    let joiner_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let client = PeerClient::connect_on(
        joiner_socket,
        host_addr,
        &joiner_identity,
        host_identity.fingerprint(),
    )
    .await
    .expect("connect despite the noise");

    assert!(client.tree().await.is_ok());
    client.close();
}
