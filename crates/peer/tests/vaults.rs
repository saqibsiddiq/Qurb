//! A device's private vault is private over the wire, not merely on screen.
//!
//! The product model is that other devices may send content into a device's
//! vault and may not read it back. That has to be a property of the protocol:
//! a user interface choosing not to draw a listing changes nothing about what
//! a peer can ask for, and a peer can ask for content by hash without ever
//! looking at a tree.
//!
//! These tests are the difference between the two.

use qurb_engine::Engine;
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

    fn device_id(&self) -> qurb_sync::DeviceId {
        self.engine.store().device_id().unwrap()
    }
}

/// Teach each device who the other is, as pairing would.
fn introduce(a: &Device, b: &Device) {
    for (one, other) in [(a, b), (b, a)] {
        one.engine
            .store()
            .db()
            .trust_peer(
                &other.device_id(),
                other.identity.fingerprint().as_bytes(),
                "the other device",
            )
            .unwrap();
    }
}

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

/// The default has not changed: a file nobody scoped is shared, and syncing
/// works exactly as it did. Everything else here would be worthless if this
/// broke.
#[tokio::test(flavor = "multi_thread")]
async fn the_shared_area_still_converges() {
    let mut host = Device::new();
    let mut guest = Device::new();
    introduce(&host, &guest);

    host.write("shared.txt", b"everybody sees this");

    let (addr, fingerprint) = serve(&host, &[guest.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &guest.identity, fingerprint).await.unwrap();
    let tree = client.tree().await.unwrap();

    assert_eq!(
        tree.iter().map(|v| v.path.as_str()).collect::<Vec<_>>(),
        vec!["shared.txt"],
        "a file in the shared area should be advertised"
    );

    let plan = guest.engine.plan_against(&tree).unwrap();
    let reader = Store::open(&guest.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&guest.root);
    let stats = guest.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap();
    client.close();

    assert_eq!(stats.adopted, 1);
    assert_eq!(fs::read(guest.root.join("shared.txt")).unwrap(), b"everybody sees this");
}

/// The headline. A vault is not advertised to anybody but its owner.
#[tokio::test(flavor = "multi_thread")]
async fn another_devices_vault_is_not_in_the_tree() {
    let mut host = Device::new();
    let guest = Device::new();
    let third = Device::new();
    introduce(&host, &guest);
    introduce(&host, &third);

    host.write("shared.txt", b"everybody");
    host.write("secret.txt", b"only the third device may have this");
    // The host is holding this on the *third* device's behalf.
    host.engine
        .store()
        .set_scope("secret.txt", Some(&third.device_id()))
        .unwrap();

    let (addr, fingerprint) = serve(&host, &[guest.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &guest.identity, fingerprint).await.unwrap();
    let tree = client.tree().await.unwrap();
    client.close();

    let paths: Vec<&str> = tree.iter().map(|v| v.path.as_str()).collect();
    assert_eq!(paths, vec!["shared.txt"], "another device's vault leaked into the tree");
}

/// And the part a listing could never have protected: asking for the bytes
/// directly. A hash is a name, and a peer can name content it was never shown.
#[tokio::test(flavor = "multi_thread")]
async fn another_devices_vault_cannot_be_fetched_by_hash() {
    let mut host = Device::new();
    let guest = Device::new();
    let third = Device::new();
    introduce(&host, &guest);
    introduce(&host, &third);

    let secret = b"only the third device may have this";
    host.write("secret.txt", secret);
    host.engine
        .store()
        .set_scope("secret.txt", Some(&third.device_id()))
        .unwrap();

    // The guest knows the content hash — it is just a hash of the bytes, and a
    // guest that once saw the file, or guessed, would have it.
    let content = blake3::hash(secret);
    let chunks = host
        .engine
        .store()
        .chunk_hashes_for_content(&content)
        .unwrap()
        .expect("the host holds it");

    let (addr, fingerprint) = serve(&host, &[guest.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &guest.identity, fingerprint).await.unwrap();

    assert!(
        client.manifest(*content.as_bytes()).await.unwrap().is_none(),
        "the manifest for another device's vault was served"
    );
    for chunk in &chunks {
        assert!(
            client.chunk(*chunk.as_bytes()).await.unwrap().is_none(),
            "a chunk of another device's vault was served"
        );
    }
    client.close();
}

/// The owner of a vault can read it. "Private" means private from others, not
/// write-only — a device must be able to get its own data back.
#[tokio::test(flavor = "multi_thread")]
async fn a_device_can_read_its_own_vault() {
    let mut host = Device::new();
    let owner = Device::new();
    introduce(&host, &owner);

    let mine = b"the owner may have this";
    host.write("mine.txt", mine);
    host.engine.store().set_scope("mine.txt", Some(&owner.device_id())).unwrap();

    let (addr, fingerprint) = serve(&host, &[owner.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &owner.identity, fingerprint).await.unwrap();

    let tree = client.tree().await.unwrap();
    assert!(
        tree.iter().any(|v| v.path == "mine.txt"),
        "a device could not see its own vault: {:?}",
        tree.iter().map(|v| &v.path).collect::<Vec<_>>()
    );

    let content = blake3::hash(mine);
    assert!(
        client.manifest(*content.as_bytes()).await.unwrap().is_some(),
        "a device could not resolve its own content"
    );
    client.close();
}

/// Content that is *also* in the shared area is not hidden by a vault copy.
/// Refusing it would deny somebody bytes they can obtain another way, and
/// deduplication means one payload often serves both.
#[tokio::test(flavor = "multi_thread")]
async fn shared_content_stays_reachable_even_if_a_vault_holds_it_too() {
    let mut host = Device::new();
    let guest = Device::new();
    let third = Device::new();
    introduce(&host, &guest);
    introduce(&host, &third);

    let bytes = b"the very same bytes";
    host.write("shared-copy.txt", bytes);
    host.write("vault-copy.txt", bytes);
    host.engine.store().set_scope("vault-copy.txt", Some(&third.device_id())).unwrap();

    let content = blake3::hash(bytes);
    let (addr, fingerprint) = serve(&host, &[guest.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &guest.identity, fingerprint).await.unwrap();

    assert!(
        client.manifest(*content.as_bytes()).await.unwrap().is_some(),
        "content in the shared area was refused because a vault also held it"
    );
    client.close();
}
