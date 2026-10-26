//! Each device's own name and picture, set on it and learned by every device
//! paired with it at a sync, over a real connection (the owner's list of
//! 2026-10-26, item 7). Before this a device was known for good by the name it
//! gave when it paired.

use qurb_peer::{learn_profile, Identity, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use std::sync::{Arc, Mutex};

const KEY: [u8; 32] = [61; 32];

struct Device {
    dir: tempfile::TempDir,
    store: Store,
    identity: Identity,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store_dir = dir.path().join(".qurb");
        let store = Store::open(&store_dir, ChunkKey::from_bytes(KEY)).unwrap();
        let identity = Identity::load_or_create(&store_dir).unwrap();
        Self { dir, store, identity }
    }

    fn id(&self) -> qurb_sync::DeviceId {
        self.store.device_id().unwrap()
    }

    /// Ask `peer` for its profile, as a sync does after its files.
    async fn learn_from(&self, peer: &Device) -> bool {
        let server = PeerServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            &peer.identity,
            &qurb_peer::tls::TrustList::new(vec![self.identity.fingerprint()]),
        )
        .unwrap();
        let addr = server.local_addr().unwrap();
        let store = Store::open(&peer.dir.path().join(".qurb"), ChunkKey::from_bytes(KEY)).unwrap();
        let serving = tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });
        let client = PeerClient::connect(addr, &self.identity, peer.identity.fingerprint()).await.unwrap();
        let learned = learn_profile(&client, &self.store, &peer.id()).await;
        client.close();
        serving.abort();
        learned
    }

    fn name_of(&self, peer: &Device) -> String {
        self.store.db().device_names().unwrap()[&peer.id()].clone()
    }
}

fn introduce(a: &Device, b: &Device) {
    for (one, other) in [(a, b), (b, a)] {
        one.store.db().trust_peer(&other.id(), other.identity.fingerprint().as_bytes(), "Pixel 8").unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_and_picture_set_on_a_device_reach_the_one_paired_with_it() {
    let phone = Device::new();
    let laptop = Device::new();
    introduce(&phone, &laptop);

    // Nothing set: nothing learned, and the pairing name stands.
    assert!(!phone.learn_from(&laptop).await);
    assert_eq!(phone.name_of(&laptop), "Pixel 8");
    assert_eq!(phone.store.db().peer_picture_at(&laptop.id()).unwrap(), None);

    let face = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
    laptop.store.db().set_own_profile(Some("Desk"), Some(Some(&face))).unwrap();
    assert!(phone.learn_from(&laptop).await);
    assert_eq!(phone.name_of(&laptop), "Desk");
    let learned = phone.store.db().peer_profile(&laptop.id()).unwrap().unwrap();
    assert_eq!(learned.picture.as_deref(), Some(&face[..]));
    let first = phone.store.db().peer_picture_at(&laptop.id()).unwrap().unwrap();

    // Asked again with nothing changed: told nothing.
    assert!(!phone.learn_from(&laptop).await);

    // Renamed: the picture stays, its time with it changed.
    laptop.store.db().set_own_profile(Some("Study"), None).unwrap();
    assert!(phone.learn_from(&laptop).await);
    assert_eq!(phone.name_of(&laptop), "Study");
    let renamed = phone.store.db().peer_profile(&laptop.id()).unwrap().unwrap();
    assert_eq!(renamed.picture.as_deref(), Some(&face[..]));
    assert!(phone.store.db().peer_picture_at(&laptop.id()).unwrap().unwrap() > first);

    // The picture taken away: none here either.
    laptop.store.db().set_own_profile(None, Some(None)).unwrap();
    assert!(phone.learn_from(&laptop).await);
    assert_eq!(phone.store.db().peer_picture_at(&laptop.id()).unwrap(), None);
    assert_eq!(phone.name_of(&laptop), "Study");
}

#[test]
fn an_older_profile_changes_nothing() {
    let phone = Device::new();
    let laptop = Device::new();
    introduce(&phone, &laptop);
    let db = phone.store.db();
    let said = |name: &str, changed_at| qurb_storage::db::Profile { name: name.into(), picture: None, changed_at };

    db.set_peer_profile(&laptop.id(), &said("Study", 20)).unwrap();
    db.set_peer_profile(&laptop.id(), &said("Desk", 10)).unwrap();
    assert_eq!(phone.name_of(&laptop), "Study");
    assert_eq!(db.peer_profile(&laptop.id()).unwrap().unwrap().changed_at, 20);

    // A device's own changes are always later than the last, whatever its
    // clock says: one asking with the last one's time must hear of them.
    let first = laptop.store.db().set_own_profile(Some("A"), None).unwrap();
    let second = laptop.store.db().set_own_profile(Some("B"), None).unwrap();
    assert!(second.changed_at > first.changed_at);
}
