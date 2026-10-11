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

/// Take what `from` offers `into`, over a real connection, as a sync would:
/// `into` plans with `from` as the peer, then tells it what it took.
async fn sync_from(into: &mut Device, from: &Device) {
    let addr = from.serve(&[into.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &into.identity, from.identity.fingerprint()).await.unwrap();
    let reader = Store::open(&into.root.join(".qurb"), ChunkKey::from_bytes(into.key))
        .unwrap()
        .in_tree(&into.root);
    let tree = client.tree().await.unwrap();
    let plan = into.engine.plan_with(&tree, Some(&from.id())).unwrap();
    let stats = into.engine.apply_plan(&plan, &mut qurb_peer::NetworkSource::new(&client, &reader)).unwrap();
    assert!(stats.failures.is_empty(), "{:?}", stats.failures);
    qurb_peer::report_holdings(&client, &reader, &from.id(), &tree, 64).await;
    client.close();
}

/// A guest's Private Vault kept by the computer it visits, which cannot read
/// it (decision 0060, step 3): the computer holds a sealed name and sealed
/// bytes, never the name or a byte of the plain text, and the guest learns
/// the file is kept, so it can free its own copy.
#[tokio::test(flavor = "multi_thread")]
async fn a_guests_vault_is_kept_sealed() {
    let mut host = Device::new(42);
    let mut guest = Device::new(7);
    welcome(&host, &guest).await;

    guest.engine.store_mut().set_new_files_private(true);
    let secret = b"Ammi's recipe for nihari, which nobody else may read".repeat(5000);
    guest.write("recipes/nihari.txt", &secret);
    guest.engine.store().db().add_holder(&host.id()).unwrap();

    sync_from(&mut host, &guest).await;

    let kept: Vec<qurb_sync::FileVersion> = host
        .engine
        .store()
        .tree_for(qurb_storage::db::Audience::Guest(&guest.id()))
        .unwrap()
        .into_iter()
        .filter(|v| v.area == qurb_sync::Area::Held)
        .collect();
    assert_eq!(kept.len(), 1, "the guest's file is not kept");
    let name = &kept[0].path;
    assert!(qurb_storage::sealed::is_sealed_name(name), "kept under its real name: {name}");
    assert!(!name.contains("nihari") && !name.contains("recipes"), "{name}");

    let qurb_sync::Content::File { hash: sealed, .. } = kept[0].content else { panic!("a tombstone") };
    assert_ne!(sealed, *blake3::hash(&secret).as_bytes(), "the plain text's hash was shown");
    let stored = host.engine.store().read_content(&blake3::Hash::from(sealed)).unwrap().unwrap();
    assert!(!stored.windows(32).any(|w| secret.windows(32).next() == Some(w)), "plain text is stored");

    // Told it is kept, the guest counts the computer's copy as one it can
    // fetch back -- and may free its own.
    let content = blake3::hash(&secret);
    assert_eq!(guest.engine.store().db().replica_count(&content).unwrap(), 1, "the guest does not know it is kept");
}

/// Never a guest's files to a computer it visits unless it chose that: a
/// phone's vault goes to the first computer it pairs with by default, and a
/// computer it visits is not one of its own (decision 0060).
#[tokio::test(flavor = "multi_thread")]
async fn a_computer_visited_is_not_a_keeper_by_default() {
    let host = Device::new(42);
    let guest = Device::new(7);
    guest.engine.store().db().set_local_kind("phone").unwrap();
    welcome(&host, &guest).await;
    assert!(guest.engine.store().db().holders().unwrap().is_empty(), "made a keeper without being chosen");
}

/// Kept by the computer, the guest lets go of its own copy -- it chose to keep
/// nothing on the phone -- and gets the file back, unsealed and checked, when
/// it is wanted (decision 0060, step 3).
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_frees_its_copy_and_fetches_it_back() {
    let mut host = Device::new(42);
    let mut guest = Device::new(7);
    welcome(&host, &guest).await;
    guest.engine.store_mut().set_new_files_private(true);
    let secret = b"a photo of the family, kept on the laptop".repeat(20_000);
    guest.write("photos/family.jpg", &secret);
    guest.engine.store().db().add_holder(&host.id()).unwrap();
    sync_from(&mut host, &guest).await;

    guest.engine.housekeep(std::time::Duration::from_secs(0)).unwrap();
    assert!(!guest.root.join("photos/family.jpg").exists(), "the phone kept its copy");
    assert_eq!(guest.engine.store().is_materialised("photos/family.jpg").unwrap(), Some(false));

    // When the vault knew it changed: what it should be dated once back, not
    // when it arrived -- two seconds later, so the two can be told apart.
    let changed = guest.engine.store().db().own_vault_rows().unwrap()[0].1.modified_at;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    guest.engine.store().db().want("photos/family.jpg").unwrap();
    let back = fetch_back(&mut guest, &host).await;
    assert_eq!(back, 1);
    assert_eq!(fs::read(guest.root.join("photos/family.jpg")).unwrap(), secret);
    assert_eq!(guest.engine.store().is_materialised("photos/family.jpg").unwrap(), Some(true));
    let dated = fs::metadata(guest.root.join("photos/family.jpg")).unwrap().modified().unwrap();
    let dated = dated.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    assert_eq!(dated, changed, "came back dated when it arrived, not when it changed");

    // Fetched to be opened, it stays a while: housekeeping straight after
    // used to let go of it again before anyone could open it.
    guest.engine.housekeep(std::time::Duration::from_secs(0)).unwrap();
    assert!(guest.root.join("photos/family.jpg").exists(), "let go of the moment it came back");
}

/// A phone set up again with the same key -- from Block Store or a code --
/// visits as a new device and finds the folder it had: kept under the
/// person, listed from the computer, and fetched back (decision 0060).
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_set_up_again_finds_its_folder() {
    let mut host = Device::new(42);
    let mut old = Device::new(7);
    welcome(&host, &old).await;
    old.engine.store_mut().set_new_files_private(true);
    let secret = b"the only copy of something".repeat(10_000);
    old.write("letters/to-ammi.txt", &secret);
    old.engine.store().db().add_holder(&host.id()).unwrap();
    sync_from(&mut host, &old).await;

    // The same person's key, a new device.
    let mut again = Device::new(7);
    assert_ne!(again.id(), old.id());
    welcome(&host, &again).await;
    again.engine.store().db().add_holder(&host.id()).unwrap();

    let addr = host.serve(&[again.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &again.identity, host.identity.fingerprint()).await.unwrap();
    let tree = client.tree().await.unwrap();
    let learned = qurb_peer::learn_kept(&client, again.engine.store_mut(), &host.id(), &tree).await;
    client.close();
    assert_eq!(learned, 1, "the folder was not found");
    assert_eq!(again.engine.store().is_materialised("letters/to-ammi.txt").unwrap(), Some(false));

    again.engine.store().db().want("letters/to-ammi.txt").unwrap();
    assert_eq!(fetch_back(&mut again, &host).await, 1);
    assert_eq!(fs::read(again.root.join("letters/to-ammi.txt")).unwrap(), secret);
}

/// Fetch back what `device` wants of what `host` keeps for it.
async fn fetch_back(device: &mut Device, host: &Device) -> usize {
    let addr = host.serve(&[device.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &device.identity, host.identity.fingerprint()).await.unwrap();
    let tree = client.tree().await.unwrap();
    let back = qurb_peer::fetch_kept(&client, device.engine.store_mut(), &host.id(), &tree).await;
    client.close();
    back
}

/// Two devices of one guest share its folder. Removing one leaves the folder
/// to the other; removing the last can delete it, when asked (decision 0060).
#[tokio::test(flavor = "multi_thread")]
async fn a_guests_folder_goes_only_with_its_last_device() {
    let mut host = Device::new(42);
    let mut phone = Device::new(7);
    let tablet = Device::new(7);
    welcome(&host, &phone).await;
    welcome(&host, &tablet).await;
    phone.engine.store_mut().set_new_files_private(true);
    phone.write("diary.txt", &b"kept for the person, not the phone".repeat(1000));
    phone.engine.store().db().add_holder(&host.id()).unwrap();
    sync_from(&mut host, &phone).await;

    let person = host.engine.store().db().person_of(&phone.id()).unwrap().unwrap();
    assert!(host.engine.store().db().kept_for_person(&person).unwrap() > 0, "setup");

    let plan = host.engine.store_mut().remove_device(&phone.id(), "phone", true).unwrap();
    assert!(plan.kept_for_it.is_empty(), "offered to delete a folder the tablet still uses");
    assert!(host.engine.store().db().kept_for_person(&person).unwrap() > 0, "the tablet's folder went");

    let plan = host.engine.store_mut().remove_device(&tablet.id(), "tablet", true).unwrap();
    assert_eq!(plan.kept_for_it.len(), 1);
    assert_eq!(host.engine.store().db().kept_for_person(&person).unwrap(), 0, "the folder stayed");
}

/// Opened at the computer with the guest's approval (decision 0060, step 5):
/// the computer asks, the guest's phone collects the ask and answers with the
/// folder's key, and the computer can read the folder until it is locked.
/// A key that does not fit opens nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_guests_folder_opens_at_the_computer_only_with_its_key() {
    let mut host = Device::new(42);
    let mut guest = Device::new(7);
    welcome(&host, &guest).await;
    guest.engine.store_mut().set_new_files_private(true);
    let letter = b"dear Saqib, open this only with me there".repeat(3000);
    guest.write("letters/saqib.txt", &letter);
    guest.engine.store().db().add_holder(&host.id()).unwrap();
    sync_from(&mut host, &guest).await;

    let person = host.engine.store().db().person_of(&guest.id()).unwrap().unwrap();
    assert!(!qurb_peer::openings::is_open(&person));
    qurb_peer::openings::ask(&person);

    let addr = host.serve(&[guest.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &guest.identity, host.identity.fingerprint()).await.unwrap();
    let nonce = client.asks().await.unwrap().expect("the ask did not reach the guest");
    assert!(!client.unlock(nonce, [0x55; 32]).await.unwrap(), "opened with a key that does not fit");
    let key = qurb_storage::sealed::FolderKey::for_host(&guest.engine.store().chunk_key(), &host.id());
    assert!(client.unlock(nonce, key.to_bytes()).await.unwrap(), "the right key did not open it");
    client.close();

    let held = qurb_storage::sealed::FolderKey::from_bytes(qurb_peer::openings::key(&person).unwrap());
    let files = host.engine.store().open_kept(&person, &held).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "letters/saqib.txt");
    assert_eq!(files[0].size, letter.len() as u64);
    let out = host.root.parent().unwrap().join("opened.txt");
    let sealed = host.engine.store().kept_file(&person, &held, "letters/saqib.txt").unwrap();
    assert_eq!(sealed, Some(files[0].sealed), "found by its sealed name");
    assert!(
        host.engine.store().unseal_kept(&held, &files[0].sealed, "letters/other.txt", &out).is_err(),
        "unsealed as a file it is not"
    );
    host.engine.store().unseal_kept(&held, &files[0].sealed, &files[0].path, &out).unwrap();
    assert_eq!(fs::read(&out).unwrap(), letter);

    qurb_peer::openings::lock(&person);
    assert!(qurb_peer::openings::key(&person).is_none(), "the key outlived the lock");
}

/// Decision 0061: a guest and the computer it visits, reaching each other
/// through the relay by their meeting's names -- the way a computer reaches a
/// guest's phone that it cannot dial, behind a carrier's NAT. Both ways, each
/// answered under the name it was called by.
#[tokio::test(flavor = "multi_thread")]
async fn a_guest_and_its_computer_reach_each_other_through_the_relay() {
    let signal = Arc::new(qurb_signal::SignalServer::bind(LOOPBACK.parse().unwrap()).await.unwrap());
    let signal_url = format!("ws://{}", signal.local_addr().unwrap());
    let running = Arc::clone(&signal);
    tokio::spawn(async move { running.serve().await });
    let relay = Arc::new(qurb_relay::RelayServer::bind(LOOPBACK.parse().unwrap()).await.unwrap());
    let relay_addr = relay.local_addr().unwrap();
    let serving = Arc::clone(&relay);
    tokio::spawn(async move { serving.serve().await });

    let host = Device::new(42);
    let guest = Device::new(7);
    welcome(&host, &guest).await;

    // Each with both ways in, served, and meeting the other under its secret.
    let mut connectors = Vec::new();
    for device in [&host, &guest] {
        let store = device.engine.store();
        let allowed = qurb_peer::trusted_fingerprints(store).unwrap();
        let connector = qurb_peer::Connector::start(
            LOOPBACK.parse().unwrap(),
            device.identity.clone(),
            qurb_keys::MasterKey::from_bytes(device.key),
            &qurb_peer::tls::TrustList::new(allowed),
            signal_url.clone(),
            qurb_peer::Finding { stun: false, beacons: None, relay: Some(relay_addr) },
        )
        .await
        .unwrap();
        for endpoint in [Some(connector.endpoint().clone()), connector.relay_endpoint().cloned()].into_iter().flatten() {
            let store = device.handle();
            tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let store = Arc::clone(&store);
                    tokio::spawn(async move {
                        if let Ok(connection) = incoming.await {
                            qurb_peer::server::serve_connection_for_test(connection, store).await;
                        }
                    });
                }
            });
        }
        for (peer, secret) in store.db().meetings().unwrap() {
            connector.meet(Fingerprint::from_bytes(peer.fingerprint), secret);
        }
        connectors.push(connector);
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let (host_conn, guest_conn) = (&connectors[0], &connectors[1]);

    let to_guest = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        host_conn.reach_via_relay(guest.identity.fingerprint()),
    )
    .await
    .expect("reaching the guest through the relay timed out")
    .expect("the computer could not reach its guest through the relay");
    assert!(to_guest.is_relayed());
    to_guest.tree().await.expect("a request to the guest, relayed");

    let to_host = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        guest_conn.reach_via_relay(host.identity.fingerprint()),
    )
    .await
    .expect("reaching the computer through the relay timed out")
    .expect("the guest could not reach its computer through the relay");
    assert!(to_host.is_relayed());
    to_host.tree().await.expect("a request to the computer, relayed");
    assert!(relay.stats().forwarded() > 0);
}
