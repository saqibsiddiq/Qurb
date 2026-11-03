//! Removing a device, over a real connection (the owner's review of
//! 2026-11-03, builds 1 to 3): the removed device is told so, whatever it
//! asks; a goodbye tells it at once; and the same phone added again is the
//! device it was, its files here its own again.

use qurb_peer::{Error, Identity, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use std::sync::{Arc, Mutex};

/// A computer serving the store at `dir`, letting in what it accepts.
async fn serve(computer: &Identity, dir: &std::path::Path) -> std::net::SocketAddr {
    let store = Store::open(dir, ChunkKey::from_bytes([3; 32])).unwrap();
    let allowed = qurb_peer::accepted_fingerprints(&store).unwrap();
    let server = PeerServer::bind("127.0.0.1:0".parse().unwrap(), computer, &qurb_peer::tls::TrustList::new(allowed)).unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });
    addr
}

fn open(dir: &std::path::Path) -> Store {
    Store::open(dir, ChunkKey::from_bytes([3; 32])).unwrap()
}

/// A removed phone still connects, and is answered "removed" to everything:
/// it learns, rather than trying for ever.
#[tokio::test(flavor = "multi_thread")]
async fn a_removed_device_is_told_so() {
    let dir = tempfile::tempdir().unwrap();
    let computer = Identity::derived(&[2; 32]).unwrap();
    let phone = Identity::derived(&[11; 32]).unwrap();
    let was = qurb_sync::DeviceId::from_bytes([20; 32]);
    let mut store = open(dir.path());
    store.db().trust_peer(&was, phone.fingerprint().as_bytes(), "skib phone").unwrap();
    store.remove_device(&was, "skib phone", false).unwrap();
    assert!(store.db().trusted_peers().unwrap().is_empty());
    let removed = store.db().removed_devices().unwrap();
    assert_eq!(removed.len(), 1, "not remembered");
    assert!(!removed[0].by_them);

    let addr = serve(&computer, dir.path()).await;
    let client = PeerClient::connect(addr, &phone, computer.fingerprint()).await.expect("a removed device kept out, never told");
    assert!(matches!(client.tree().await, Err(Error::Removed)), "not told it was removed");
    assert!(matches!(client.manifest([1; 32]).await, Err(Error::Removed)));
    client.close();
}

/// A device that never was paired is still refused at the door.
#[tokio::test(flavor = "multi_thread")]
async fn a_stranger_is_still_refused() {
    let dir = tempfile::tempdir().unwrap();
    let computer = Identity::derived(&[2; 32]).unwrap();
    let stranger = Identity::derived(&[13; 32]).unwrap();
    let mut store = open(dir.path());
    let was = qurb_sync::DeviceId::from_bytes([20; 32]);
    store.db().trust_peer(&was, Identity::derived(&[11; 32]).unwrap().fingerprint().as_bytes(), "skib phone").unwrap();
    store.remove_device(&was, "skib phone", false).unwrap();
    let addr = serve(&computer, dir.path()).await;
    let reached = PeerClient::connect(addr, &stranger, computer.fingerprint()).await;
    let refused = match reached {
        Err(_) => true,
        Ok(client) => client.tree().await.is_err(),
    };
    assert!(refused, "a stranger was let in");
}

/// The device that removes another says goodbye: the other forgets it, and
/// remembers it was removed by it.
#[tokio::test(flavor = "multi_thread")]
async fn a_goodbye_is_heard() {
    let dir = tempfile::tempdir().unwrap();
    let phone = Identity::derived(&[11; 32]).unwrap();
    let computer = Identity::derived(&[2; 32]).unwrap();
    let laptop = qurb_sync::DeviceId::from_bytes([40; 32]);
    open(dir.path()).db().trust_peer(&laptop, computer.fingerprint().as_bytes(), "saqib").unwrap();

    // The phone serving, the computer removing it.
    let addr = serve(&phone, dir.path()).await;
    let client = PeerClient::connect(addr, &computer, phone.fingerprint()).await.unwrap();
    client.goodbye().await.unwrap();
    client.close();

    let store = open(dir.path());
    assert!(store.db().trusted_peers().unwrap().is_empty(), "the computer still trusted");
    let removed = store.db().removed_devices().unwrap();
    assert_eq!(removed.len(), 1);
    assert!(removed[0].by_them, "not remembered as removed by it");
    assert_eq!(removed[0].name, "saqib");
    // Not let back in on that account: removed by it is not removed by us.
    assert!(qurb_peer::accepted_fingerprints(&store).unwrap().is_empty());
}

/// A removed phone set up again and added again says its permanent identity
/// and is told the device it was: trusted again under its old id, so that
/// what was kept for it here is its own again.
#[tokio::test(flavor = "multi_thread")]
async fn a_removed_phone_added_again_is_the_device_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let computer = Identity::derived(&[2; 32]).unwrap();
    let (permanent, passing) = (Identity::derived(&[11; 32]).unwrap(), Identity::derived(&[12; 32]).unwrap());
    let (was, joined) = (qurb_sync::DeviceId::from_bytes([20; 32]), qurb_sync::DeviceId::from_bytes([21; 32]));
    let mut store = open(dir.path());
    store.db().trust_peer(&was, permanent.fingerprint().as_bytes(), "skib phone").unwrap();
    store.db().set_peer_kind(&was, "phone").unwrap();
    store.remove_device(&was, "skib phone", false).unwrap();
    // Added again, as a new install: paired under a passing identity.
    store.db().trust_peer(&joined, passing.fingerprint().as_bytes(), "SM-S911B").unwrap();
    store.db().set_peer_kind(&joined, "phone").unwrap();

    let addr = serve(&computer, dir.path()).await;
    let client = PeerClient::connect(addr, &passing, computer.fingerprint()).await.unwrap();
    assert_eq!(client.become_permanent(permanent.cert_bytes()).await.unwrap(), Some(*was.as_bytes()), "not told the device it was");
    client.close();

    let store = open(dir.path());
    let row = store.db().peer_by_fingerprint(permanent.fingerprint().as_bytes()).unwrap().expect("not trusted again");
    assert_eq!(row.device_id, was);
    assert_eq!(store.db().peer_kind(&was).unwrap().as_deref(), Some("phone"));
    assert_eq!(store.db().became(&joined).unwrap(), Some(was));
    assert!(store.db().removed_devices().unwrap().is_empty(), "still remembered as removed");

    // And connected as itself, it is answered, not told it was removed.
    let addr = serve(&computer, dir.path()).await;
    let client = PeerClient::connect(addr, &permanent, computer.fingerprint()).await.unwrap();
    assert!(client.tree().await.is_ok());
    client.close();
}

/// The same install, removed and paired again, keeps its device id: paired
/// again is removed no longer.
#[tokio::test(flavor = "multi_thread")]
async fn the_same_install_paired_again_is_removed_no_longer() {
    let dir = tempfile::tempdir().unwrap();
    let computer = Identity::derived(&[2; 32]).unwrap();
    let (permanent, passing) = (Identity::derived(&[11; 32]).unwrap(), Identity::derived(&[12; 32]).unwrap());
    let was = qurb_sync::DeviceId::from_bytes([20; 32]);
    let mut store = open(dir.path());
    store.db().trust_peer(&was, permanent.fingerprint().as_bytes(), "skib phone").unwrap();
    store.remove_device(&was, "skib phone", false).unwrap();
    store.db().trust_peer(&was, passing.fingerprint().as_bytes(), "skib phone").unwrap();
    assert!(store.db().removed_devices().unwrap().is_empty());

    let addr = serve(&computer, dir.path()).await;
    let client = PeerClient::connect(addr, &passing, computer.fingerprint()).await.unwrap();
    assert_eq!(client.become_permanent(permanent.cert_bytes()).await.unwrap(), Some(*was.as_bytes()));
    client.close();
}
