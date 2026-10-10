//! Guests: another person's device visiting a computer (decision 0060).
//!
//! A guest holds its own key, so it pairs with a guest invite that gives it
//! nothing of the computer's, and the two are given a secret to meet under.
//! What matters most is what each is then shown: only what was sent to it --
//! never the shared area, however it asks, and never a vault kept for anyone.

use qurb_engine::Engine;
use qurb_peer::{visit, Error, Fingerprint, Identity, Invite, Ours, PairingHost, PeerClient, PeerServer};
use qurb_storage::db::Relation;
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_757_462_400;
const LOOPBACK: &str = "127.0.0.1:0";

/// The computer's person, and the guest: two keys.
static HOST_KEY: std::sync::LazyLock<qurb_keys::MasterKey> =
    std::sync::LazyLock::new(|| qurb_keys::MasterKey::from_bytes([42; 32]));

fn ours() -> &'static Ours<'static> {
    Box::leak(Box::new(Ours { name: "Laptop", kind: "computer", key: &HOST_KEY }))
}

struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    identity: Identity,
    engine: Engine,
    key: [u8; 32],
}

impl Device {
    fn new(key: u8) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");
        let store = Store::open(&store_dir, ChunkKey::from_bytes([key; 32])).unwrap().in_tree(&root);
        let identity = Identity::load_or_create(&store_dir).unwrap();
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        Self { _dir: dir, root: root.clone(), identity, engine: Engine::new(root, store, ignore), key: [key; 32] }
    }

    fn id(&self) -> qurb_sync::DeviceId {
        self.engine.store().device_id().unwrap()
    }

    /// Another handle on this device's store, as the pairing listener and the
    /// server hold one.
    fn handle(&self) -> Arc<Mutex<Store>> {
        let store = Store::open(&self.root.join(".qurb"), ChunkKey::from_bytes(self.key))
            .unwrap()
            .in_tree(&self.root);
        Arc::new(Mutex::new(store))
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        fs::write(self.root.join(rel), contents).unwrap();
        self.engine.reconcile().unwrap();
    }

    fn serve(&self, allowed: &[Fingerprint]) -> SocketAddr {
        let server = PeerServer::bind(
            LOOPBACK.parse().unwrap(),
            &self.identity,
            &qurb_peer::tls::TrustList::new(allowed.to_vec()),
        )
        .unwrap();
        let addr = server.local_addr().unwrap();
        let store = self.handle();
        tokio::spawn(async move { server.serve(store).await });
        addr
    }
}

/// The computer shows a guest code; the guest visits with it. Returns what
/// the computer was asked to approve.
async fn welcome(host: &Device, guest: &Device) -> qurb_peer::Asking {
    let listener = PairingHost::open_for_guest(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();
    let asked: Arc<Mutex<Option<qurb_peer::Asking>>> = Arc::new(Mutex::new(None));
    let seen = asked.clone();
    let store = host.handle();
    let waiting = tokio::spawn(async move {
        listener
            .wait(store, ours(), NOW, move |asking| {
                *seen.lock().unwrap() = Some(asking);
                async { true }
            })
            .await
    });
    visit(&invite, &guest.identity, guest.handle(), "Ammi's phone", "phone", NOW).await.unwrap();
    waiting.await.unwrap().unwrap();
    let asking = asked.lock().unwrap().clone().unwrap();
    asking
}

#[test]
fn a_guest_invite_says_so_and_round_trips() {
    let mut invite = Invite::new(Fingerprint::from_bytes([0x5A; 32]), "10.0.0.7:4000".parse().unwrap(), NOW);
    invite.guest = true;
    let code = invite.encode();
    assert!(code.starts_with("qurbg1-"), "{code}");
    assert_eq!(Invite::parse(&code).unwrap(), invite);
    assert_eq!(Invite::parse(&invite.for_humans()).unwrap(), invite);
}

/// Visited, without the key: each records the other as another person's
/// device, and both hold the same meeting secret.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_visits_without_the_key() {
    let host = Device::new(42);
    let guest = Device::new(7);
    let asking = welcome(&host, &guest).await;
    assert!(asking.guest, "asked to approve as one's own device");
    assert!(!asking.wants_key);

    let on_host = host.engine.store().db().meetings().unwrap();
    let on_guest = guest.engine.store().db().meetings().unwrap();
    assert_eq!(on_host.len(), 1);
    assert_eq!(on_guest.len(), 1);
    assert_eq!(on_host[0].0.device_id, guest.id());
    assert_eq!(on_host[0].0.relation, Relation::Guest);
    assert_eq!(on_guest[0].0.device_id, host.id());
    assert_eq!(on_guest[0].0.relation, Relation::Host);
    assert_eq!(on_host[0].1, on_guest[0].1, "they do not share a meeting secret");
}

/// A guest invite gives nothing of the computer's key, and pairs nobody as
/// one of its person's own devices.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_invite_never_gives_the_key() {
    let host = Device::new(42);
    let newcomer = Device::new(7);
    let listener = PairingHost::open_for_guest(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();
    let store = host.handle();
    let waiting = tokio::spawn(async move {
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            listener.wait(store, ours(), NOW, |_| async { true }),
        )
        .await
    });
    let joined = qurb_peer::join(&invite, &newcomer.identity, "thief", "phone", NOW, |_| {
        panic!("a guest invite gave its key")
    })
    .await;
    assert!(joined.is_err());
    let _ = waiting.await;
    assert!(host.engine.store().db().trusted_peers().unwrap().is_empty());
}

/// And an ordinary invite takes no visitors: a guest is welcomed only on
/// purpose.
#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_invite_refuses_a_visit() {
    let host = Device::new(42);
    let guest = Device::new(7);
    let listener = PairingHost::open(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let mut invite = listener.invite().clone();
    invite.guest = true; // what a guest's device would need to send a visit
    let store = host.handle();
    let waiting = tokio::spawn(async move {
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            listener.wait(store, ours(), NOW, |_| async { true }),
        )
        .await
    });
    let visited = visit(&invite, &guest.identity, guest.handle(), "Ammi's phone", "phone", NOW).await;
    assert!(matches!(visited, Err(Error::PairingRefused)), "{visited:?}");
    let _ = waiting.await;
    assert!(host.engine.store().db().trusted_peers().unwrap().is_empty());
}

/// Shown only what was sent to it, over a real connection: not the shared
/// area, and not a shared file's bytes when asked for them by hash.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_is_shown_only_what_was_sent_to_it() {
    let mut host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;

    host.write("family-photo.jpg", b"the host's own, shared with the host's devices");
    let outgoing = host.root.parent().unwrap().join("for-ammi.txt");
    fs::write(&outgoing, b"for Ammi").unwrap();
    host.engine.store_mut().send_to_vault("for-ammi.txt", &outgoing, &guest.id()).unwrap();

    let addr = host.serve(&[guest.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &guest.identity, host.identity.fingerprint()).await.unwrap();
    let tree = client.tree().await.unwrap();
    let names: Vec<&str> = tree.iter().map(|v| v.path.as_str()).collect();
    assert_eq!(names, vec!["for-ammi.txt"], "shown more than was sent");
    assert!(tree.iter().all(|v| v.area == qurb_sync::Area::Sent));

    let shared = blake3::hash(b"the host's own, shared with the host's devices");
    assert_eq!(client.manifest(*shared.as_bytes()).await.unwrap(), None, "a shared file's chunks offered");
    let sent = blake3::hash(b"for Ammi");
    assert!(client.manifest(*sent.as_bytes()).await.unwrap().is_some(), "what was sent is refused");
    client.close();
}

/// The guest takes only deliveries from the computer, even if something of
/// the computer's shared area were offered to it.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_takes_only_deliveries() {
    let mut host = Device::new(42);
    let mut guest = Device::new(7);
    welcome(&host, &guest).await;
    host.write("family-photo.jpg", b"shared among the host's own devices");
    let outgoing = host.root.parent().unwrap().join("for-ammi.txt");
    fs::write(&outgoing, b"for Ammi").unwrap();
    host.engine.store_mut().send_to_vault("for-ammi.txt", &outgoing, &guest.id()).unwrap();

    // What the host's server shows the guest, and its shared area on top, as
    // though a server had shown it all.
    let mut everything =
        host.engine.store().tree_for(qurb_storage::db::Audience::Guest(&guest.id())).unwrap();
    everything.extend(host.engine.tree().unwrap());
    assert!(everything.iter().any(|v| v.path == "family-photo.jpg"), "setup");
    let plan = guest.engine.plan_with(&everything, Some(&host.id())).unwrap();
    let mut source = qurb_engine::StoreSource::new(host.engine.store());
    let stats = guest.engine.apply_plan(&plan, &mut source).unwrap();
    assert!(stats.failures.is_empty(), "{:?}", stats.failures);

    assert!(!guest.root.join("family-photo.jpg").exists(), "another person's shared file was adopted");
    assert_eq!(fs::read(guest.root.join("for-ammi.txt")).unwrap(), b"for Ammi");
}

/// A guest's copy is another person's: never one this computer can ask for
/// back, so freeing space never counts on it.
#[tokio::test(flavor = "multi_thread")]
async fn a_guests_copy_never_counts_as_one_to_ask_for() {
    let mut host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;
    host.write("photo.jpg", b"only on the host");
    let content = blake3::hash(b"only on the host");
    host.engine.store().db().note_replica(&content, &guest.id()).unwrap();
    assert_eq!(host.engine.store().db().replica_count(&content).unwrap(), 0, "a guest's copy counted");
}
