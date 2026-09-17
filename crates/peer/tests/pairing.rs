//! Pairing: deciding which device to trust in the first place.
//!
//! Every other test in this crate starts from "both devices already know each
//! other's fingerprint". This is where that knowledge comes from, and it is the
//! step that makes pinned identity worth anything — a fingerprint only protects
//! you if you learned the right one.
//!
//! The security rests on the invite carrying the inviter's *full* fingerprint
//! across an out-of-band channel: a QR code on a screen, or a code read over
//! the phone. An attacker in the network path cannot change what is printed on
//! a screen. The one-time token is a narrower thing — it does not authenticate
//! anyone, it just stops a device that found the port from pairing with one
//! that never saw the code.

use qurb_peer::{accept, Identity, Invite, PairingHost};
use qurb_storage::{ChunkKey, Store};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_757_462_400;
const LOOPBACK: &str = "127.0.0.1:0";

struct Device {
    _dir: tempfile::TempDir,
    identity: Identity,
    store: Arc<Mutex<Store>>,
}

impl Device {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("store"), ChunkKey::from_bytes([42; 32])).unwrap();
        let identity = Identity::load_or_create(dir.path()).unwrap();
        Self { _dir: dir, identity, store: Arc::new(Mutex::new(store)) }
    }

    fn device_id(&self) -> qurb_sync::DeviceId {
        self.store.lock().unwrap().device_id().unwrap()
    }

    fn trusted(&self) -> Vec<qurb_storage::db::TrustedPeer> {
        self.store.lock().unwrap().db().trusted_peers().unwrap()
    }
}

// -- the invite format -------------------------------------------------------

#[test]
fn an_invite_round_trips() {
    let fingerprint = qurb_peer::Fingerprint::from_bytes([0xAB; 32]);
    for address in ["192.168.1.40:51820", "[2001:db8::1]:51820"] {
        let invite = Invite::new(fingerprint, address.parse().unwrap(), NOW);
        let parsed = Invite::parse(&invite.encode()).unwrap();
        assert_eq!(parsed, invite, "failed for {address}");
    }
}

#[test]
fn an_invite_survives_being_read_aloud() {
    // People retype these from a screen, in whatever case and with whatever
    // grouping they find readable.
    let invite = Invite::new(
        qurb_peer::Fingerprint::from_bytes([0x5A; 32]),
        "10.0.0.7:4000".parse().unwrap(),
        NOW,
    );

    for text in [
        invite.encode(),
        invite.for_humans(),
        invite.encode().to_lowercase(),
        format!("  {}  ", invite.for_humans()),
    ] {
        assert_eq!(Invite::parse(&text).unwrap(), invite, "failed on {text}");
    }
}

#[test]
fn a_damaged_invite_is_refused() {
    let invite = Invite::new(
        qurb_peer::Fingerprint::from_bytes([1; 32]),
        "10.0.0.1:1234".parse().unwrap(),
        NOW,
    );
    let encoded = invite.encode();

    for broken in [
        String::new(),
        "not a code".to_string(),
        encoded[..encoded.len() - 4].to_string(),
        encoded.replace("qurb1-", ""),
        format!("{encoded}EXTRA"),
    ] {
        match Invite::parse(&broken) {
            Err(_) => {}
            Ok(parsed) => assert_ne!(parsed, invite, "a damaged code decoded to the real invite"),
        }
    }
}

#[test]
fn every_invite_carries_a_different_token() {
    let fp = qurb_peer::Fingerprint::from_bytes([1; 32]);
    let addr: SocketAddr = "10.0.0.1:1234".parse().unwrap();
    let a = Invite::new(fp, addr, NOW);
    let b = Invite::new(fp, addr, NOW);
    assert_ne!(a.token, b.token, "a reused token would let one code pair twice");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invite_never_offers_an_address_nobody_can_dial() {
    // A socket bound to every interface reports `0.0.0.0`, which is true and
    // useless. An invite carrying it fails at the far end in a way that looks
    // like the joining device's fault.
    //
    // Every test above binds 127.0.0.1 explicitly and so never saw this; it took
    // running the real program, which binds the wildcard, to find it.
    let dir = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_create(dir.path()).unwrap();
    let host = PairingHost::open("0.0.0.0:0".parse().unwrap(), &identity, NOW).unwrap();

    let offered = host.invite().address;
    assert!(!offered.ip().is_unspecified(), "the invite offered {offered}");
    assert_ne!(offered.port(), 0, "the invite offered port 0");

    // And it survives being written down and read back.
    assert_eq!(Invite::parse(&host.invite().encode()).unwrap().address, offered);
}

// -- the exchange ------------------------------------------------------------

/// Run a full pairing and return what each side learned.
async fn pair(
    host: &Device,
    joiner: &Device,
    now: i64,
) -> (qurb_peer::Result<qurb_peer::Paired>, qurb_peer::Result<qurb_peer::Paired>) {
    let listener =
        PairingHost::open(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();

    let host_store = Arc::clone(&host.store);
    let waiting = async move { listener.wait(host_store, "Desktop", now).await };
    let joining = accept(&invite, &joiner.identity, Arc::clone(&joiner.store), "Laptop", now);

    tokio::join!(waiting, joining)
}

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_learn_each_other() {
    let host = Device::new();
    let joiner = Device::new();

    let (host_saw, joiner_saw) = pair(&host, &joiner, NOW).await;
    let host_saw = host_saw.expect("host side");
    let joiner_saw = joiner_saw.expect("joiner side");

    // Each learned the other's real identity, not what it claimed.
    assert_eq!(host_saw.device_id, joiner.device_id());
    assert_eq!(host_saw.fingerprint, joiner.identity.fingerprint());
    assert_eq!(host_saw.name, "Laptop");

    assert_eq!(joiner_saw.device_id, host.device_id());
    assert_eq!(joiner_saw.fingerprint, host.identity.fingerprint());
    assert_eq!(joiner_saw.name, "Desktop");

    // And both wrote it down.
    let trusted = host.trusted();
    assert_eq!(trusted.len(), 1);
    assert_eq!(trusted[0].device_id, joiner.device_id());
    assert_eq!(&trusted[0].fingerprint, joiner.identity.fingerprint().as_bytes());

    let trusted = joiner.trusted();
    assert_eq!(trusted.len(), 1);
    assert_eq!(trusted[0].device_id, host.device_id());
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_binds_the_device_id_to_the_fingerprint() {
    // The gap this closes. Version vectors count against a device id and
    // connections authenticate a fingerprint; until pairing, nothing tied the
    // two together, so an authenticated device could claim any history it liked.
    let host = Device::new();
    let joiner = Device::new();
    pair(&host, &joiner, NOW).await.0.expect("pairing");

    let store = host.store.lock().unwrap();
    let looked_up = store
        .db()
        .peer_by_fingerprint(joiner.identity.fingerprint().as_bytes())
        .unwrap()
        .expect("the fingerprint should resolve to a device");

    assert_eq!(looked_up.device_id, joiner.device_id());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_never_saw_the_code_cannot_pair() {
    // Someone who found the port open, rather than the invite.
    let host = Device::new();
    let stranger = Device::new();

    let listener = PairingHost::open(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let mut forged = listener.invite().clone();
    forged.token = [0xFF; 16];

    let attempt = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        accept(&forged, &stranger.identity, Arc::clone(&stranger.store), "Attacker", NOW),
    )
    .await;

    // Refused, one way or another -- never accepted.
    if let Ok(Ok(_)) = attempt {
        panic!("a device without the token was paired");
    }
    assert!(host.trusted().is_empty(), "the host trusted a device that had no token");
    listener.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_expired_invite_is_refused() {
    let host = Device::new();
    let joiner = Device::new();

    let listener = PairingHost::open(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();
    let long_after = invite.expires_at + 1;

    let result =
        accept(&invite, &joiner.identity, Arc::clone(&joiner.store), "Laptop", long_after).await;
    assert!(result.is_err(), "an expired code was accepted");
    assert!(joiner.trusted().is_empty());
    listener.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invite_cannot_be_used_twice() {
    // The host stops listening once someone pairs, so a code read off a screen
    // by a second person is worthless.
    let host = Device::new();
    let first = Device::new();
    let second = Device::new();

    let listener = PairingHost::open(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();

    let host_store = Arc::clone(&host.store);
    let waiting = async move { listener.wait(host_store, "Desktop", NOW).await };
    let joining = accept(&invite, &first.identity, Arc::clone(&first.store), "First", NOW);
    let (host_saw, first_saw) = tokio::join!(waiting, joining);
    host_saw.expect("first pairing");
    first_saw.expect("first pairing");

    let replay = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        accept(&invite, &second.identity, Arc::clone(&second.store), "Second", NOW),
    )
    .await;
    assert!(
        matches!(replay, Err(_) | Ok(Err(_))),
        "the same code paired a second device"
    );
    assert_eq!(host.trusted().len(), 1, "a replayed code added a second trusted device");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_joiner_pinned_to_the_wrong_fingerprint_refuses_to_connect() {
    // The out-of-band channel is what authenticates the host. A code carrying
    // somebody else's fingerprint must fail rather than pair with whoever is at
    // that address.
    let host = Device::new();
    let joiner = Device::new();
    let impostor = Device::new();

    let listener = PairingHost::open(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let mut tampered = listener.invite().clone();
    tampered.fingerprint = impostor.identity.fingerprint();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        accept(&tampered, &joiner.identity, Arc::clone(&joiner.store), "Laptop", NOW),
    )
    .await;

    assert!(matches!(result, Err(_) | Ok(Err(_))), "connected to the wrong device");
    assert!(joiner.trusted().is_empty());
    listener.close();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hostile_name_is_not_taken_literally() {
    // The name is chosen by the peer, so it is untrusted text that ends up in
    // logs and interfaces.
    let host = Device::new();
    let joiner = Device::new();

    let listener = PairingHost::open(LOOPBACK.parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();

    let host_store = Arc::clone(&host.store);
    let waiting = async move { listener.wait(host_store, "Desktop", NOW).await };
    let nasty = format!("evil\n\r\u{0}{}", "A".repeat(500));
    let joining = accept(&invite, &joiner.identity, Arc::clone(&joiner.store), &nasty, NOW);
    let (host_saw, _) = tokio::join!(waiting, joining);

    let name = host_saw.expect("pairing").name;
    assert!(!name.contains('\n') && !name.contains('\0'), "control characters survived: {name:?}");
    assert!(name.chars().count() <= 64, "an overlong name was stored: {} chars", name.chars().count());
}

#[tokio::test(flavor = "multi_thread")]
async fn re_pairing_a_known_device_updates_it_rather_than_duplicating() {
    // Replacing a device's certificate must not leave the old identity trusted
    // for ever.
    let host = Device::new();
    let joiner = Device::new();
    pair(&host, &joiner, NOW).await.0.expect("first pairing");

    {
        let store = host.store.lock().unwrap();
        store
            .db()
            .trust_peer(&joiner.device_id(), &[0xEE; 32], "Laptop (new certificate)")
            .unwrap();
    }

    let trusted = host.trusted();
    assert_eq!(trusted.len(), 1, "re-pairing created a second row");
    assert_eq!(trusted[0].fingerprint, [0xEE; 32], "the old fingerprint is still trusted");
    assert!(
        host.store
            .lock()
            .unwrap()
            .db()
            .peer_by_fingerprint(joiner.identity.fingerprint().as_bytes())
            .unwrap()
            .is_none(),
        "the superseded fingerprint still resolves to a device"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forgotten_device_is_no_longer_trusted() {
    let host = Device::new();
    let joiner = Device::new();
    pair(&host, &joiner, NOW).await.0.expect("pairing");

    let store = host.store.lock().unwrap();
    assert!(store.db().forget_peer(&joiner.device_id()).unwrap());
    assert!(store.db().trusted_peers().unwrap().is_empty());
    assert!(!store.db().forget_peer(&joiner.device_id()).unwrap(), "forgetting twice reported work");
}

// -- what pairing is for -----------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_paired_device_can_sync_and_an_unpaired_one_cannot() {
    // The whole point, end to end. Before this phase a caller had to supply
    // fingerprints by hand and the system had no notion of "my devices". Now the
    // trust store answers it, and the answer is enforced.
    use qurb_peer::{PeerClient, PeerServer};

    let host = Device::new();
    let joiner = Device::new();
    let stranger = Device::new();

    // Something worth serving.
    {
        let mut store = host.store.lock().unwrap();
        store.put_bytes("secret.txt", b"only for my own devices", 0).unwrap();
    }

    pair(&host, &joiner, NOW).await.0.expect("pairing");

    // The listener takes its guest list from the trust store, not from a caller.
    let server = {
        let store = host.store.lock().unwrap();
        PeerServer::bind_trusting(LOOPBACK.parse().unwrap(), &host.identity, &store).unwrap()
    };
    let addr = server.local_addr().unwrap();
    let serving = Arc::clone(&host.store);
    tokio::spawn(async move { server.serve(serving).await });

    // The paired device is let in.
    let client = PeerClient::connect(addr, &joiner.identity, host.identity.fingerprint())
        .await
        .expect("a paired device should connect");
    let tree = client.tree().await.expect("a paired device should be served");
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].path, "secret.txt");
    client.close();

    // The stranger is not, even though its certificate is perfectly valid and
    // it knows the host's fingerprint.
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        PeerClient::connect(addr, &stranger.identity, host.identity.fingerprint()),
    )
    .await;
    if let Ok(Ok(client)) = outcome {
        assert!(client.tree().await.is_err(), "an unpaired device was served files");
    }
}
