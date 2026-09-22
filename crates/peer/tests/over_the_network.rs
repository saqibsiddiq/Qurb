//! Two devices syncing over a real QUIC connection.
//!
//! Everything here is genuine: real sockets on loopback, a real TLS 1.3
//! handshake with mutual certificate pinning, real chunk requests. What is not
//! genuine is the *distance* — both ends are in one process, so nothing here
//! says anything about behaviour across a NAT or a slow link.
//!
//! The two-device tests in `qurb-engine` already prove the decisions converge.
//! These prove the decisions survive being carried over a wire: that vectors
//! and tombstones encode and decode, that only missing chunks are requested,
//! and that a peer we do not recognise gets nowhere.

use qurb_engine::Engine;
use qurb_peer::{Fingerprint, Identity, NetworkSource, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::collections::BTreeMap;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const LOOPBACK: &str = "127.0.0.1:0";

/// One device: a directory, a store, an engine, and a network identity.
struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    engine: Engine,
    identity: Identity,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();

        let store_dir = root.join(".qurb");
        // One key for both devices, as a single user's devices would share.
        let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        let identity = Identity::load_or_create(&store_dir).unwrap();

        Self {
            _dir: dir,
            root: root.clone(),
            engine: Engine::new(root, store, ignore),
            identity,
        }
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn remove(&mut self, rel: &str) {
        fs::remove_file(self.root.join(rel)).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn on_disk(&self) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap().flatten() {
                let p = entry.path();
                if p.file_name().unwrap() == ".qurb" {
                    continue;
                }
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(&self.root).unwrap().to_string_lossy().to_string();
                    out.insert(rel, fs::read(&p).unwrap());
                }
            }
        }
        out
    }
}

/// A device's store, served in the background.
struct Served {
    addr: SocketAddr,
    fingerprint: Fingerprint,
    stats: Arc<qurb_peer::ServerStats>,
    _task: tokio::task::JoinHandle<()>,
}

/// Serve a device's store without taking it out of use.
///
/// The server opens its own connection to the same store. WAL mode allows
/// concurrent readers, and it is the shape a real daemon has anyway: one task
/// answering peers while another follows the filesystem.
fn serve(device: &Device, allowed: &[Fingerprint]) -> Served {
    let store_dir = device.root.join(".qurb");
    // With the folder attached, as a real server's store is: a syncing device
    // keeps its payloads in the files themselves, so a store that does not
    // know the folder would answer "not found" for every chunk it holds.
    let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&device.root);

    let server = PeerServer::bind(LOOPBACK.parse().unwrap(), &device.identity, &qurb_peer::tls::TrustList::new(allowed.to_vec())).unwrap();
    let addr = server.local_addr().unwrap();
    let stats = server.stats();
    let fingerprint = device.identity.fingerprint();

    let task = tokio::spawn(async move {
        server.serve(Arc::new(Mutex::new(store))).await;
    });

    Served { addr, fingerprint, stats, _task: task }
}

/// Pull everything the peer has into `local`, over the network.
///
/// The second store is not a workaround: the engine holds its own store mutably
/// while applying a plan, and the content source needs to read the same data to
/// find chunks already on disk. A real daemon has the same shape.
async fn pull(local: &mut Device, remote: &Served) -> qurb_engine::PlanStats {
    let client = PeerClient::connect(remote.addr, &local.identity, remote.fingerprint)
        .await
        .expect("connect");

    let tree = client.tree().await.expect("tree");
    let plan = local.engine.plan_against(&tree).unwrap();

    let reader = Store::open(&local.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&local.root);
    let mut source = NetworkSource::new(&client, &reader);
    let stats = local.engine.apply_plan(&plan, &mut source).unwrap();

    client.close();
    assert!(stats.is_clean(), "plan had failures: {:?}", stats.failures);
    stats
}

/// Exchange in both directions until neither side has anything left to do.
async fn sync_until_stable(a: &mut Device, b: &mut Device) -> usize {
    let a_fp = a.identity.fingerprint();
    let b_fp = b.identity.fingerprint();
    let a_served = serve(a, &[b_fp]);
    let b_served = serve(b, &[a_fp]);

    for round in 1..=6 {
        pull(b, &a_served).await;
        pull(a, &b_served).await;

        let a_tree = a.engine.tree().unwrap();
        let b_tree = b.engine.tree().unwrap();
        if a.engine.plan_against(&b_tree).unwrap().is_empty()
            && b.engine.plan_against(&a_tree).unwrap().is_empty()
        {
            return round;
        }
    }
    panic!("two devices did not converge over the network within 6 rounds");
}

// -- the basics --------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_file_crosses_the_network() {
    let mut a = Device::new();
    let mut b = Device::new();
    a.write("notes.txt", b"written on A");

    let b_fp = b.identity.fingerprint();
    let served = serve(&a, &[b_fp]);

    let stats = pull(&mut b, &served).await;
    assert_eq!(stats.adopted, 1);
    assert_eq!(b.on_disk().get("notes.txt").map(Vec::as_slice), Some(&b"written on A"[..]));
}

#[tokio::test(flavor = "multi_thread")]
async fn tombstones_cross_the_network() {
    // The file has to reach B *first*, so that A's later deletion is causally
    // after B's copy rather than concurrent with it. Two devices that each
    // created the file independently would be a genuine conflict, and decision
    // 0009 says the edit wins there -- which is a different test.
    let mut a = Device::new();
    let mut b = Device::new();
    a.write("doomed.txt", b"temporary");

    sync_until_stable(&mut a, &mut b).await;
    assert!(b.on_disk().contains_key("doomed.txt"), "setup: the file should have arrived");

    a.remove("doomed.txt");
    sync_until_stable(&mut a, &mut b).await;

    assert!(!b.on_disk().contains_key("doomed.txt"), "the deletion did not cross");
    assert!(a.on_disk().is_empty() && b.on_disk().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_concurrent_delete_loses_to_a_concurrent_edit_over_the_network() {
    // The case the previous test deliberately avoids. Both devices know the
    // file; one edits it and the other deletes it, neither having seen the
    // other. The edit must survive -- a deletion stays recoverable, a discarded
    // edit does not.
    let mut a = Device::new();
    let mut b = Device::new();
    a.write("contested.txt", b"original");
    sync_until_stable(&mut a, &mut b).await;

    a.write("contested.txt", b"edited, not deleted");
    b.remove("contested.txt");

    sync_until_stable(&mut a, &mut b).await;
    assert_eq!(
        b.on_disk().get("contested.txt").map(Vec::as_slice),
        Some(&b"edited, not deleted"[..]),
        "the edit did not come back to the device that deleted it"
    );
    assert_eq!(a.on_disk(), b.on_disk());
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_edits_conflict_identically_on_both_sides() {
    // Both devices must independently produce the same conflict filename, with
    // no negotiation. If they disagreed they would each rename the other's copy
    // and end up with two conflict files and no original.
    let mut a = Device::new();
    let mut b = Device::new();
    a.write("shared.txt", b"original");
    sync_until_stable(&mut a, &mut b).await;

    a.write("shared.txt", b"edited on A");
    b.write("shared.txt", b"edited on B");

    sync_until_stable(&mut a, &mut b).await;

    let files = a.on_disk();
    assert_eq!(files, b.on_disk(), "the two devices disagree about the outcome");
    assert_eq!(files.len(), 2, "both edits must survive: {:?}", files.keys().collect::<Vec<_>>());

    let contents: Vec<&[u8]> = files.values().map(Vec::as_slice).collect();
    assert!(contents.contains(&&b"edited on A"[..]), "A's edit was lost");
    assert!(contents.contains(&&b"edited on B"[..]), "B's edit was lost");
    assert!(files.keys().any(|k| k.starts_with("shared.conflict-")));
}

#[tokio::test(flavor = "multi_thread")]
async fn nested_and_non_ascii_paths_survive() {
    let mut a = Device::new();
    let mut b = Device::new();
    a.write("work/отчёт/q3 summary.md", "# Q3 — done".as_bytes());

    let b_fp = b.identity.fingerprint();
    let served = serve(&a, &[b_fp]);

    pull(&mut b, &served).await;
    assert_eq!(
        b.on_disk().get("work/отчёт/q3 summary.md").map(|v| String::from_utf8_lossy(v).to_string()),
        Some("# Q3 — done".to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_large_file_arrives_intact() {
    let mut a = Device::new();
    let mut b = Device::new();
    let content = pseudo_random(1, 6 << 20);
    a.write("big.bin", &content);

    let b_fp = b.identity.fingerprint();
    let served = serve(&a, &[b_fp]);

    pull(&mut b, &served).await;
    assert_eq!(b.on_disk().get("big.bin"), Some(&content), "the file did not survive transfer");
}

// -- incremental transfer ----------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn only_missing_chunks_cross_the_wire() {
    // The whole point of content-defined chunking, finally applied to a
    // network. A large file edited near its start shares almost every chunk
    // with the copy the other device already has.
    let mut a = Device::new();
    let mut b = Device::new();

    let original = pseudo_random(2, 8 << 20);
    a.write("big.bin", &original);
    b.write("big.bin", &original);

    // Change a few bytes near the beginning and add a little, which shifts
    // every byte after it -- the case fixed-size blocks handle worst.
    let mut edited = Vec::with_capacity(original.len() + 16);
    edited.extend_from_slice(&original[..1000]);
    edited.extend_from_slice(b"SIXTEEN NEW BYTE");
    edited.extend_from_slice(&original[1000..]);
    a.write("big.bin", &edited);

    let total_chunks = {
        let store = a.engine.store();
        let hash = blake3::hash(&edited);
        store.chunk_hashes_for_content(&hash).unwrap().unwrap().len()
    };

    let b_fp = b.identity.fingerprint();
    let served = serve(&a, &[b_fp]);

    // Count what actually crossed by watching the server's chunk reads.
    let before = served.stats.chunks();
    pull(&mut b, &served).await;
    let fetched = served.stats.chunks() - before;

    assert_eq!(b.on_disk().get("big.bin"), Some(&edited), "the edit did not arrive intact");
    assert!(total_chunks >= 8, "expected several chunks, got {total_chunks}");
    assert!(
        fetched <= 3,
        "{fetched} of {total_chunks} chunks crossed for a 16-byte edit ({} bytes); \
         chunking is not saving anything",
        served.stats.bytes()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn content_already_held_under_another_name_is_not_fetched() {
    let mut a = Device::new();
    let mut b = Device::new();
    let content = pseudo_random(3, 4 << 20);

    a.write("original.bin", &content);
    b.write("original.bin", &content);
    // Same bytes, new name. Nothing needs to move.
    a.write("renamed.bin", &content);

    let b_fp = b.identity.fingerprint();
    let served = serve(&a, &[b_fp]);

    let before = served.stats.chunks();
    let stats = pull(&mut b, &served).await;

    assert_eq!(served.stats.chunks() - before, 0, "bytes crossed for a file already held");
    assert_eq!(stats.fetched, 0);
    assert_eq!(b.on_disk().get("renamed.bin"), Some(&content));
}

// -- identity ----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn an_unrecognised_peer_is_refused() {
    // The server is told to accept one device. A different one must get
    // nowhere, even though it presents a perfectly valid certificate.
    let mut a = Device::new();
    a.write("secret.txt", b"not for strangers");

    let expected = Identity::load_or_create(tempfile::tempdir().unwrap().path()).unwrap();
    let stranger = Device::new();

    let served_fp = a.identity.fingerprint();
    let served = serve(&a, &[expected.fingerprint()]);

    let result = PeerClient::connect(served.addr, &stranger.identity, served_fp).await;
    match result {
        Err(_) => {}
        Ok(client) => {
            // Some failures only surface on first use rather than at connect.
            assert!(client.tree().await.is_err(), "a stranger was served files");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn connecting_to_the_wrong_fingerprint_fails() {
    // The client's protection: it will not talk to a server that is not the one
    // it meant to reach, however valid that server's certificate is.
    let mut a = Device::new();
    a.write("notes.txt", b"content");
    let b = Device::new();

    let b_fp = b.identity.fingerprint();
    let served = serve(&a, &[b_fp]);

    let wrong = Identity::load_or_create(tempfile::tempdir().unwrap().path())
        .unwrap()
        .fingerprint();

    let result = PeerClient::connect(served.addr, &b.identity, wrong).await;
    assert!(result.is_err(), "connected to a peer with the wrong fingerprint");
}

fn pseudo_random(seed: u32, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut x = seed | 1;
    for _ in 0..len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.push(x as u8);
    }
    out
}
