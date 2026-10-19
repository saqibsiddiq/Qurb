//! Reading part of a file from another device (`qurb_peer::Ranged`): what a
//! player seeking in a video kept on a computer asks for.

use qurb_engine::Engine;
use qurb_peer::{Identity, PeerClient, PeerServer, Ranged};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const KEY: [u8; 32] = [61; 32];

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
        let store = Store::open(&store_dir, ChunkKey::from_bytes(KEY)).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        let identity = Identity::load_or_create(&store_dir).unwrap();
        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore), identity }
    }

    fn id(&self) -> qurb_sync::DeviceId {
        self.engine.store().device_id().unwrap()
    }

    /// Serve this device's store to `allowed`, as the daemon does.
    fn serve(&self, allowed: &Device) -> std::net::SocketAddr {
        let server = PeerServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            &self.identity,
            &qurb_peer::tls::TrustList::new(vec![allowed.identity.fingerprint()]),
        )
        .unwrap();
        let addr = server.local_addr().unwrap();
        let store = Store::open(&self.root.join(".qurb"), ChunkKey::from_bytes(KEY)).unwrap().in_tree(&self.root);
        tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });
        addr
    }
}

fn noisy(size: usize) -> Vec<u8> {
    let mut state = 0x9e3779b97f4a7c15u64;
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn parts_of_a_file_are_read_as_they_are_asked_for() {
    let computer = Device::new();
    let phone = Device::new();
    for (one, other) in [(&computer, &phone), (&phone, &computer)] {
        one.engine.store().db().trust_peer(&other.id(), other.identity.fingerprint().as_bytes(), "other").unwrap();
    }
    // Several chunks: they average 512 KiB.
    let video = noisy(6 * 1024 * 1024 + 12_345);
    fs::write(computer.root.join("film.mkv"), &video).unwrap();
    let mut engine = computer.engine;
    engine.reconcile().unwrap();
    let computer = Device { engine, ..computer };

    let addr = computer.serve(&phone);
    let client = PeerClient::connect(addr, &phone.identity, computer.identity.fingerprint()).await.unwrap();
    let content = *blake3::hash(&video).as_bytes();
    let ranged = Ranged::open(client, content).await.unwrap().expect("not shown the file");
    assert_eq!(ranged.size(), video.len() as u64);

    // The start, across chunk boundaries, the end, a seek backwards, and
    // past the end.
    let size = video.len() as u64;
    for (offset, length) in [
        (0u64, 4096usize),
        (500_000, 1_000_000),
        (1_048_576, 131_072),
        (size - 1000, 4096),
        (3_000_000, 1),
        (10, 300_000),
        (size, 10),
        (size + 5, 10),
    ] {
        let read = ranged.read_at(offset, length).await.unwrap();
        let from = (offset as usize).min(video.len());
        let to = (offset as usize).saturating_add(length).min(video.len());
        assert_eq!(read, &video[from..to.max(from)], "wrong bytes at {offset}+{length}");
    }
    // Never more than the bound, whatever is asked.
    assert_eq!(ranged.read_at(0, 10 << 20).await.unwrap().len(), qurb_peer::ranged::MOST);

    // Closed, as the phone does when its network changes under a player: a
    // read then fails at once, rather than waiting out a connection that is
    // gone (2026-10-19, a player declared not responding).
    let addr = computer.serve(&phone);
    let client = PeerClient::connect(addr, &phone.identity, computer.identity.fingerprint()).await.unwrap();
    let fresh = Ranged::open(client, content).await.unwrap().expect("not shown the file");
    fresh.close();
    let asked = std::time::Instant::now();
    assert!(fresh.read_at(size / 2, 4096).await.is_err(), "read after closing");
    assert!(asked.elapsed() < std::time::Duration::from_secs(2), "waited {:?}", asked.elapsed());

    // Content it does not hold is not offered.
    let addr = computer.serve(&phone);
    let client = PeerClient::connect(addr, &phone.identity, computer.identity.fingerprint()).await.unwrap();
    assert!(Ranged::open(client, [7; 32]).await.unwrap().is_none());
}
