//! A device's file list kept from the last sync, brought up to date with only
//! what changed in it, and fetched whole only when that does not add up
//! (`qurb_peer::trees`, decisions 0071 and 0072). On the S23 on 2026-10-19 a
//! pass received the whole list, 9.8 MB, and on another phone's hotspot ran
//! out of time before it arrived.

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
async fn only_what_changed_in_a_list_is_sent() {
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

    // Nothing changed: the kept list, and no list sent.
    let again = client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(qurb_peer::wire::tree_digest(&again), qurb_peer::wire::tree_digest(&first));
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 1, "sent again though nothing changed");

    // A file changed and one added: only those sent, the list rebuilt here,
    // and it is the computer's list (decision 0072).
    fs::write(computer.root.join("file007.txt"), "changed").unwrap();
    fs::write(computer.root.join("added.txt"), "added").unwrap();
    computer.engine.reconcile().unwrap();
    let changed = client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 1, "the whole list sent for two changes");
    assert_eq!(changed.len(), 301);
    let theirs = computer.engine.store().tree_for(qurb_storage::db::Audience::Device(&phone.id())).unwrap();
    assert_eq!(qurb_peer::wire::tree_digest(&changed), qurb_peer::wire::tree_digest(&theirs), "rebuilt wrong");

    // A deletion is a change like any other.
    fs::remove_file(computer.root.join("file008.txt")).unwrap();
    computer.engine.reconcile().unwrap();
    let deleted = client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 1);
    assert!(deleted.iter().any(|v| v.path == "file008.txt" && v.content.is_deleted()));

    // Most of it changed: sent whole, as cheap as its changes.
    for i in 0..250 {
        fs::write(computer.root.join(format!("file{i:03}.txt")), format!("again {i}")).unwrap();
    }
    computer.engine.reconcile().unwrap();
    client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 2, "most of a list changed was not sent whole");

    // A kept list that is damaged is not used: sent whole instead.
    for entry in fs::read_dir(trees_dir.path()).unwrap() {
        let path = entry.unwrap().path();
        let mut bytes = fs::read(&path).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        fs::write(&path, bytes).unwrap();
    }
    client.tree_kept(&peer, &trees).await.unwrap();
    assert_eq!(stats.trees_served.load(Ordering::Relaxed), 3, "a damaged kept list was used");
}

/// A device that sends its list only whole, as a computer keeping this
/// device's vault sealed does: a kept list is still used while that device's
/// fingerprint says it is its list (decision 0071), and not fetched every
/// pass.
#[tokio::test(flavor = "multi_thread")]
async fn a_list_sent_only_whole_is_not_sent_again_unchanged() {
    use qurb_peer::wire::{tree_digest, tree_frames, Request, Response, MAX_MESSAGE};
    use std::sync::atomic::AtomicUsize;

    let server_dir = tempfile::tempdir().unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let server = Identity::load_or_create(server_dir.path()).unwrap();
    let client = Identity::load_or_create(client_dir.path()).unwrap();
    let mut vector = qurb_sync::VersionVector::new();
    vector.set(qurb_sync::DeviceId::from_bytes([7; 32]), 1);
    let list: Vec<qurb_sync::FileVersion> = (0..50)
        .map(|i| {
            qurb_sync::FileVersion::file(format!("f{i:02}"), [i; 32], 10, vector.clone(), qurb_sync::DeviceId::from_bytes([9; 32]), 1_760_000_000)
        })
        .collect();

    let config = qurb_peer::tls::server_config(&server, &qurb_peer::tls::TrustList::new(vec![client.fingerprint()])).unwrap();
    let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = endpoint.local_addr().unwrap();
    let sent_whole = Arc::new(AtomicUsize::new(0));
    let counted = sent_whole.clone();
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let (list, counted) = (list.clone(), counted.clone());
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else { return };
                while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                    let Ok(raw) = recv.read_to_end(MAX_MESSAGE).await else { return };
                    match Request::decode(&raw) {
                        Ok(Request::TreeStream) => {
                            counted.fetch_add(1, Ordering::Relaxed);
                            for frame in tree_frames(&list) {
                                let _ = send.write_all(&frame).await;
                            }
                        }
                        Ok(Request::TreeDigest) => {
                            let _ = send.write_all(&Response::Digest(tree_digest(&list)).encode()).await;
                        }
                        _ => {
                            let _ = send.write_all(&Response::NotFound.encode()).await;
                        }
                    }
                    let _ = send.finish();
                }
            });
        }
    });

    let trees_dir = tempfile::tempdir().unwrap();
    let trees = Trees::at(trees_dir.path());
    let peer = server.fingerprint();
    let asking = PeerClient::connect(addr, &client, peer).await.unwrap();
    assert_eq!(asking.tree_kept(&peer, &trees).await.unwrap().len(), 50);
    assert_eq!(sent_whole.load(Ordering::Relaxed), 1);
    assert_eq!(asking.tree_kept(&peer, &trees).await.unwrap().len(), 50);
    assert_eq!(sent_whole.load(Ordering::Relaxed), 1, "sent whole again though its fingerprint was the same");
}
