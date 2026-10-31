//! The speed limit (`qurb_peer::limit`), over a real connection.
//!
//! In a test binary of its own: the limit is the whole process's, and would
//! slow every other test that ran beside it.

use qurb_engine::Engine;
use qurb_peer::{Identity, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const KEY: [u8; 32] = [77; 32];

/// A file of `size` bytes in a folder served on this machine, and a client
/// connected to it: the server and its folder kept alive with it.
async fn served(size: usize) -> (tempfile::TempDir, PeerClient, Vec<[u8; 32]>, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sync");
    std::fs::create_dir_all(&root).unwrap();
    let store_dir = root.join(".qurb");
    let bytes: Vec<u8> = (0..size as u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 9) as u8).collect();
    std::fs::write(root.join("film.mkv"), &bytes).unwrap();
    let store = Store::open(&store_dir, ChunkKey::from_bytes(KEY)).unwrap();
    let mut engine = Engine::new(root.clone(), store, IgnoreRules::new().with_store_dir(&store_dir));
    engine.reconcile().unwrap();

    let server_id = Identity::load_or_create(&store_dir).unwrap();
    let client_id = Identity::load_or_create(&dir.path().join("client")).unwrap();
    let server = PeerServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        &server_id,
        &qurb_peer::tls::TrustList::new(vec![client_id.fingerprint()]),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let reader = Store::open(&store_dir, ChunkKey::from_bytes(KEY)).unwrap().in_tree(&root);
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(reader))).await });
    let client = PeerClient::connect(addr, &client_id, server_id.fingerprint()).await.unwrap();
    let chunks = client.manifest(*blake3::hash(&bytes).as_bytes()).await.unwrap().unwrap();
    (dir, client, chunks, bytes)
}

async fn fetch_all(client: &PeerClient, chunks: &[[u8; 32]]) -> (Vec<u8>, f64) {
    let start = Instant::now();
    let mut got = Vec::new();
    for hash in chunks {
        got.extend(client.chunk(*hash).await.unwrap().unwrap());
    }
    (got, start.elapsed().as_secs_f64())
}

/// 4 MB fetched with a 1 MB/s download limit takes three seconds or more --
/// the first second's worth goes at once -- and with none, well under one.
/// The times it took are printed: `cargo test -p qurb-peer --test limit --
/// --nocapture`.
#[tokio::test(flavor = "multi_thread")]
async fn a_download_limit_holds_a_fetch_to_its_rate() {
    const SIZE: usize = 4_000_000;
    let (_dir, client, chunks, bytes) = served(SIZE).await;

    qurb_peer::limit::set(0, 0);
    let (got, free) = fetch_all(&client, &chunks).await;
    assert_eq!(got, bytes);

    qurb_peer::limit::set(0, 1_000_000);
    let (got, held) = fetch_all(&client, &chunks).await;
    qurb_peer::limit::set(0, 0);
    assert_eq!(got, bytes);
    println!("4 MB in {free:.2} s with no limit, {held:.2} s at 1 MB/s");
    assert!(free < 1.0, "{free} s with no limit");
    assert!(held >= 2.9, "{held} s at 1 MB/s");
    assert!(held < 5.0, "{held} s at 1 MB/s: held back more than the rate");
}
