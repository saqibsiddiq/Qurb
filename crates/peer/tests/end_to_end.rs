//! Everything, joined up.
//!
//! Two devices that have never spoken: they pair out of band, announce
//! themselves to a rendezvous service, are introduced, connect, and sync a file.
//! Every layer the project has built is involved, and this is the first test in
//! which they all run together.
//!
//! What it does *not* prove is traversal — there is no NAT on loopback. What it
//! proves is that the sequence is right: discovery, rendezvous, simultaneous
//! dialling, candidate racing, pinned identity, and then the data plane.

use qurb_engine::Engine;
use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_peer::{Connector, Identity, PairingHost};
use qurb_signal::SignalServer;
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const NOW: i64 = 1_757_462_400;

/// One device: a directory, a key, a store, an identity.
struct Device {
    _dir: tempfile::TempDir,
    root: PathBuf,
    master: MasterKey,
    identity: Identity,
    store: Arc<Mutex<Store>>,
}

impl Device {
    /// The first device: creates the key.
    fn first() -> (Self, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");

        let Opened::Created { key, phrase } = Vault::at(&store_dir).open_or_create().unwrap()
        else {
            panic!("expected a new key");
        };
        let phrase = phrase.to_string();
        let device = Self::open(dir, root, store_dir, key);
        (device, phrase)
    }

    /// A second device, enrolled with the first's recovery phrase. That shared
    /// key is what makes them one person's devices rather than strangers.
    fn enrolled(phrase: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sync");
        fs::create_dir_all(&root).unwrap();
        let store_dir = root.join(".qurb");
        let key = Vault::at(&store_dir).restore(&RecoveryPhrase::parse(phrase).unwrap()).unwrap();
        Self::open(dir, root, store_dir, key)
    }

    fn open(dir: tempfile::TempDir, root: PathBuf, store_dir: PathBuf, master: MasterKey) -> Self {
        let chunk_key =
            ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
        let store = Store::open(&store_dir, chunk_key).unwrap();
        let identity = Identity::load_or_create(&store_dir).unwrap();
        Self { _dir: dir, root, master, identity, store: Arc::new(Mutex::new(store)) }
    }

    fn engine(&self) -> Engine {
        let store_dir = self.root.join(".qurb");
        let chunk_key =
            ChunkKey::from_bytes(self.master.derive(Purpose::ChunkEncryption).to_bytes());
        let store = Store::open(&store_dir, chunk_key).unwrap();
        Engine::new(
            self.root.clone(),
            store,
            IgnoreRules::new().with_store_dir(&store_dir),
        )
    }

    fn trusted(&self) -> Vec<qurb_peer::Fingerprint> {
        qurb_peer::trusted_fingerprints(&self.store.lock().unwrap()).unwrap()
    }
}

/// Pair two devices out of band, as scanning a QR code would.
async fn pair(host: &Device, joiner: &Device) {
    let listener =
        PairingHost::open("127.0.0.1:0".parse().unwrap(), &host.identity, NOW).unwrap();
    let invite = listener.invite().clone();

    let host_store = Arc::clone(&host.store);
    let waiting = async move { listener.wait(host_store, "Desktop", NOW).await };
    let joining =
        qurb_peer::accept(&invite, &joiner.identity, Arc::clone(&joiner.store), "Laptop", NOW);

    let (a, b) = tokio::join!(waiting, joining);
    a.expect("host paired");
    b.expect("joiner paired");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_strangers_pair_find_each_other_and_sync() {
    // The rendezvous service.
    let signal = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let url = format!("ws://{}", signal.local_addr().unwrap());
    let running = Arc::clone(&signal);
    tokio::spawn(async move { running.serve().await });

    // Two devices belonging to one person, sharing a master key.
    let (desktop, phrase) = Device::first();
    let laptop = Device::enrolled(&phrase);

    // Something worth syncing.
    fs::write(desktop.root.join("notes.txt"), b"written on the desktop").unwrap();
    let mut desktop_engine = desktop.engine();
    desktop_engine.reconcile().unwrap();

    // They have never spoken. Pairing is the out-of-band step.
    assert!(desktop.trusted().is_empty());
    pair(&desktop, &laptop).await;
    assert_eq!(desktop.trusted().len(), 1, "pairing did not take");

    // Both bring up their connection machinery and announce.
    let desktop_conn = Connector::start(
        "127.0.0.1:0".parse().unwrap(),
        desktop.identity.clone(),
        desktop.master.clone(),
        &desktop.trusted(),
        &url,
        false, // no STUN: there is nothing to discover on one machine
        None,
    )
    .await
    .unwrap();

    let laptop_conn = Connector::start(
        "127.0.0.1:0".parse().unwrap(),
        laptop.identity.clone(),
        laptop.master.clone(),
        &laptop.trusted(),
        &url,
        false,
        None,
    )
    .await
    .unwrap();

    // The desktop sits waiting, serving both the rendezvous channel and QUIC.
    let serving_store = Arc::clone(&desktop.store);
    let desktop_endpoint = desktop_conn.endpoint().clone();
    tokio::spawn(async move {
        while let Some(incoming) = desktop_endpoint.accept().await {
            let store = Arc::clone(&serving_store);
            tokio::spawn(async move {
                if let Ok(connection) = incoming.await {
                    qurb_peer::server::serve_connection_for_test(connection, store).await;
                }
            });
        }
    });

    // Give the desktop a moment to appear in the directory.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The laptop asks to be introduced, and is.
    let client = tokio::time::timeout(
        Duration::from_secs(20),
        laptop_conn.reach(desktop.identity.fingerprint()),
    )
    .await
    .expect("reaching the desktop timed out")
    .expect("could not reach the desktop");

    // And the data plane works over the connection that came out of it.
    let tree = client.tree().await.expect("tree");
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].path, "notes.txt");

    let mut laptop_engine = laptop.engine();
    let plan = laptop_engine.plan_against(&tree).unwrap();
    let reader = Store::open(
        &laptop.root.join(".qurb"),
        ChunkKey::from_bytes(laptop.master.derive(Purpose::ChunkEncryption).to_bytes()),
    )
    .unwrap();
    let mut source = qurb_peer::NetworkSource::new(&client, &reader);
    let stats = laptop_engine.apply_plan(&plan, &mut source).unwrap();
    assert!(stats.is_clean(), "{:?}", stats.failures);

    assert_eq!(
        fs::read(laptop.root.join("notes.txt")).unwrap(),
        b"written on the desktop",
        "the file did not arrive"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_is_not_announced_is_reported_rather_than_hung_on() {
    // A device that is switched off. The caller must find out, not wait for
    // ever -- this is the case a relay and an always-on replica exist for.
    let signal = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let url = format!("ws://{}", signal.local_addr().unwrap());
    let running = Arc::clone(&signal);
    tokio::spawn(async move { running.serve().await });

    let (desktop, phrase) = Device::first();
    let laptop = Device::enrolled(&phrase);
    pair(&desktop, &laptop).await;

    let laptop_conn = Connector::start(
        "127.0.0.1:0".parse().unwrap(),
        laptop.identity.clone(),
        laptop.master.clone(),
        &laptop.trusted(),
        &url,
        false,
        None,
    )
    .await
    .unwrap();

    let outcome = tokio::time::timeout(
        Duration::from_secs(25),
        laptop_conn.reach(desktop.identity.fingerprint()),
    )
    .await;

    match outcome {
        Ok(Err(_)) => {}
        Ok(Ok(_)) => panic!("connected to a device that was not there"),
        Err(_) => panic!("waiting for an absent peer never returned"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_local_address_is_preferred_over_a_public_one() {
    // Two devices on one network should not route through the internet to reach
    // each other. Candidates are raced in parallel, so this is about ordering
    // rather than about which one is tried first in time.
    let endpoints = qurb_signal::Endpoints {
        public: Some("203.0.113.5:4500".parse().unwrap()),
        local: vec!["192.168.1.40:4500".parse().unwrap()],
    };
    let candidates = endpoints.candidates();
    assert_eq!(candidates[0], "192.168.1.40:4500".parse().unwrap());
}

// -- the long way round ------------------------------------------------------

/// A relay, running in the background.
async fn relay() -> std::net::SocketAddr {
    let server =
        Arc::new(qurb_relay::RelayServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let addr = server.local_addr().unwrap();
    tokio::spawn(async move { server.serve().await });
    addr
}

/// Bring a device up with both paths, and serve on both.
async fn connected(
    device: &Device,
    signal_url: &str,
    relay_addr: std::net::SocketAddr,
) -> Arc<Connector> {
    let connector = Connector::start(
        "127.0.0.1:0".parse().unwrap(),
        device.identity.clone(),
        device.master.clone(),
        &device.trusted(),
        signal_url,
        false,
        Some(relay_addr),
    )
    .await
    .unwrap();

    // A device has two ways in and must listen on both. Serving only the direct
    // endpoint would make the relay useless in exactly the case it exists for.
    for endpoint in [Some(connector.endpoint().clone()), connector.relay_endpoint().cloned()]
        .into_iter()
        .flatten()
    {
        let store = Arc::clone(&device.store);
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

    Arc::new(connector)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_syncs_through_the_relay_when_there_is_no_direct_path() {
    // The fallback, end to end. Everything the direct path does — pinned
    // identity, the wire protocol, chunk transfer — has to work unchanged when
    // the bytes take the long way round, because the relay carries the QUIC
    // session rather than the messages inside it.
    let signal = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let signal_url = format!("ws://{}", signal.local_addr().unwrap());
    let running = Arc::clone(&signal);
    tokio::spawn(async move { running.serve().await });
    let relay_addr = relay().await;

    let (desktop, phrase) = Device::first();
    let laptop = Device::enrolled(&phrase);
    fs::write(desktop.root.join("notes.txt"), b"carried the long way round").unwrap();
    desktop.engine().reconcile().unwrap();
    pair(&desktop, &laptop).await;

    let _desktop_conn = connected(&desktop, &signal_url, relay_addr).await;
    let laptop_conn = connected(&laptop, &signal_url, relay_addr).await;

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Force the relay rather than arranging a real failure: on one machine every
    // direct attempt succeeds, so the fallback would never be exercised.
    let client = tokio::time::timeout(
        Duration::from_secs(20),
        laptop_conn.reach_via_relay(desktop.identity.fingerprint()),
    )
    .await
    .expect("the relayed connection timed out")
    .expect("could not reach the desktop through the relay");

    let tree = client.tree().await.expect("tree over the relay");
    assert_eq!(tree.len(), 1);

    let mut laptop_engine = laptop.engine();
    let plan = laptop_engine.plan_against(&tree).unwrap();
    let reader = Store::open(
        &laptop.root.join(".qurb"),
        ChunkKey::from_bytes(laptop.master.derive(Purpose::ChunkEncryption).to_bytes()),
    )
    .unwrap();
    let mut source = qurb_peer::NetworkSource::new(&client, &reader);
    let stats = laptop_engine.apply_plan(&plan, &mut source).unwrap();
    assert!(stats.is_clean(), "{:?}", stats.failures);

    assert_eq!(
        fs::read(laptop.root.join("notes.txt")).unwrap(),
        b"carried the long way round",
        "the file did not arrive over the relay"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn identity_is_pinned_just_as_hard_over_the_relay() {
    // Nothing about trust may loosen because the path got longer. The relay
    // carries the handshake without being party to it.
    let signal = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let signal_url = format!("ws://{}", signal.local_addr().unwrap());
    let running = Arc::clone(&signal);
    tokio::spawn(async move { running.serve().await });
    let relay_addr = relay().await;

    let (desktop, phrase) = Device::first();
    let laptop = Device::enrolled(&phrase);
    pair(&desktop, &laptop).await;

    let _desktop_conn = connected(&desktop, &signal_url, relay_addr).await;
    let laptop_conn = connected(&laptop, &signal_url, relay_addr).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // A stranger's fingerprint, over a relay that will happily forward to it.
    let stranger = tempfile::tempdir().unwrap();
    let wrong = Identity::load_or_create(stranger.path()).unwrap().fingerprint();

    let outcome = tokio::time::timeout(
        Duration::from_secs(15),
        laptop_conn.reach_via_relay(wrong),
    )
    .await;

    match outcome {
        Ok(Err(_)) => {}
        Ok(Ok(_)) => panic!("the relay path accepted an unexpected identity"),
        Err(_) => panic!("the attempt never returned"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn falling_back_needs_a_relay_to_fall_back_to() {
    // A device with no relay configured must say so rather than appear to work.
    let signal = Arc::new(SignalServer::bind("127.0.0.1:0".parse().unwrap()).await.unwrap());
    let signal_url = format!("ws://{}", signal.local_addr().unwrap());
    let running = Arc::clone(&signal);
    tokio::spawn(async move { running.serve().await });

    let (desktop, phrase) = Device::first();
    let laptop = Device::enrolled(&phrase);
    pair(&desktop, &laptop).await;

    let laptop_conn = Connector::start(
        "127.0.0.1:0".parse().unwrap(),
        laptop.identity.clone(),
        laptop.master.clone(),
        &laptop.trusted(),
        &signal_url,
        false,
        None,
    )
    .await
    .unwrap();

    assert!(laptop_conn.relay_endpoint().is_none());
    let outcome = laptop_conn.reach_via_relay(desktop.identity.fingerprint()).await;
    assert!(matches!(outcome, Err(qurb_peer::Error::NoRelay)), "expected NoRelay");
}
