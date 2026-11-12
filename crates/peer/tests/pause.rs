//! A device paused by its person (decision 0086) answers nobody: another
//! device's request is refused as if it were off, and answered again once
//! the pause ends. Its own binary, because the pause is process-wide.

use qurb_peer::{Identity, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use std::sync::{Arc, Mutex};

fn device(dir: &std::path::Path, key: u8) -> (Identity, Store) {
    let root = dir.join("sync");
    std::fs::create_dir_all(&root).unwrap();
    let store_dir = root.join(".qurb");
    let store = Store::open(&store_dir, ChunkKey::from_bytes([key; 32])).unwrap().in_tree(&root);
    (Identity::load_or_create(&store_dir).unwrap(), store)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paused_device_answers_nobody_until_it_resumes() {
    let (here_dir, there_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (here, store) = device(here_dir.path(), 1);
    let (there, _theirs) = device(there_dir.path(), 1);
    store.db().trust_peer(&qurb_sync::DeviceId::from_bytes([9; 32]), there.fingerprint().as_bytes(), "Phone").unwrap();
    let server = PeerServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        &here,
        &qurb_peer::tls::TrustList::new(vec![there.fingerprint()]),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });

    let asking = PeerClient::connect(addr, &there, here.fingerprint()).await.unwrap();
    assert!(asking.tree().await.is_ok(), "not answered before the pause");
    asking.close();

    qurb_peer::pause::set(qurb_peer::pause::UNTIL_RESUMED);
    let asking = PeerClient::connect(addr, &there, here.fingerprint()).await.unwrap();
    assert!(asking.tree().await.is_err(), "a paused device answered");
    asking.close();

    qurb_peer::pause::set(0);
    let asking = PeerClient::connect(addr, &there, here.fingerprint()).await.unwrap();
    assert!(asking.tree().await.is_ok(), "not answered after resuming");
    asking.close();
}
