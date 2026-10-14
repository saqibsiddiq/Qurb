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
        .keep_outside_folder(".qurb-outside/whatsapp", &[photo_entry, backup_entry.clone()], &media, None, &|_| false)
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
        .keep_outside_folder(".qurb-outside/whatsapp", &[newer_entry], &media, None, &|_| false)
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
    phone.engine.store_mut().keep_outside_folder(".qurb-outside/whatsapp", &listing, &|_| true, None, &|_| false).unwrap();
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
    phone.engine.store_mut().keep_outside_folder(".qurb-outside/whatsapp", &listing, &|_| true, None, &|_| false).unwrap();
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

/// Opens a "document" slowly, as a phone's platform does a file in a folder
/// it was given, and counts how many are open at once.
struct SlowDocuments {
    dir: PathBuf,
    delay: std::time::Duration,
}

impl qurb_storage::Documents for SlowDocuments {
    fn open(&self, source: &str) -> Option<std::fs::File> {
        std::thread::sleep(self.delay);
        fs::File::open(self.dir.join(source.strip_prefix("content://slow/")?)).ok()
    }
    fn release(&self, _source: &str) {}
}

/// Requests made at once are read at once (decision 0064).
///
/// Every request used to hold the store's lock while it read the file's
/// bytes, so a phone answered a laptop's 32 requests in turn: at the S23's
/// cost of opening each file, about 25 files a second, which is what the
/// laptop collected its WhatsApp backup at. Eight files that each take 200 ms
/// to open must come in well under the 1.6 s they took one at a time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn requests_made_at_once_are_read_at_once() {
    let mut phone = Device::new();
    phone.engine.store_mut().set_new_files_private(true);
    let desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();
    phone.engine.store().db().set_outside_offered(true).unwrap();

    let lent = phone._dir.path().join("lent");
    let delay = std::time::Duration::from_millis(200);
    let listing: Vec<qurb_storage::OutsideEntry> = (0..8u32)
        .map(|i| {
            let name = format!("Media/{i}.jpg");
            let bytes: Vec<u8> = (0..20_000u32).map(|j| (j.wrapping_mul(31).wrapping_add(i * 7)) as u8).collect();
            fs::create_dir_all(lent.join("Media")).unwrap();
            fs::write(lent.join(&name), &bytes).unwrap();
            qurb_storage::OutsideEntry {
                path: name.clone(),
                source: format!("content://slow/{name}"),
                size: bytes.len() as u64,
                mtime_ns: 1,
            }
        })
        .collect();
    let documents: Arc<dyn qurb_storage::Documents> = Arc::new(SlowDocuments { dir: lent.clone(), delay });
    phone.engine.store_mut().set_documents(Arc::clone(&documents));
    phone.engine.store_mut().keep_outside_folder(".qurb-outside/whatsapp", &listing, &|_| true, None, &|_| false).unwrap();

    let server = PeerServer::bind(
        LOOPBACK.parse().unwrap(),
        &phone.identity,
        &qurb_peer::tls::TrustList::new(vec![desktop.identity.fingerprint()]),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    let mut served = phone.reader();
    served.set_documents(documents);
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(served))).await });

    let client = PeerClient::connect(addr, &desktop.identity, phone.identity.fingerprint()).await.unwrap();
    let mut chunks = Vec::new();
    for version in client.tree().await.unwrap() {
        if let qurb_sync::Content::File { hash, .. } = version.content {
            chunks.extend(client.manifest(hash).await.unwrap().expect("the phone holds it"));
        }
    }
    assert_eq!(chunks.len(), 8, "one chunk a file");

    let t = std::time::Instant::now();
    let got = futures_util::future::join_all(chunks.iter().map(|hash| client.chunk(*hash))).await;
    let took = t.elapsed();
    assert!(got.iter().all(|c| matches!(c, Ok(Some(_)))), "{got:?}");
    assert!(took < delay * 4, "eight files took {took:?}, as if read one at a time");
}

/// A computer keeping a phone's vault is told the phone's folders; one that
/// does not keep it is told nothing (decision 0063).
#[tokio::test(flavor = "multi_thread")]
async fn only_a_computer_keeping_the_vault_sees_a_phones_folders() {
    let phone = Device::new();
    let desktop = Device::new();
    let other = Device::new();
    introduce(&phone, &desktop);
    introduce(&phone, &other);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();
    let folders = vec![
        qurb_storage::db::FolderSummary { path: "DCIM".into(), files: 3, bytes: 9_000 },
        qurb_storage::db::FolderSummary { path: "DCIM/Camera".into(), files: 3, bytes: 9_000 },
    ];
    phone.engine.store().db().set_own_folders(&folders).unwrap();

    let (addr, fingerprint) = phone.serve(desktop.identity.fingerprint());
    let keeper = PeerClient::connect(addr, &desktop.identity, fingerprint).await.unwrap();
    assert_eq!(keeper.folders().await.unwrap().map(|(f, _)| f), Some(folders));

    let (addr, fingerprint) = phone.serve(other.identity.fingerprint());
    let stranger = PeerClient::connect(addr, &other.identity, fingerprint).await.unwrap();
    assert_eq!(stranger.folders().await.unwrap(), None, "a device not keeping the vault was shown its folders");
}

/// A phone asks what was chosen for it, and is told only its own (decision
/// 0063): the device is the one the connection proves.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_is_told_what_was_chosen_for_it_and_nothing_else() {
    let phone = Device::new();
    let tablet = Device::new();
    let desktop = Device::new();
    introduce(&phone, &desktop);
    introduce(&tablet, &desktop);
    let db = desktop.engine.store().db();
    db.choose_folder(&phone.id(), "DCIM/Camera", true).unwrap();
    db.choose_folder(&phone.id(), "Documents", true).unwrap();
    db.choose_folder(&phone.id(), "Documents", false).unwrap();
    db.choose_folder(&tablet.id(), "Download", true).unwrap();

    let (addr, fingerprint) = desktop.serve(phone.identity.fingerprint());
    let asking = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let chosen = |c: Option<Vec<qurb_storage::db::Choice>>| -> Vec<String> {
        c.unwrap().into_iter().filter(|c| c.chosen).map(|c| c.folder).collect()
    };
    assert_eq!(chosen(asking.chosen().await.unwrap()), vec!["DCIM/Camera".to_string()]);

    let (addr, fingerprint) = desktop.serve(tablet.identity.fingerprint());
    let asking = PeerClient::connect(addr, &tablet.identity, fingerprint).await.unwrap();
    assert_eq!(chosen(asking.chosen().await.unwrap()), vec!["Download".to_string()]);
}

/// A folder chosen on the phone reaches the computer when it asks for the
/// phone's folders, and a later change on the computer reaches the phone when
/// it asks what was chosen: the later choice wins wherever it was made
/// (the owner's item 10, 2026-10-14).
#[tokio::test(flavor = "multi_thread")]
async fn a_choice_made_on_either_side_reaches_the_other() {
    let phone = Device::new();
    let desktop = Device::new();
    introduce(&phone, &desktop);
    phone.engine.store().db().add_holder(&desktop.id()).unwrap();
    let (pdb, ddb) = (phone.engine.store().db(), desktop.engine.store().db());

    // Unticked on the computer a while ago; ticked on the phone since.
    ddb.merge_choices_for(&phone.id(), &[qurb_storage::db::Choice { folder: "Documents".into(), chosen: false, changed_at: 1_000 }]).unwrap();
    pdb.choose_here("Documents", true, 2_000).unwrap();

    let (addr, fingerprint) = phone.serve(desktop.identity.fingerprint());
    let asking = PeerClient::connect(addr, &desktop.identity, fingerprint).await.unwrap();
    let (_, choices) = asking.folders().await.unwrap().expect("the computer keeps its vault");
    ddb.merge_choices_for(&phone.id(), &choices).unwrap();
    assert_eq!(ddb.chosen_for(&phone.id()).unwrap(), vec!["Documents".to_string()]);

    // Unticked on the computer now: later than the phone's tick.
    ddb.choose_folder(&phone.id(), "Documents", false).unwrap();
    let (addr, fingerprint) = desktop.serve(phone.identity.fingerprint());
    let asking = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    pdb.merge_choices_here(&asking.chosen().await.unwrap().unwrap()).unwrap();
    assert!(pdb.chosen_here().unwrap().is_empty(), "the later untick did not reach the phone");
}

/// A phone browsing its folder on the computer (decision 0067, step 4): it
/// is shown what the computer keeps for it and for itself before it was set
/// up again, named as the window names them, and can fetch a file of either.
/// Another device of the same person is shown neither, and cannot fetch them
/// by their hash.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_reaches_its_folder_on_the_computer_and_what_it_kept_before() {
    let mut before = Device::new();
    before.engine.store_mut().set_new_files_private(true);
    let mut desktop = Device::new();
    introduce(&before, &desktop);
    before.engine.store().db().add_holder(&desktop.id()).unwrap();
    let diary = b"written before the phone was cleared".repeat(200);
    fs::write(before.root.join("diary.txt"), &diary).unwrap();
    before.engine.reconcile().unwrap();
    desktop.sync_from(&before).await;

    // The same phone, set up again: a new identity of the same name and
    // kind, paired after the old one was last seen.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let phone = Device::new();
    introduce(&phone, &desktop);
    // And a computer of the same person, which is not that phone.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let laptop = Device::new();
    introduce(&laptop, &desktop);
    for (device, kind) in [(&before, "phone"), (&phone, "phone"), (&laptop, "computer")] {
        desktop.engine.store().db().set_peer_kind(&device.id(), kind).unwrap();
    }

    let (addr, fingerprint) = desktop.serve(phone.identity.fingerprint());
    let client = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let (top, _) = client.kept("").await.unwrap().expect("not told");
    assert_eq!(top, vec!["the other one".to_string(), "the other one, earlier".to_string()]);
    let (folders, _) = client.kept("the other one, earlier").await.unwrap().unwrap();
    assert_eq!(folders, vec!["Private Vault".to_string()]);
    let (_, files) = client.kept("the other one, earlier/Private Vault").await.unwrap().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "diary.txt");
    assert_eq!(files[0].owner, *before.id().as_bytes());
    let fetched = client.fetch_content(&phone.reader(), files[0].content, files[0].size).await.unwrap();
    assert_eq!(fetched, diary);
    client.close();
    let content = files[0].content;

    let (addr, fingerprint) = desktop.serve(laptop.identity.fingerprint());
    let client = PeerClient::connect(addr, &laptop.identity, fingerprint).await.unwrap();
    let (top, _) = client.kept("").await.unwrap().unwrap();
    assert_eq!(top, vec!["the other one".to_string()], "shown another device's folder");
    assert!(client.fetch_content(&laptop.reader(), content, diary.len() as u64).await.is_err(), "fetched another device's file");
    client.close();
}
