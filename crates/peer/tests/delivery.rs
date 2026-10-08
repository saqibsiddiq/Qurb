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

/// Content delivered before there was any way to report it must not be
/// counted as undelivered for ever.
///
/// A file both devices already have is never transferred again, so the
/// transfer-time report can never fix it. The holder says so separately.
#[tokio::test(flavor = "multi_thread")]
async fn a_peer_reports_what_it_is_merely_holding() {
    let mut sender = Device::new();
    let mut receiver = Device::new();
    introduce(&sender, &receiver);

    sender.write("old.bin", b"delivered long ago");

    // Both devices hold it, and nobody ever said so -- the state an upgrade
    // leaves behind. Built by copying the plan across without the report.
    let tree = sender.engine.tree().unwrap();
    let plan = receiver.engine.plan_against(&tree).unwrap();
    receiver
        .engine
        .apply_plan(&plan, &mut StoreSource::new(sender.engine.store()))
        .unwrap();

    assert_eq!(
        sender.outstanding(),
        vec!["old.bin".to_string()],
        "setup: the sender should still think nobody has it"
    );

    // Now they sync properly, and the receiver mentions what it is holding.
    let (addr, fingerprint) = serve(&sender, &[receiver.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &receiver.identity, fingerprint).await.unwrap();

    let sender_id = sender.engine.store().device_id().unwrap();
    let told = qurb_peer::report_holdings(&client, receiver.engine.store(), &sender_id, &[], 64).await;
    client.close();

    assert_eq!(told, 1, "the receiver reported nothing");
    assert!(
        sender.outstanding().is_empty(),
        "still counted as undelivered: {:?}",
        sender.outstanding()
    );
}

/// Said once, not on every sweep. A device that has reported a holding does
/// not report it again.
#[tokio::test(flavor = "multi_thread")]
async fn a_holding_is_reported_only_once() {
    let mut sender = Device::new();
    let mut receiver = Device::new();
    introduce(&sender, &receiver);

    sender.write("thing.bin", b"content");
    let tree = sender.engine.tree().unwrap();
    let plan = receiver.engine.plan_against(&tree).unwrap();
    receiver
        .engine
        .apply_plan(&plan, &mut StoreSource::new(sender.engine.store()))
        .unwrap();

    let (addr, fingerprint) = serve(&sender, &[receiver.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &receiver.identity, fingerprint).await.unwrap();
    let sender_id = sender.engine.store().device_id().unwrap();

    let first = qurb_peer::report_holdings(&client, receiver.engine.store(), &sender_id, &[], 64).await;
    let second = qurb_peer::report_holdings(&client, receiver.engine.store(), &sender_id, &[], 64).await;
    client.close();

    assert_eq!(first, 1);
    assert_eq!(second, 0, "the same holding was reported twice");
}

/// A file the other device lists and does not hold -- freed there on the
/// strength of a device that has since lost it -- is recorded as being
/// elsewhere, not fetched and failed on every sync. On 2026-10-07 a phone
/// showed *Didn't finish* for 18 such files every time it synced.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_the_peer_does_not_hold_is_listed_not_failed_every_sync() {
    let mut laptop = Device::new();
    let mut phone = Device::new();
    introduce(&laptop, &phone);

    laptop.write("photo.jpg", b"kept by a phone whose data was cleared");
    laptop.write("here.txt", b"still on the laptop");
    let gone = qurb_sync::DeviceId::from_bytes([7; 32]);
    let photo = blake3::hash(b"kept by a phone whose data was cleared");
    laptop.engine.store().db().note_replica(&photo, &gone).unwrap();
    laptop.engine.store_mut().free_local("photo.jpg").unwrap();

    let (addr, fingerprint) = serve(&laptop, &[phone.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let reader = Store::open(&phone.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&phone.root);

    // As a phone left it: failed on earlier syncs, which said so.
    phone
        .engine
        .store()
        .db()
        .record(qurb_storage::db::Event::Failed, Some("photo.jpg"), None, None, Some("not there"))
        .unwrap();

    let laptop_id = laptop.engine.store().device_id().unwrap();
    let tree = client.tree().await.unwrap();
    let plan = phone.engine.plan_against(&tree).unwrap();
    let mut source = NetworkSource::new(&client, &reader).for_peer(Some(laptop_id));
    let stats = phone.engine.apply_plan(&plan, &mut source).unwrap();
    assert!(stats.failures.is_empty(), "{:?}", stats.failures);
    assert_eq!(fs::read(phone.root.join("here.txt")).unwrap(), b"still on the laptop");
    // And not shown as on the laptop, which made it and said it has it not.
    let listed = phone.engine.store().db().folder_entry("photo.jpg").unwrap().unwrap();
    assert_eq!(listed.availability, qurb_storage::db::Availability::Nowhere);
    // With the time it was made, not 1970.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    assert!((listed.mtime_ns / 1_000_000_000 - now).abs() < 600, "listed as changed at {}", listed.mtime_ns);
    let failed = phone.engine.store().db().activity_for("photo.jpg", 10).unwrap();
    assert!(
        failed.iter().all(|e| e.kind != qurb_storage::db::Event::Failed),
        "the old failures still show: {failed:?}"
    );

    // Listed, not here, and no half-made file left beside it.
    assert!(!phone.root.join("photo.jpg").exists());
    assert_eq!(phone.engine.store().is_materialised("photo.jpg").unwrap(), Some(false));
    assert!(!phone.root.join(".photo.jpg.incoming").exists(), "a staging file was left");

    // And the next sync has nothing to do about it.
    let tree = client.tree().await.unwrap();
    assert!(phone.engine.plan_against(&tree).unwrap().is_empty(), "asked for again");
    client.close();
}

/// Asked for by name, the same file is still a failure: somebody asked for
/// it, and should hear that it did not come.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_asked_for_that_the_peer_does_not_hold_still_fails() {
    let mut laptop = Device::new();
    let mut phone = Device::new();
    introduce(&laptop, &phone);

    laptop.write("photo.jpg", b"freed on the laptop");
    let elsewhere = qurb_sync::DeviceId::from_bytes([7; 32]);
    laptop
        .engine
        .store()
        .db()
        .note_replica(&blake3::hash(b"freed on the laptop"), &elsewhere)
        .unwrap();
    laptop.engine.store_mut().free_local("photo.jpg").unwrap();

    let (addr, fingerprint) = serve(&laptop, &[phone.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let reader = Store::open(&phone.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&phone.root);

    let tree = client.tree().await.unwrap();
    let plan = phone.engine.plan_against(&tree).unwrap();
    phone.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap();
    assert!(phone.engine.store().db().want("photo.jpg").unwrap());

    let plan = phone.engine.plan_with(&tree, None).unwrap();
    let stats = phone.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap();
    assert_eq!(stats.failures.len(), 1, "asked for, and nothing said it did not come");

    // Said once in its history, though it fails again on the next sync.
    let again = phone.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap();
    client.close();
    assert_eq!(again.failures.len(), 1);
    let failed = phone
        .engine
        .store()
        .db()
        .activity_for("photo.jpg", 10)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == qurb_storage::db::Event::Failed)
        .count();
    assert_eq!(failed, 1, "the same failure recorded on every sync");
}

/// A copy this device was told of -- by the device that made the file -- and
/// that device freed since, is asked about once and then not counted
/// (decision 0055). A copy that is there is asked about once and confirmed.
#[tokio::test(flavor = "multi_thread")]
async fn a_copy_recorded_for_a_peer_is_asked_about_once() {
    let mut laptop = Device::new();
    let mut phone = Device::new();
    introduce(&laptop, &phone);
    let laptop_id = laptop.engine.store().device_id().unwrap();

    laptop.write("gone.bin", b"the laptop freed this later");
    laptop.write("kept.bin", b"the laptop still has this");

    // The phone takes both, and records the laptop as holding them, as the
    // device that made them.
    let (addr, fingerprint) = serve(&laptop, &[phone.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let reader = Store::open(&phone.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&phone.root);
    let tree = client.tree().await.unwrap();
    let plan = phone.engine.plan_against(&tree).unwrap();
    phone.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap();

    // Then each frees its copy: the laptop of one, on the strength of a third
    // device; the phone of both, on the strength of the laptop.
    let third = qurb_sync::DeviceId::from_bytes([9; 32]);
    let gone = blake3::hash(b"the laptop freed this later");
    let mut laptop_store = Store::open(&laptop.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&laptop.root);
    laptop_store.db().note_replica(&gone, &third).unwrap();
    laptop_store.free_local("gone.bin").unwrap();
    phone.engine.store_mut().free_local("gone.bin").unwrap();
    phone.engine.store_mut().free_local("kept.bin").unwrap();

    let availability = |phone: &Device, path: &str| {
        phone.engine.store().db().folder_entry(path).unwrap().unwrap().availability
    };
    use qurb_storage::db::Availability;
    assert_eq!(availability(&phone, "gone.bin"), Availability::Elsewhere, "setup");

    let asked = qurb_peer::check_holders(&client, phone.engine.store(), &laptop_id, 16).await;
    let again = qurb_peer::check_holders(&client, phone.engine.store(), &laptop_id, 16).await;
    client.close();

    assert_eq!(asked, 2);
    assert_eq!(again, 0, "asked again about what was settled");
    assert_eq!(availability(&phone, "gone.bin"), Availability::Nowhere);
    assert_eq!(availability(&phone, "kept.bin"), Availability::Elsewhere);
}

/// A device removed and paired again: removing it marked its copies out of
/// reach, and pairing again undid none of it. Asked once, what it holds counts
/// again (decision 0055). On 2026-10-08 a laptop called three files the only
/// copy, minutes after the phone it had removed and paired again synced them.
#[tokio::test(flavor = "multi_thread")]
async fn a_device_paired_again_has_its_copies_counted_again() {
    use qurb_storage::db::Availability;
    let mut laptop = Device::new();
    let mut phone = Device::new();
    introduce(&laptop, &phone);
    laptop.write("photo.jpg", b"on both devices");

    // The phone takes it over the network, and its Got records the copy.
    let (addr, fingerprint) = serve(&laptop, &[phone.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let reader = Store::open(&phone.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&phone.root);
    let tree = client.tree().await.unwrap();
    let plan = phone.engine.plan_against(&tree).unwrap();
    phone.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap();
    client.close();

    let availability = |d: &Device| {
        d.engine.store().db().folder_entry("photo.jpg").unwrap().unwrap().availability
    };
    assert_eq!(availability(&laptop), Availability::Here, "setup");

    let phone_id = phone.engine.store().device_id().unwrap();
    laptop.engine.store_mut().remove_device(&phone_id, "the phone", false).unwrap();
    laptop
        .engine
        .store()
        .db()
        .trust_peer(&phone_id, phone.identity.fingerprint().as_bytes(), "the phone")
        .unwrap();
    assert_eq!(availability(&laptop), Availability::OnlyHere, "paired again, and still marked");

    let (addr, fingerprint) = serve(&phone, &[laptop.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &laptop.identity, fingerprint).await.unwrap();
    let asked = qurb_peer::check_holders(&client, laptop.engine.store(), &phone_id, 16).await;
    let again = qurb_peer::check_holders(&client, laptop.engine.store(), &phone_id, 16).await;
    client.close();

    assert_eq!((asked, again), (1, 0));
    assert_eq!(availability(&laptop), Availability::Here);
}

/// A file made by a device that is no longer paired: the phone's own, made
/// before it was set up again with a new identity. Nobody is left to report
/// holding it -- reports go to a file's maker -- so the phone called a photo
/// the laptop also had the only copy (2026-10-08). Asked once, each copy
/// counts or does not.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_whose_maker_is_gone_is_asked_about_once() {
    use qurb_storage::db::Availability;
    let mut maker = Device::new();
    let mut laptop = Device::new();
    let mut phone = Device::new();
    introduce(&maker, &laptop);
    introduce(&laptop, &phone);
    let laptop_id = laptop.engine.store().device_id().unwrap();

    maker.write("photo.jpg", b"on the laptop and the phone");
    maker.write("gone.bin", b"the laptop freed this later");

    // Each device takes both from the one before it.
    async fn take_all(from: &Device, to: &mut Device) {
        let (addr, fingerprint) = serve(from, &[to.identity.fingerprint()]);
        let client = PeerClient::connect(addr, &to.identity, fingerprint).await.unwrap();
        let reader = Store::open(&to.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
            .unwrap()
            .in_tree(&to.root);
        let tree = client.tree().await.unwrap();
        let plan = to.engine.plan_against(&tree).unwrap();
        to.engine.apply_plan(&plan, &mut NetworkSource::new(&client, &reader)).unwrap();
        client.close();
    }
    take_all(&maker, &mut laptop).await;
    take_all(&laptop, &mut phone).await;

    // The laptop frees one, on the strength of the maker.
    let mut laptop_store = Store::open(&laptop.root.join(".qurb"), ChunkKey::from_bytes([42; 32]))
        .unwrap()
        .in_tree(&laptop.root);
    laptop_store.free_local("gone.bin").unwrap();

    let availability = |d: &Device, path: &str| {
        d.engine.store().db().folder_entry(path).unwrap().unwrap().availability
    };
    assert_eq!(availability(&phone, "photo.jpg"), Availability::OnlyHere, "setup");

    let (addr, fingerprint) = serve(&laptop, &[phone.identity.fingerprint()]);
    let client = PeerClient::connect(addr, &phone.identity, fingerprint).await.unwrap();
    let asked = qurb_peer::check_holders(&client, phone.engine.store(), &laptop_id, 16).await;
    let again = qurb_peer::check_holders(&client, phone.engine.store(), &laptop_id, 16).await;
    client.close();

    assert_eq!((asked, again), (2, 0), "asked again about what was settled");
    assert_eq!(availability(&phone, "photo.jpg"), Availability::Here);
    assert_eq!(availability(&phone, "gone.bin"), Availability::OnlyHere);
}
