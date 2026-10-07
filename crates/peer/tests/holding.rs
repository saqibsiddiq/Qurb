//! Holding a phone's vault, over a real connection.
//!
//! The in-process tests in the engine cover planning. What only a connection
//! shows is the bookkeeping each side does from the other's `Got`: the phone
//! must count the desktop's copy as one it can ask back -- or it can never
//! free anything -- and the desktop must record the phone taking its own file
//! back without a word of it reaching the desktop's history, where the file's
//! name would be shown to whoever uses the desktop. Decision 0036.

use qurb_engine::Engine;
use qurb_peer::{Fingerprint, Identity, NetworkSource, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const LOOPBACK: &str = "127.0.0.1:0";
const KEY: [u8; 32] = [52; 32];

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

    fn reader(&self) -> Store {
        Store::open(&self.root.join(".qurb"), ChunkKey::from_bytes(KEY))
            .unwrap()
            .in_tree(&self.root)
    }

    fn serve(&self, allowed: Fingerprint) -> (std::net::SocketAddr, Fingerprint) {
        let server = PeerServer::bind(
            LOOPBACK.parse().unwrap(),
            &self.identity,
            &qurb_peer::tls::TrustList::new(vec![allowed]),
        )
        .unwrap();
        let addr = server.local_addr().unwrap();
        let store = self.reader();
        tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });
        (addr, self.identity.fingerprint())
    }

    /// Sync from `peer` over a real connection, as the daemon does.
    async fn sync_from(&mut self, peer: &Device) -> qurb_engine::PlanStats {
        let (addr, fingerprint) = peer.serve(self.identity.fingerprint());
        let client = PeerClient::connect(addr, &self.identity, fingerprint).await.unwrap();
        let tree = client.tree().await.unwrap();
        let plan = self.engine.plan_with(&tree, Some(&peer.id())).unwrap();
        let reader = self.reader();
        let stats = tokio::task::block_in_place(|| {
            self.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap()
        });
        client.close();
        assert!(stats.failures.is_empty(), "{:?}", stats.failures);
        stats
    }
}

fn introduce(a: &Device, b: &Device) {
    for (one, other) in [(a, b), (b, a)] {
        one.engine
            .store()
            .db()
            .trust_peer(&other.id(), other.identity.fingerprint().as_bytes(), "the other one")
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_frees_a_photo_the_desktop_holds_and_gets_it_back() {
    let mut phone = Device::new();
    phone.engine.store_mut().set_new_files_private(true);
    let mut desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();

    let photo: Vec<u8> =
        (0..600_000u32).map(|i| (i.wrapping_mul(2_246_822_519) >> 16) as u8).collect();
    fs::write(phone.root.join("IMG_0001.jpg"), &photo).unwrap();
    phone.engine.reconcile().unwrap();

    // The phone's photo, before anybody holds it, is the only copy.
    assert!(phone.engine.store_mut().evict("IMG_0001.jpg").is_err());

    let held = desktop.sync_from(&phone).await;
    assert_eq!(held.held, 1);
    assert!(!desktop.root.join("IMG_0001.jpg").exists());

    // The desktop's Got reached the phone and counts as a copy it can ask
    // back, so the photo can be freed now.
    let candidates: Vec<String> =
        phone.engine.store().evictable().unwrap().into_iter().map(|(p, _, _)| p).collect();
    assert_eq!(candidates, vec!["IMG_0001.jpg".to_string()]);
    phone.engine.store_mut().evict("IMG_0001.jpg").unwrap();
    assert!(!phone.root.join("IMG_0001.jpg").exists());

    // Asked whether it holds the freed photo, the desktop says it does:
    // a phone's own file kept for it is not shown as on no device
    // (decision 0055).
    let (addr, fingerprint) = desktop.serve(phone.identity.fingerprint());
    let client = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let asked = qurb_peer::check_holders(&client, phone.engine.store(), &desktop.id(), 16).await;
    client.close();
    assert_eq!(asked, 1);
    assert_eq!(
        phone.engine.store().db().folder_entry("IMG_0001.jpg").unwrap().unwrap().availability,
        qurb_storage::db::Availability::Elsewhere
    );

    phone.engine.store().db().want("IMG_0001.jpg").unwrap();
    let back = phone.sync_from(&desktop).await;
    assert_eq!(back.adopted, 1);
    assert_eq!(fs::read(phone.root.join("IMG_0001.jpg")).unwrap(), photo, "not byte for byte");

    // Nothing of it on the desktop's screen: no history line, no collection,
    // nothing waiting.
    let history = desktop.engine.store().db().activity(50, None).unwrap();
    assert!(
        history.iter().all(|a| a.path.as_deref() != Some("IMG_0001.jpg")),
        "the phone's photo is named in the desktop's history: {history:?}"
    );
    assert!(desktop.engine.store().pending_deliveries().unwrap().is_empty());

    // And the desktop still has it, for the next time.
    let content = blake3::hash(&photo);
    assert_eq!(desktop.engine.store().read_content(&content).unwrap().unwrap(), photo);
}
