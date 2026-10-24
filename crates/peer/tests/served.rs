//! What a device serving others is told about who reached it
//! (`qurb_peer::Served::asked_by`): what lets a phone know a computer is on
//! when the computer reached the phone and the phone could not reach it.

use qurb_peer::tls::TrustList;
use qurb_peer::{Fingerprint, Generation, Identity, PeerClient};
use qurb_storage::{ChunkKey, Store};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Heard {
    asked: Mutex<Vec<Fingerprint>>,
}

impl qurb_peer::Served for Heard {
    fn served(&self, _to: &Fingerprint, _chunk: &blake3::Hash, _bytes: u64) {}

    fn asked_by(&self, from: &Fingerprint) {
        self.asked.lock().unwrap().push(*from);
    }
}

struct Device {
    dir: tempfile::TempDir,
    identity: Identity,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::load_or_create(&dir.path().join(".qurb")).unwrap();
        Self { dir, identity }
    }

    /// Serve an empty store to whoever `trust` allows, as a phone does during
    /// a pass, telling `heard`.
    fn serve(&self, trust: TrustList, heard: Arc<Heard>) -> SocketAddr {
        let config = qurb_peer::tls::server_config(&self.identity, &trust).unwrap();
        let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = endpoint.local_addr().unwrap();
        let root = self.dir.path().to_path_buf();
        let store = Store::open(&root.join(".qurb"), ChunkKey::from_bytes([7; 32])).unwrap().in_tree(&root);
        let store = Arc::new(std::sync::Mutex::new(store));
        let generation = Generation::new();
        tokio::spawn(async move {
            while let Some(incoming) = endpoint.accept().await {
                let (store, generation, trust) = (Arc::clone(&store), Arc::clone(&generation), trust.clone());
                let heard: Arc<dyn qurb_peer::Served> = heard.clone();
                tokio::spawn(async move {
                    if let Ok(connection) = incoming.await {
                        qurb_peer::server::serve_connection_watched(connection, store, generation, heard, &trust)
                            .await;
                    }
                });
            }
        });
        addr
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trusted_device_that_asks_is_heard_once_per_connection() {
    let phone = Device::new();
    let computer = Device::new();
    let heard = Arc::new(Heard::default());
    let addr = phone.serve(TrustList::new(vec![computer.identity.fingerprint()]), Arc::clone(&heard));

    let client = PeerClient::connect(addr, &computer.identity, phone.identity.fingerprint()).await.unwrap();
    client.tree_digest().await.unwrap();
    client.tree_digest().await.unwrap();

    assert_eq!(*heard.asked.lock().unwrap(), vec![computer.identity.fingerprint()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_not_trusted_is_never_heard() {
    let phone = Device::new();
    let computer = Device::new();
    let stranger = Device::new();
    let heard = Arc::new(Heard::default());
    let addr = phone.serve(TrustList::new(vec![computer.identity.fingerprint()]), Arc::clone(&heard));

    // Refused at the handshake, or at its first request: either way, not a
    // device that reached this one.
    if let Ok(client) = PeerClient::connect(addr, &stranger.identity, phone.identity.fingerprint()).await {
        let _ = client.tree_digest().await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(heard.asked.lock().unwrap().is_empty(), "a stranger was heard");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_removed_before_it_asks_is_not_heard() {
    let phone = Device::new();
    let computer = Device::new();
    let heard = Arc::new(Heard::default());
    let trust = TrustList::new(vec![computer.identity.fingerprint()]);
    let addr = phone.serve(trust.clone(), Arc::clone(&heard));

    let client = PeerClient::connect(addr, &computer.identity, phone.identity.fingerprint()).await.unwrap();
    // Removed on the phone after connecting and before asking anything.
    trust.replace(Vec::new());
    let _ = client.tree_digest().await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(heard.asked.lock().unwrap().is_empty(), "a removed device was heard");
}
