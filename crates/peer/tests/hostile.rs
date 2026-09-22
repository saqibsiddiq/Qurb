//! A peer that is broken, or lying.
//!
//! Everything else assumes the other end is a working copy of this software.
//! These tests assume the opposite: a peer that sends the wrong bytes, claims to
//! have content it does not, answers with nonsense, or vanishes mid-transfer.
//!
//! The property being defended is simple and absolute: **a peer can waste our
//! time, but it cannot corrupt our store.** Content is addressed by hash, so
//! every byte that arrives can be checked against what was asked for, and
//! anything else is refused before it reaches disk.
//!
//! This matters more than it might seem. Relays are Phase 3 and will forward
//! traffic we do not control, and a device that is stolen or compromised keeps
//! its pinned identity. Authentication says *who* is talking; it says nothing
//! about whether they are telling the truth.

use qurb_peer::wire::{Request, Response, MAX_MESSAGE};
use qurb_peer::{Fingerprint, Identity, PeerClient};
use qurb_storage::{ChunkKey, Store};
use std::net::SocketAddr;

/// How a hostile server should answer.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Answer every chunk request with bytes that are not the ones asked for.
    WrongContent,
    /// Claim a file is made of chunks, then deny having them.
    ManifestWithoutChunks,
    /// Reply with bytes that are not a valid message at all.
    Garbage,
    /// Accept the request and then close without answering.
    Silence,
    /// Answer the wrong kind of message.
    WrongMessageType,
}

/// Start a server that behaves badly, and return where to reach it.
fn start_hostile(
    identity: &Identity,
    allowed: Fingerprint,
    behaviour: Behaviour,
    real_manifest: Vec<[u8; 32]>,
) -> SocketAddr {
    let config = qurb_peer::tls::server_config(identity, &qurb_peer::tls::TrustList::new(vec![allowed])).unwrap();
    let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = endpoint.local_addr().unwrap();

    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let manifest = real_manifest.clone();
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else { return };
                while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                    let Ok(raw) = recv.read_to_end(MAX_MESSAGE).await else { return };
                    let Ok(request) = Request::decode(&raw) else { return };

                    let reply: Option<Vec<u8>> = match (behaviour, &request) {
                        (Behaviour::Silence, _) => None,
                        (Behaviour::Garbage, _) => Some(vec![0xFF; 64]),
                        (Behaviour::WrongMessageType, _) => {
                            Some(Response::Chunk(b"not a tree".to_vec()).encode())
                        }
                        (_, Request::Tree) => Some(Response::Tree(Vec::new()).encode()),
                        (Behaviour::WrongContent, Request::Manifest { .. })
                        | (Behaviour::ManifestWithoutChunks, Request::Manifest { .. }) => {
                            Some(Response::Manifest(manifest.clone()).encode())
                        }
                        (Behaviour::WrongContent, Request::Chunk { .. }) => {
                            Some(Response::Chunk(b"plausible but wrong".to_vec()).encode())
                        }
                        (Behaviour::ManifestWithoutChunks, Request::Chunk { .. }) => {
                            Some(Response::NotFound.encode())
                        }
                        // Pairing has its own listener; this one never serves it.
                        (_, Request::Pair { .. }) => Some(Response::NotFound.encode()),
                        // A hostile peer that claims to have changed, endlessly,
                        // can waste our time and nothing else.
                        (_, Request::Changes { .. }) => {
                            Some(Response::Changed { generation: u64::MAX }.encode())
                        }
                    };

                    match reply {
                        Some(bytes) => {
                            let _ = send.write_all(&bytes).await;
                            let _ = send.finish();
                        }
                        None => {
                            // Accept the request, answer nothing, go away.
                            connection.close(0u32.into(), b"");
                            return;
                        }
                    }
                }
            });
        }
    });

    addr
}

/// A local store holding nothing, so every chunk must come from the peer.
fn empty_store(dir: &std::path::Path) -> Store {
    Store::open(dir, ChunkKey::from_bytes([42; 32])).unwrap()
}

/// The chunk list of some real content, so a hostile manifest is plausible.
fn real_content() -> (Vec<u8>, [u8; 32], Vec<[u8; 32]>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("store"), ChunkKey::from_bytes([42; 32])).unwrap();

    let mut content = Vec::with_capacity(2 << 20);
    let mut x: u32 = 99;
    for _ in 0..(2usize << 20) {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        content.push(x as u8);
    }
    store.put_bytes("real.bin", &content, 0).unwrap();

    let hash = *blake3::hash(&content).as_bytes();
    let chunks = store
        .chunk_hashes_for_content(&blake3::Hash::from(hash))
        .unwrap()
        .unwrap()
        .iter()
        .map(|h| *h.as_bytes())
        .collect();
    (content, hash, chunks, dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_sends_the_wrong_bytes_is_caught() {
    // The core defence. Chunks are named by their content, so a lying peer is
    // detected by arithmetic rather than by trust.
    let (_content, hash, chunks, _keep) = real_content();

    let server_dir = tempfile::tempdir().unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let server_identity = Identity::load_or_create(server_dir.path()).unwrap();
    let client_identity = Identity::load_or_create(client_dir.path()).unwrap();

    let addr = start_hostile(
        &server_identity,
        client_identity.fingerprint(),
        Behaviour::WrongContent,
        chunks,
    );

    let local = empty_store(&client_dir.path().join("store"));
    let client = PeerClient::connect(addr, &client_identity, server_identity.fingerprint())
        .await
        .unwrap();

    let result = client.fetch_content(&local, hash, 2 << 20).await;
    assert!(result.is_err(), "a peer's wrong bytes were accepted");
    assert!(
        local.verify(true).unwrap().is_healthy(),
        "the hostile response reached the store"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_promises_chunks_it_lacks_fails_cleanly() {
    let (_content, hash, chunks, _keep) = real_content();

    let server_dir = tempfile::tempdir().unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let server_identity = Identity::load_or_create(server_dir.path()).unwrap();
    let client_identity = Identity::load_or_create(client_dir.path()).unwrap();

    let addr = start_hostile(
        &server_identity,
        client_identity.fingerprint(),
        Behaviour::ManifestWithoutChunks,
        chunks,
    );

    let local = empty_store(&client_dir.path().join("store"));
    let client = PeerClient::connect(addr, &client_identity, server_identity.fingerprint())
        .await
        .unwrap();

    assert!(client.fetch_content(&local, hash, 2 << 20).await.is_err());
    assert!(local.verify(true).unwrap().is_healthy());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_answers_with_nonsense_is_refused() {
    let (_content, _hash, chunks, _keep) = real_content();

    let server_dir = tempfile::tempdir().unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let server_identity = Identity::load_or_create(server_dir.path()).unwrap();
    let client_identity = Identity::load_or_create(client_dir.path()).unwrap();

    let addr = start_hostile(
        &server_identity,
        client_identity.fingerprint(),
        Behaviour::Garbage,
        chunks,
    );

    let client = PeerClient::connect(addr, &client_identity, server_identity.fingerprint())
        .await
        .unwrap();

    // An error, not a panic and not a hang. The decoder has no panicking path.
    assert!(client.tree().await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_answers_the_wrong_question_is_refused() {
    let (_content, _hash, chunks, _keep) = real_content();

    let server_dir = tempfile::tempdir().unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let server_identity = Identity::load_or_create(server_dir.path()).unwrap();
    let client_identity = Identity::load_or_create(client_dir.path()).unwrap();

    let addr = start_hostile(
        &server_identity,
        client_identity.fingerprint(),
        Behaviour::WrongMessageType,
        chunks,
    );

    let client = PeerClient::connect(addr, &client_identity, server_identity.fingerprint())
        .await
        .unwrap();

    let err = client.tree().await.unwrap_err().to_string();
    assert!(err.contains("tree"), "the mismatch should name what was asked for: {err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_goes_quiet_does_not_hang_us_forever() {
    let (_content, _hash, chunks, _keep) = real_content();

    let server_dir = tempfile::tempdir().unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let server_identity = Identity::load_or_create(server_dir.path()).unwrap();
    let client_identity = Identity::load_or_create(client_dir.path()).unwrap();

    let addr = start_hostile(
        &server_identity,
        client_identity.fingerprint(),
        Behaviour::Silence,
        chunks,
    );

    let client = PeerClient::connect(addr, &client_identity, server_identity.fingerprint())
        .await
        .unwrap();

    // The connection is closed rather than left open, so this must return
    // rather than wait for the idle timeout.
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), client.tree()).await;
    assert!(outcome.is_ok(), "a silent peer left us waiting");
    assert!(outcome.unwrap().is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_disappears_mid_session_fails_the_next_request() {
    let (_content, _hash, chunks, _keep) = real_content();

    let server_dir = tempfile::tempdir().unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let server_identity = Identity::load_or_create(server_dir.path()).unwrap();
    let client_identity = Identity::load_or_create(client_dir.path()).unwrap();

    let addr = start_hostile(
        &server_identity,
        client_identity.fingerprint(),
        Behaviour::ManifestWithoutChunks,
        chunks,
    );

    let client = PeerClient::connect(addr, &client_identity, server_identity.fingerprint())
        .await
        .unwrap();
    assert!(client.tree().await.is_ok(), "setup: the first request should work");

    // Closing from our side stands in for the peer vanishing: either way the
    // connection is gone and the next request must fail rather than hang.
    client.close();
    let outcome =
        tokio::time::timeout(std::time::Duration::from_secs(10), client.tree()).await;
    assert!(outcome.is_ok(), "a dead connection left us waiting");
    assert!(outcome.unwrap().is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_hostile_ever_reaches_disk() {
    // The summary property, stated once over every behaviour: a peer can waste
    // our time, but it cannot corrupt our store.
    let (_content, hash, chunks, _keep) = real_content();

    for behaviour in [
        Behaviour::WrongContent,
        Behaviour::ManifestWithoutChunks,
        Behaviour::Garbage,
        Behaviour::WrongMessageType,
    ] {
        let server_dir = tempfile::tempdir().unwrap();
        let client_dir = tempfile::tempdir().unwrap();
        let server_identity = Identity::load_or_create(server_dir.path()).unwrap();
        let client_identity = Identity::load_or_create(client_dir.path()).unwrap();

        let addr = start_hostile(
            &server_identity,
            client_identity.fingerprint(),
            behaviour,
            chunks.clone(),
        );

        let local = empty_store(&client_dir.path().join("store"));
        let client = PeerClient::connect(addr, &client_identity, server_identity.fingerprint())
            .await
            .unwrap();

        let _ = client.tree().await;
        let _ = client.fetch_content(&local, hash, 2 << 20).await;

        assert!(local.verify(true).unwrap().is_healthy(), "store damaged by a hostile peer");
        assert_eq!(local.db().chunk_count().unwrap(), 0, "hostile bytes were stored");
        client.close();
    }
}

