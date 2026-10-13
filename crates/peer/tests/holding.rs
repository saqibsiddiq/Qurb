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

/// A shared file moved into the phone's Private Vault leaves the desktop's
/// folder, and the desktop, keeping that vault, holds it still: moving a
/// file into private does not make it the only copy (decision 0057).
#[tokio::test(flavor = "multi_thread")]
async fn a_file_moved_into_the_vault_is_kept_by_the_device_holding_it() {
    let mut phone = Device::new();
    let mut desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();

    fs::write(phone.root.join("notes.txt"), b"shared, then private").unwrap();
    phone.engine.reconcile().unwrap();
    desktop.sync_from(&phone).await;
    assert!(desktop.root.join("notes.txt").exists());

    assert!(phone.engine.store_mut().move_area("notes.txt", true).unwrap());
    desktop.sync_from(&phone).await;

    assert!(!desktop.root.join("notes.txt").exists(), "still in the desktop's folder");
    let content = blake3::hash(b"shared, then private");
    assert!(
        desktop.engine.store().db().holds_for(&content, &phone.id()).unwrap(),
        "the desktop does not hold the phone's file for it"
    );
    assert_eq!(
        desktop.engine.store().read_content(&content).unwrap().as_deref(),
        Some(&b"shared, then private"[..]),
    );
}

/// WhatsApp's folder kept in the phone's vault where it lies (decision
/// 0062): the desktop holds every file with nothing copied on the phone; a
/// photo WhatsApp deleted stays on the desktop; a chat backup WhatsApp
/// replaced goes from it.
#[tokio::test(flavor = "multi_thread")]
async fn a_folder_kept_where_it_lies_is_held_by_the_desktop() {
    let mut phone = Device::new();
    phone.engine.store_mut().set_new_files_private(true);
    let mut desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();
    // On Wi-Fi, as the app tells the engine: offered (decision 0062).
    phone.engine.store().db().set_outside_offered(true).unwrap();

    let whatsapp = phone._dir.path().join("whatsapp");
    let photo: Vec<u8> = (0..600_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    let backup: Vec<u8> = (0..300_000u32).map(|i| (i.wrapping_mul(40_503) >> 7) as u8).collect();
    let entry = |relative: &str, bytes: &[u8]| {
        let path = whatsapp.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        qurb_storage::OutsideEntry {
            path: relative.to_string(),
            source: path.to_string_lossy().into_owned(),
            size: bytes.len() as u64,
            mtime_ns: 1,
        }
    };
    let photo_entry = entry("Media/WhatsApp Images/IMG-1.jpg", &photo);
    let backup_entry = entry("Databases/msgstore.db.crypt14", &backup);
    let media = |path: &str| path.starts_with("Media/");
    let report = phone
        .engine
        .store_mut()
        .keep_outside_folder(".qurb-outside/whatsapp", &[photo_entry, backup_entry.clone()], &media, None)
        .unwrap();
    assert_eq!(report.added, 2);

    // The scan of the phone's own folder does not take them for deletions.
    phone.engine.reconcile().unwrap();
    assert_eq!(phone.engine.store().db().outside_under(".qurb-outside/whatsapp").unwrap().len(), 2);

    let held = desktop.sync_from(&phone).await;
    assert_eq!(held.held, 2);
    assert_eq!(phone.engine.store().usage().unwrap().chunks, 0, "the phone stored a copy");
    let on_desktop = |desktop: &Device, bytes: &[u8]| {
        desktop.engine.store().read_content(&blake3::hash(bytes)).unwrap()
    };
    assert_eq!(on_desktop(&desktop, &photo).unwrap(), photo);
    assert_eq!(on_desktop(&desktop, &backup).unwrap(), backup);
    assert!(!desktop.root.join(".qurb-outside/whatsapp").exists(), "shown on the desktop");

    // WhatsApp deletes the photo, and replaces the backup with a new one.
    fs::remove_file(whatsapp.join("Media/WhatsApp Images/IMG-1.jpg")).unwrap();
    fs::remove_file(whatsapp.join("Databases/msgstore.db.crypt14")).unwrap();
    let newer: Vec<u8> = backup.iter().map(|b| b.wrapping_add(1)).collect();
    let newer_entry = entry("Databases/msgstore-increment-1.db.crypt14", &newer);
    let report = phone
        .engine
        .store_mut()
        .keep_outside_folder(".qurb-outside/whatsapp", &[newer_entry], &media, None)
        .unwrap();
    assert_eq!((report.kept, report.deleted, report.added), (1, 1, 1));

    desktop.sync_from(&phone).await;
    let phone_id = phone.id();
    let kept = |path: &str| {
        desktop
            .engine
            .store()
            .db()
            .live_row_in(&format!(".qurb-outside/whatsapp/{path}"), Some(&phone_id))
            .unwrap()
            .is_some()
    };
    assert!(kept("Media/WhatsApp Images/IMG-1.jpg"), "the photo WhatsApp deleted went from the desktop");
    assert_eq!(on_desktop(&desktop, &photo).unwrap(), photo);
    assert!(!kept("Databases/msgstore.db.crypt14"), "the replaced backup is still kept");
    assert!(kept("Databases/msgstore-increment-1.db.crypt14"));
    assert_eq!(on_desktop(&desktop, &newer).unwrap(), newer);
}

/// Not a test: how long the desktop takes to collect many small files of a
/// folder kept where it lies, over a connection on this machine.
/// `cargo test --release -p qurb-peer --test holding timing -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn timing_collecting_many_small_files() {
    let mut phone = Device::new();
    phone.engine.store_mut().set_new_files_private(true);
    let mut desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();
    phone.engine.store().db().set_outside_offered(true).unwrap();

    let whatsapp = phone._dir.path().join("whatsapp");
    let listing: Vec<qurb_storage::OutsideEntry> = (0..2000u32)
        .map(|i| {
            let path = whatsapp.join(format!("Media/{i:05}.jpg"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let bytes: Vec<u8> = (0..150_000u32).map(|j| (j.wrapping_mul(2_654_435_761).wrapping_add(i * 7919) >> 13) as u8).collect();
            fs::write(&path, &bytes).unwrap();
            qurb_storage::OutsideEntry { path: format!("Media/{i:05}.jpg"), source: path.to_string_lossy().into_owned(), size: bytes.len() as u64, mtime_ns: 1 }
        })
        .collect();
    let t = std::time::Instant::now();
    phone.engine.store_mut().keep_outside_folder(".qurb-outside/whatsapp", &listing, &|_| true, None).unwrap();
    println!("indexing: {:.2} ms per file", t.elapsed().as_secs_f64() * 1000.0 / 2000.0);

    let t = std::time::Instant::now();
    let held = desktop.sync_from(&phone).await;
    println!("collecting {} files: {:.2} ms per file", held.held, t.elapsed().as_secs_f64() * 1000.0 / held.held as f64);
}

/// A file kept for another device that cannot be fetched -- the phone went
/// away part-way -- is not named in this device's history (decision 0062).
#[tokio::test(flavor = "multi_thread")]
async fn a_held_file_that_fails_is_not_in_the_history() {
    let mut phone = Device::new();
    phone.engine.store_mut().set_new_files_private(true);
    let mut desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();
    phone.engine.store().db().set_outside_offered(true).unwrap();

    let whatsapp = phone._dir.path().join("whatsapp");
    let path = whatsapp.join("Media/gone.jpg");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, vec![7u8; 90_000]).unwrap();
    let listing = [qurb_storage::OutsideEntry {
        path: "Media/gone.jpg".into(),
        source: path.to_string_lossy().into_owned(),
        size: 90_000,
        mtime_ns: 1,
    }];
    phone.engine.store_mut().keep_outside_folder(".qurb-outside/whatsapp", &listing, &|_| true, None).unwrap();
    // Changed where it lies before the desktop collects it: not servable.
    fs::write(&path, vec![8u8; 90_000]).unwrap();

    let (addr, fingerprint) = phone.serve(desktop.identity.fingerprint());
    let client = PeerClient::connect(addr, &desktop.identity, fingerprint).await.unwrap();
    let tree = client.tree().await.unwrap();
    let plan = desktop.engine.plan_with(&tree, Some(&phone.id())).unwrap();
    let reader = desktop.reader();
    let stats = tokio::task::block_in_place(|| {
        desktop.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap()
    });
    client.close();
    assert_eq!(stats.failures.len(), 1, "setup: the fetch should fail");
    let history = desktop.engine.store().db().activity(50, None).unwrap();
    assert!(history.iter().all(|a| a.path.as_deref().is_none_or(|p| !p.contains("gone.jpg"))), "{history:?}");
}
