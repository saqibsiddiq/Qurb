//! Knowing that content actually arrived.
//!
//! A device that shares a file while the other one is off needs to answer one
//! question afterwards: did it get there? Nothing else in the protocol asks it
//! — every other message is a device fetching what it wants — so the receiving
//! device says so, once, after the content is committed.
//!
//! The answer matters twice. It is what a phone shows the person who shared a
//! photo, and it is the evidence a storage cap consults before dropping a
//! local copy.

use qurb_engine::{Engine, StoreSource};
use qurb_peer::{Fingerprint, Identity, NetworkSource, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const LOOPBACK: &str = "127.0.0.1:0";

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

        let store = Store::open(&store_dir, ChunkKey::from_bytes([42; 32])).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        let identity = Identity::load_or_create(&store_dir).unwrap();

        Self { _dir: dir, root: root.clone(), engine: Engine::new(root, store, ignore), identity }
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        fs::write(self.root.join(rel), contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn outstanding(&self) -> Vec<String> {
        self.engine
            .store()
            .undelivered()
            .unwrap()
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    }
}

/// Teach each device who the other is, as pairing would.
fn introduce(a: &Device, b: &Device) {
    for (one, other) in [(a, b), (b, a)] {
        one.engine
            .store()
            .db()
            .trust_peer(
                &other.engine.store().device_id().unwrap(),
                other.identity.fingerprint().as_bytes(),
                "the other device",
            )
            .unwrap();
    }
}

/// Serve a device, with the folder attached as a real server's store is.
fn serve(device: &Device, allowed: &[Fingerprint]) -> (std::net::SocketAddr, Fingerprint) {
    let store = Store::open(&device.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&device.root);

    let server = PeerServer::bind(
        LOOPBACK.parse().unwrap(),
        &device.identity,
        &qurb_peer::tls::TrustList::new(allowed.to_vec()),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let fingerprint = device.identity.fingerprint();
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });
    (addr, fingerprint)
}

/// The headline: a file this device made is outstanding until someone else
/// takes it, and then it is not.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_stops_being_outstanding_once_a_peer_takes_it() {
    let mut sender = Device::new();
    let mut receiver = Device::new();
    introduce(&sender, &receiver);

    sender.write("photo.jpg", b"pretend this is a photograph");

    assert_eq!(
        sender.outstanding(),
        vec!["photo.jpg".to_string()],
        "a file nobody else has should be outstanding"
    );

    let (addr, fingerprint) = serve(&sender, &[receiver.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &receiver.identity, fingerprint).await.unwrap();

    let tree = client.tree().await.unwrap();
    let plan = receiver.engine.plan_against(&tree).unwrap();
    let reader = Store::open(&receiver.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&receiver.root);
    let mut source = NetworkSource::new(&client, &reader);
    let stats = receiver.engine.apply_plan(&plan, &mut source).unwrap();
    assert_eq!(stats.adopted, 1);
    client.close();

    assert_eq!(
        fs::read(receiver.root.join("photo.jpg")).unwrap(),
        b"pretend this is a photograph"
    );
    assert!(
        sender.outstanding().is_empty(),
        "the sender still thinks the file has not been delivered: {:?}",
        sender.outstanding()
    );
}

/// Delivery is credited to the device the connection proves, not to whatever a
/// message claims. This is the property that makes the record worth trusting.
#[tokio::test(flavor = "multi_thread")]
async fn delivery_is_credited_to_the_authenticated_device() {
    let mut sender = Device::new();
    let receiver = Device::new();
    introduce(&sender, &receiver);

    sender.write("notes.txt", b"something");
    let hash = blake3::hash(b"something");

    let (addr, fingerprint) = serve(&sender, &[receiver.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &receiver.identity, fingerprint).await.unwrap();
    client.got(*hash.as_bytes()).await.unwrap();
    client.close();

    // Recorded, and against the receiver specifically.
    let count = sender.engine.store().db().replica_count(&hash).unwrap();
    assert_eq!(count, 1, "the report was not recorded");
    assert!(sender.outstanding().is_empty());
}

/// A device that only *holds* content it received does not report a backlog.
/// Outstanding means "mine and nobody else's", not "not yet copied to me".
#[tokio::test(flavor = "multi_thread")]
async fn content_received_from_elsewhere_is_not_a_backlog() {
    let mut sender = Device::new();
    let mut receiver = Device::new();
    introduce(&sender, &receiver);

    sender.write("from-sender.bin", b"made over there");

    let (addr, fingerprint) = serve(&sender, &[receiver.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &receiver.identity, fingerprint).await.unwrap();
    let tree = client.tree().await.unwrap();
    let plan = receiver.engine.plan_against(&tree).unwrap();
    let reader = Store::open(&receiver.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&receiver.root);
    receiver
        .engine
        .apply_plan(&plan, &mut NetworkSource::new(&client, &reader))
        .unwrap();
    client.close();

    assert!(
        receiver.outstanding().is_empty(),
        "a device counted content it merely received as its own backlog: {:?}",
        receiver.outstanding()
    );
}

/// Reading from a local store delivers nothing to anyone, so nothing is
/// reported. The default `received` must stay a no-op.
#[test]
fn a_local_source_reports_no_delivery() {
    let mut sender = Device::new();
    let mut receiver = Device::new();
    introduce(&sender, &receiver);

    sender.write("local.bin", b"copied by hand");

    let tree = sender.engine.tree().unwrap();
    let plan = receiver.engine.plan_against(&tree).unwrap();
    receiver
        .engine
        .apply_plan(&plan, &mut StoreSource::new(sender.engine.store()))
        .unwrap();

    assert_eq!(
        sender.outstanding(),
        vec!["local.bin".to_string()],
        "copying out of a local store is not delivery to a device"
    );
}
