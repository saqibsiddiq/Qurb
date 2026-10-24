//! A computer's shared folder, reached live from phones (decision 0073, item
//! 6): by its own person's phones and by its guests, one folder at a time,
//! with the files' bytes -- and never by anyone else, nor any folder shared
//! without the asker.

use qurb_engine::Engine;
use qurb_peer::{visit, Fingerprint, Identity, Ours, PairingHost, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_757_462_400;
const LOOPBACK: &str = "127.0.0.1:0";

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

    fn handle(&self) -> Arc<Mutex<Store>> {
        let store = Store::open(&self.root.join(".qurb"), ChunkKey::from_bytes(self.key))
            .unwrap()
            .in_tree(&self.root);
        Arc::new(Mutex::new(store))
    }

    fn write(&mut self, rel: &str, contents: &[u8]) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
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

    /// Trust `other` as one of this device's own person's, of `kind`.
    fn trust_own(&self, other: &Device, name: &str, kind: &str) {
        let db = self.engine.store().db();
        db.trust_peer(&other.id(), other.identity.fingerprint().as_bytes(), name).unwrap();
        db.set_peer_kind(&other.id(), kind).unwrap();
    }
}

/// The computer shows a guest code, and `guest` visits with it.
async fn welcome(host: &Device, guest: &Device) {
    let listener = PairingHost::open_for_guest(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();
    let store = host.handle();
    let waiting = tokio::spawn(async move { listener.wait(store, ours(), NOW, |_| async { true }).await });
    visit(&invite, &guest.identity, guest.handle(), "Ammi's phone", "phone", NOW).await.unwrap();
    waiting.await.unwrap().unwrap();
}

async fn connect(host: &Device, asker: &Device) -> PeerClient {
    let addr = host.serve(&[asker.identity.fingerprint()]);
    PeerClient::connect(addr, &asker.identity, host.identity.fingerprint()).await.unwrap()
}

fn names(files: &[qurb_peer::wire::KeptEntry]) -> Vec<&str> {
    files.iter().map(|f| f.name.as_str()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_guest_reaches_the_shared_folder_live_and_is_never_sent_it() {
    let mut host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;
    host.write("Family/photo.jpg", b"a photo of the family");
    host.write("notes.txt", b"shopping");
    fs::create_dir_all(host.root.join("Empty")).unwrap();

    let client = connect(&host, &guest).await;
    let (folders, files) = client.shared("").await.unwrap().expect("a guest is told");
    assert_eq!(folders, vec!["Empty", "Family"], "an empty folder on disk shows too");
    assert_eq!(names(&files), vec!["notes.txt"]);
    let (_, inside) = client.shared("Family").await.unwrap().unwrap();
    assert_eq!(names(&inside), vec!["photo.jpg"]);
    assert_eq!(inside[0].path, "Family/photo.jpg");
    assert_eq!(inside[0].owner, *host.id().as_bytes());

    // Its bytes, which a guest was refused before.
    let manifest = client.manifest(inside[0].content).await.unwrap().expect("the file's chunks");
    let chunk = client.chunk(manifest[0]).await.unwrap().expect("its first chunk");
    assert_eq!(chunk, b"a photo of the family");

    // Live only: the list a guest syncs still has none of it.
    assert!(client.tree().await.unwrap().is_empty(), "the shared area sent to a guest to sync");
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_shared_without_the_guest_stays_hidden_from_it() {
    let mut host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;
    host.write("Private/salary.txt", b"nobody else's business");
    host.write("open.txt", b"for everyone");
    let other = qurb_sync::DeviceId::from_bytes([9; 32]);
    let rules = BTreeMap::from([("Private".to_string(), BTreeSet::from([other]))]);
    host.engine.store().db().replace_shares(&qurb_sync::sharing::Rules::new(rules), "test").unwrap();

    let client = connect(&host, &guest).await;
    let (folders, files) = client.shared("").await.unwrap().unwrap();
    assert!(folders.is_empty(), "a folder shared without the guest listed: {folders:?}");
    assert_eq!(names(&files), vec!["open.txt"]);
    let (folders, files) = client.shared("Private").await.unwrap().unwrap();
    assert!(folders.is_empty() && files.is_empty(), "opened by name");
    let hidden = blake3::hash(b"nobody else's business");
    assert_eq!(client.manifest(*hidden.as_bytes()).await.unwrap(), None, "its bytes given by hash");
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_owners_phone_reaches_it_and_the_owners_computer_does_not() {
    let mut host = Device::new(42);
    let phone = Device::new(42);
    let computer = Device::new(42);
    host.trust_own(&phone, "Phone", "phone");
    host.trust_own(&computer, "Desktop", "computer");
    host.write("notes.txt", b"shopping");

    let client = connect(&host, &phone).await;
    let (_, files) = client.shared("").await.unwrap().expect("its own person's phone is told");
    assert_eq!(names(&files), vec!["notes.txt"]);
    client.close();

    // Another computer syncs the shared area; it is not served it this way.
    let client = connect(&host, &computer).await;
    assert_eq!(client.shared("").await.unwrap(), None);
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_computer_visited_gets_nothing_of_the_visitors_shared_folder() {
    let host = Device::new(42);
    let mut guest = Device::new(7);
    welcome(&host, &guest).await;
    guest.write("mine.txt", b"the visitor's own");

    // The other way round: the computer asking the device that visits it.
    let client = connect(&guest, &host).await;
    assert_eq!(client.shared("").await.unwrap(), None, "the visitor's shared folder listed to its host");
    let mine = blake3::hash(b"the visitor's own");
    assert_eq!(client.manifest(*mine.as_bytes()).await.unwrap(), None, "the visitor's shared file given");
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_out_of_the_folder_lists_nothing() {
    let mut host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;
    host.write("notes.txt", b"shopping");
    fs::write(host.root.parent().unwrap().join("outside.txt"), b"not in the folder").unwrap();

    let client = connect(&host, &guest).await;
    for dir in ["..", "../sync", ".qurb", "/etc"] {
        match client.shared(dir).await {
            Ok(Some((folders, files))) => {
                assert!(folders.is_empty() && files.is_empty(), "{dir:?} listed {folders:?} {files:?}")
            }
            Ok(None) | Err(_) => {}
        }
    }
    client.close();
}

// ---------------------------------------------------------------- changes

#[tokio::test(flavor = "multi_thread")]
async fn a_guest_adds_renames_moves_and_deletes_in_the_shared_folder() {
    let host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;
    let client = connect(&host, &guest).await;

    assert_eq!(client.shared_mkdir("Trips").await.unwrap(), Ok(()));
    assert!(host.root.join("Trips").is_dir());

    // Three pieces: 9 MiB, at most 4 MiB each.
    let bytes: Vec<u8> = (0..9 << 20).map(|i: u32| (i % 251) as u8).collect();
    let file = host.root.parent().unwrap().join("from-the-phone.jpg");
    fs::write(&file, &bytes).unwrap();
    let mut heard = Vec::new();
    let placed = client.shared_upload(&file, "Trips/beach.jpg", |n| heard.push(n)).await.unwrap();
    assert_eq!(placed, Ok("Trips/beach.jpg".to_string()));
    assert_eq!(heard, vec![4 << 20, 8 << 20, 9 << 20], "progress after each piece");
    assert_eq!(fs::read(host.root.join("Trips/beach.jpg")).unwrap(), bytes);
    let (_, listed) = client.shared("Trips").await.unwrap().unwrap();
    assert_eq!(names(&listed), vec!["beach.jpg"], "an added file not listed");

    // Again under the same name: numbered.
    let placed = client.shared_upload(&file, "Trips/beach.jpg", |_| {}).await.unwrap();
    assert_eq!(placed, Ok("Trips/beach (2).jpg".to_string()));

    assert_eq!(client.shared_move("Trips/beach.jpg", "Trips/sea.jpg").await.unwrap(), Ok(()));
    assert!(host.root.join("Trips/sea.jpg").is_file() && !host.root.join("Trips/beach.jpg").exists());
    assert_eq!(client.shared_move("Trips", "Holidays/Trips").await.unwrap(), Ok(()));
    assert!(host.root.join("Holidays/Trips/sea.jpg").is_file());

    assert_eq!(client.shared_delete("Holidays").await.unwrap(), Ok(()));
    assert!(!host.root.join("Holidays").exists());
    let trash = host.engine.store().recently_deleted().unwrap();
    assert_eq!(trash.len(), 2);
    assert!(trash.iter().all(|t| t.deleted_by == Some(guest.id())), "Recently deleted does not name the guest");
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_refused_says_why() {
    let mut host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;
    host.write("notes.txt", b"shopping");
    let client = connect(&host, &guest).await;

    for path in [".qurb/x", "../out", "/etc"] {
        assert!(client.shared_mkdir(path).await.unwrap().is_err(), "{path:?} made");
    }
    let taken = client.shared_mkdir("notes.txt").await.unwrap().unwrap_err();
    assert!(taken.contains("already called notes.txt"), "{taken}");
    let missing = client.shared_delete("nothing.txt").await.unwrap().unwrap_err();
    assert!(missing.contains("nothing is called"), "{missing}");
    assert!(host.root.join("notes.txt").is_file());
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn only_its_guests_and_its_owners_phones_may_change_it() {
    let mut host = Device::new(42);
    let computer = Device::new(42);
    host.trust_own(&computer, "Desktop", "computer");
    host.write("notes.txt", b"shopping");

    let client = connect(&host, &computer).await;
    let refused = client.shared_delete("notes.txt").await.unwrap().unwrap_err();
    assert!(refused.contains("does not take changes"), "{refused}");
    assert!(host.root.join("notes.txt").is_file(), "deleted for the owner's computer");
    client.close();

    let phone = Device::new(42);
    host.trust_own(&phone, "Phone", "phone");
    let client = connect(&host, &phone).await;
    assert_eq!(client.shared_mkdir("From the phone").await.unwrap(), Ok(()));
    client.close();

    // And a computer visited takes nothing into its visitor's folder.
    let visited = Device::new(42);
    let mut visitor = Device::new(7);
    welcome(&visited, &visitor).await;
    visitor.write("mine.txt", b"the visitor's own");
    let client = connect(&visitor, &visited).await;
    assert!(client.shared_delete("mine.txt").await.unwrap().is_err());
    assert!(visitor.root.join("mine.txt").is_file());
    client.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_search_finds_what_the_guest_may_see_at_any_depth() {
    let mut host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;
    host.write("Trips/2025/beach.jpg", b"sand");
    host.write("Beach umbrella.txt", b"blue");
    host.write("Private/beach-secret.txt", b"nobody else's");
    host.write("notes.txt", b"shopping");
    let other = qurb_sync::DeviceId::from_bytes([9; 32]);
    let rules = BTreeMap::from([("Private".to_string(), BTreeSet::from([other]))]);
    host.engine.store().db().replace_shares(&qurb_sync::sharing::Rules::new(rules), "test").unwrap();

    let client = connect(&host, &guest).await;
    let found = client.search(qurb_peer::wire::Within::Shared, "BEACH").await.unwrap().expect("a guest may search");
    let mut found: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
    found.sort();
    assert_eq!(found, vec!["Beach umbrella.txt", "Trips/2025/beach.jpg"]);
    assert!(client.search(qurb_peer::wire::Within::Shared, "").await.unwrap().unwrap().is_empty());
    // A guest has no folder of its own person's phones here to search.
    assert_eq!(client.search(qurb_peer::wire::Within::Kept, "beach").await.unwrap(), None);
    client.close();
}
