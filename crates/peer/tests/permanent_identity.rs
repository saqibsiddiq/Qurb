//! A device's permanent identity (decision 0079), over a real connection: an
//! identity made from a seed is the same every time, and two devices with
//! such identities connect, each pinning the other's.

use qurb_peer::{Identity, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use std::sync::{Arc, Mutex};

#[test]
fn the_same_seed_makes_the_same_identity() {
    let one = Identity::derived(&[7; 32]).unwrap();
    let again = Identity::derived(&[7; 32]).unwrap();
    let other = Identity::derived(&[8; 32]).unwrap();
    assert_eq!(one.fingerprint(), again.fingerprint(), "made again, it differs: a reinstall would be a new device");
    assert_ne!(one.fingerprint(), other.fingerprint());

    // Saved and loaded, it is still itself.
    let dir = tempfile::tempdir().unwrap();
    one.save(dir.path()).unwrap();
    assert_eq!(Identity::load(dir.path()).unwrap().fingerprint(), one.fingerprint());
    other.save(dir.path()).unwrap();
    assert_eq!(Identity::load(dir.path()).unwrap().fingerprint(), other.fingerprint(), "saving over did not replace");
}

#[tokio::test(flavor = "multi_thread")]
async fn devices_with_permanent_identities_connect() {
    let (phone, computer) = (Identity::derived(&[1; 32]).unwrap(), Identity::derived(&[2; 32]).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), ChunkKey::from_bytes([3; 32])).unwrap();
    let server = PeerServer::bind(
        "127.0.0.1:0".parse().unwrap(),
        &computer,
        &qurb_peer::tls::TrustList::new(vec![phone.fingerprint()]),
    )
    .unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });
    let client = PeerClient::connect(addr, &phone, computer.fingerprint()).await.unwrap();
    assert!(client.tree().await.unwrap().is_empty());
    client.close();
}

/// A computer serving `store`, trusting `allowed`.
async fn serve(computer: &Identity, store: Store, allowed: Vec<qurb_peer::Fingerprint>) -> std::net::SocketAddr {
    let server = PeerServer::bind("127.0.0.1:0".parse().unwrap(), computer, &qurb_peer::tls::TrustList::new(allowed)).unwrap();
    let addr = server.local_addr().unwrap();
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(store))).await });
    addr
}

/// A phone already installed takes its permanent identity: the computer
/// turns its fingerprint, keeping its device id, so everything kept for it
/// stays its; and then knows it by the permanent one.
#[tokio::test(flavor = "multi_thread")]
async fn an_installed_phone_becomes_its_permanent_identity() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), ChunkKey::from_bytes([3; 32])).unwrap();
    let computer = Identity::derived(&[2; 32]).unwrap();
    let (now, permanent) = (Identity::derived(&[10; 32]).unwrap(), Identity::derived(&[11; 32]).unwrap());
    let phone = qurb_sync::DeviceId::from_bytes([20; 32]);
    store.db().trust_peer(&phone, now.fingerprint().as_bytes(), "skib phone").unwrap();

    let reader = Store::open(dir.path(), ChunkKey::from_bytes([3; 32])).unwrap();
    let addr = serve(&computer, reader, vec![now.fingerprint(), permanent.fingerprint()]).await;
    let client = PeerClient::connect(addr, &now, computer.fingerprint()).await.unwrap();
    let became = client.become_permanent(permanent.cert_bytes()).await.unwrap();
    client.close();
    assert_eq!(became, Some(*phone.as_bytes()), "not told its own device id");
    let row = store.db().peer_by_fingerprint(permanent.fingerprint().as_bytes()).unwrap().expect("the permanent identity not known");
    assert_eq!(row.device_id, phone);
    assert!(store.db().peer_by_fingerprint(now.fingerprint().as_bytes()).unwrap().is_none(), "the old fingerprint still trusted");

    // And it is that device, connected as its permanent identity.
    let client = PeerClient::connect(addr, &permanent, computer.fingerprint()).await.unwrap();
    assert_eq!(client.become_permanent(permanent.cert_bytes()).await.unwrap(), Some(*phone.as_bytes()));
    client.close();
}

/// A phone set up again joins under a passing identity, says the permanent
/// one -- which the computer knows -- and is told the device it was; the
/// passing pairing then counts as that device, not a second one.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_set_up_again_is_told_the_device_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), ChunkKey::from_bytes([3; 32])).unwrap();
    let computer = Identity::derived(&[2; 32]).unwrap();
    let (permanent, passing) = (Identity::derived(&[11; 32]).unwrap(), Identity::derived(&[12; 32]).unwrap());
    let (was, joined) = (qurb_sync::DeviceId::from_bytes([20; 32]), qurb_sync::DeviceId::from_bytes([21; 32]));
    store.db().trust_peer(&was, permanent.fingerprint().as_bytes(), "skib phone").unwrap();
    store.db().trust_peer(&joined, passing.fingerprint().as_bytes(), "SM-S911B").unwrap();

    let reader = Store::open(dir.path(), ChunkKey::from_bytes([3; 32])).unwrap();
    let addr = serve(&computer, reader, vec![permanent.fingerprint(), passing.fingerprint()]).await;
    let client = PeerClient::connect(addr, &passing, computer.fingerprint()).await.unwrap();
    assert_eq!(client.become_permanent(permanent.cert_bytes()).await.unwrap(), Some(*was.as_bytes()));
    client.close();
    assert_eq!(store.db().became(&joined).unwrap(), Some(was), "the passing pairing is a second device");
}

/// The seed is the same for the same person's key and install, and differs
/// for another install -- another phone -- or another person.
#[test]
fn a_seed_is_one_install_of_one_person() {
    let key = [5; 32];
    let seed = Identity::permanent_seed(&key, "a1b2c3d4e5f60718");
    assert_eq!(seed, Identity::permanent_seed(&key, " a1b2c3d4e5f60718\n"));
    assert_ne!(seed, Identity::permanent_seed(&key, "ffffffffffffffff"), "two phones, one identity");
    assert_ne!(seed, Identity::permanent_seed(&[6; 32], "a1b2c3d4e5f60718"), "two people, one identity");
}
