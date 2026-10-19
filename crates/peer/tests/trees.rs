//! A device's file list kept from the last sync, and fetched again only when
//! its fingerprint says it changed (`qurb_peer::trees`). On the S23 on
//! 2026-10-19 a pass with nothing changed received 9.8 MB, and on another
//! phone's hotspot ran out of time before the list arrived.

use qurb_engine::Engine;
use qurb_peer::trees::Trees;
use qurb_peer::{Identity, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

const KEY: [u8; 32] = [62; 32];

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
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unchanged_list_is_not_fetched_again() {
    let mut computer = Device::new();
    let phone = Device::new();
    for (one, other) in [(&computer, &phone), (&phone, &computer)] {
        one.engine.store().db().trust_peer(&other.id(), other.identity.fingerprint().as_bytes(), "other").unwrap();
    }
    for i in 0..300 {
        fs::write(computer.root.join(format!("file{i:03}.txt")), format!("file {i}")).unwrap();
    }
    computer.engine.reconcile().unwrap();

    let server = PeerServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        &computer.identity,
        &qurb_peer::tls::TrustList::new(vec![phone.identity.fingerprint()]),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let stats = server.stats();
    let store = Store::open(&computer.root.join(".qurb"), ChunkKey::from_bytes(KEY)).unwrap().in_tree(&computer.root);
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });

    let trees_dir = tempfile::tempdir().unwrap();
    let trees = Trees::at(trees_dir.path());
    let peer = computer.identity.fingerprint();
    let client = PeerClient::connect(addr, &phone.identity, peer).await.unwrap();

    let first = client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(first.len(), 300);
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 1);

    // Nothing changed: the kept list, and nothing more fetched.
    let again = client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(again, first);
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 1, "fetched again though nothing changed");

    // A file changed: fetched, and the change in it.
    fs::write(computer.root.join("file007.txt"), "changed").unwrap();
    computer.engine.reconcile().unwrap();
    let changed = client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 2, "a changed list was not fetched");
    let file = changed.iter().find(|v| v.path == "file007.txt").unwrap();
    assert_ne!(Some(file), first.iter().find(|v| v.path == "file007.txt"));

    // A kept list that is damaged is not used: fetched again instead.
    for entry in fs::read_dir(trees_dir.path()).unwrap() {
        let path = entry.unwrap().path();
        let mut bytes = fs::read(&path).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        fs::write(&path, bytes).unwrap();
    }
    let after = client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(after, changed);
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 3, "a damaged kept list was used");
}
