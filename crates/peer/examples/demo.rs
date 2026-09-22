//! The whole system, running.
//!
//! ```bash
//! cargo run --release -p qurb-peer --example demo -- /tmp/device-a /tmp/device-b
//! ```
//!
//! Starts a signalling service and a relay, brings up two devices in the two
//! directories given, pairs them, introduces them, connects, and syncs. Then it
//! keeps running: drop a file into either directory and watch it appear in the
//! other.
//!
//! Everything is in one process, which is the one thing that is not realistic.
//! The sockets, the handshakes, the encryption, the chunking and the conflict
//! resolution are all the real implementations.

use qurb_engine::Engine;
use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_peer::{Connector, Identity, PairingHost, PeerClient};
use qurb_relay::RelayServer;
use qurb_signal::SignalServer;
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const NOW: i64 = 1_757_462_400;

struct Device {
    name: &'static str,
    root: PathBuf,
    master: MasterKey,
    identity: Identity,
    store: Arc<Mutex<Store>>,
}

impl Device {
    fn open(name: &'static str, root: &Path, phrase: Option<&str>) -> (Self, Option<String>) {
        std::fs::create_dir_all(root).expect("create the directory");
        let store_dir = root.join(".qurb");

        let (master, shown) = match phrase {
            Some(words) => {
                let vault = Vault::at(&store_dir);
                if vault.exists() {
                    (vault.open_or_create().unwrap().key().clone(), None)
                } else {
                    let phrase = RecoveryPhrase::parse(words).expect("a valid recovery phrase");
                    (vault.restore(&phrase).unwrap(), None)
                }
            }
            None => match Vault::at(&store_dir).open_or_create().unwrap() {
                Opened::Created { key, phrase } => (key, Some(phrase.to_string())),
                Opened::Existing(key) => (key, None),
            },
        };

        let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
        let store = Store::open(&store_dir, chunk_key).expect("open the store");
        let identity = Identity::load_or_create(&store_dir).expect("device identity");

        (
            Self { name, root: root.to_path_buf(), master, identity, store: Arc::new(Mutex::new(store)) },
            shown,
        )
    }

    fn engine(&self) -> Engine {
        let store_dir = self.root.join(".qurb");
        let chunk_key =
            ChunkKey::from_bytes(self.master.derive(Purpose::ChunkEncryption).to_bytes());
        let store = Store::open(&store_dir, chunk_key).unwrap();
        Engine::new(self.root.clone(), store, IgnoreRules::new().with_store_dir(&store_dir))
    }

    fn reader(&self) -> Store {
        let chunk_key =
            ChunkKey::from_bytes(self.master.derive(Purpose::ChunkEncryption).to_bytes());
        Store::open(&self.root.join(".qurb"), chunk_key).unwrap()
    }

    fn trusted(&self) -> Vec<qurb_peer::Fingerprint> {
        qurb_peer::trusted_fingerprints(&self.store.lock().unwrap()).unwrap()
    }
}

fn step(n: u8, what: &str) {
    println!("\n\x1b[1m[{n}] {what}\x1b[0m");
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (a_root, b_root) = match (args.get(1), args.get(2)) {
        (Some(a), Some(b)) => (PathBuf::from(a), PathBuf::from(b)),
        _ => {
            eprintln!("usage: demo <dir-a> <dir-b>");
            eprintln!();
            eprintln!("Two directories to treat as two devices. They will be created.");
            std::process::exit(2);
        }
    };

    // -- infrastructure ------------------------------------------------------
    step(1, "Starting the servers");

    let signal = Arc::new(SignalServer::bind("127.0.0.1:0".parse()?).await?);
    let signal_url = format!("ws://{}", signal.local_addr()?);
    let running = Arc::clone(&signal);
    tokio::spawn(async move { running.serve().await });
    println!("    rendezvous  {signal_url}");
    println!("                knows no filenames, and cannot tell whose devices these are");

    let relay = Arc::new(RelayServer::bind("127.0.0.1:0".parse()?).await?);
    let relay_addr = relay.local_addr()?;
    let relay_stats = relay.stats();
    let running = Arc::clone(&relay);
    tokio::spawn(async move { running.serve().await });
    println!("    relay       {relay_addr}");
    println!("                forwards ciphertext it has no key for");

    // -- devices -------------------------------------------------------------
    step(2, "Opening the two devices");

    let (desktop, phrase) = Device::open("A", &a_root, None);
    println!("    A  {}", a_root.display());
    if let Some(phrase) = &phrase {
        let words: Vec<&str> = phrase.split(' ').collect();
        println!("       new key created. Recovery phrase begins:");
        println!("         {} ... {}", words[..4].join(" "), words[23]);
    } else {
        println!("       existing key");
    }

    let (laptop, _) = Device::open("B", &b_root, phrase.as_deref());
    println!("    B  {}", b_root.display());
    println!("       {}", if phrase.is_some() { "enrolled with A's recovery phrase" } else { "existing key" });
    println!("\n    They share a master key, which is what makes them one person's");
    println!("    devices. Neither has the other's network identity yet.");

    // -- pairing -------------------------------------------------------------
    step(3, "Pairing, out of band");

    if desktop.trusted().is_empty() {
        let host = PairingHost::open("127.0.0.1:0".parse()?, &desktop.identity, NOW)?;
        let invite = host.invite().clone();
        println!("    A shows this, as a QR code or read aloud:");
        println!("      {}", invite.for_humans());
        println!("\n    It carries A's full fingerprint. An attacker on the network");
        println!("    cannot change what is printed on a screen, which is the whole");
        println!("    reason this step happens outside the network.");

        let store = Arc::clone(&desktop.store);
        let waiting = async move { host.wait(store, "Device A", NOW).await };
        let joining =
            qurb_peer::accept(&invite, &laptop.identity, Arc::clone(&laptop.store), "Device B", NOW);
        let (a, b) = tokio::join!(waiting, joining);
        let a = a?;
        let b = b?;
        println!("\n    A now trusts {} ({})", a.name, a.fingerprint.short());
        println!("    B now trusts {} ({})", b.name, b.fingerprint.short());
    } else {
        println!("    already paired ({} trusted device(s))", desktop.trusted().len());
    }

    // -- index what is there -------------------------------------------------
    step(4, "Indexing what is already in the directories");

    for device in [&desktop, &laptop] {
        let stats = device.engine().reconcile()?;
        println!(
            "    {}  stored {} unchanged {} deleted {}",
            device.name, stats.stored, stats.unchanged, stats.deleted
        );
    }

    // -- connect -------------------------------------------------------------
    step(5, "Finding each other and connecting");

    let a_conn = Arc::new(
        Connector::start(
            "127.0.0.1:0".parse()?,
            desktop.identity.clone(),
            desktop.master.clone(),
            &qurb_peer::tls::TrustList::new(desktop.trusted()),
            &signal_url,
            false,
            Some(relay_addr),
        )
        .await?,
    );
    let b_conn = Arc::new(
        Connector::start(
            "127.0.0.1:0".parse()?,
            laptop.identity.clone(),
            laptop.master.clone(),
            &qurb_peer::tls::TrustList::new(laptop.trusted()),
            &signal_url,
            false,
            Some(relay_addr),
        )
        .await?,
    );

    // Both devices listen on both paths.
    for (device, connector) in [(&desktop, &a_conn), (&laptop, &b_conn)] {
        for endpoint in
            [Some(connector.endpoint().clone()), connector.relay_endpoint().cloned()]
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
    }

    // Each device announced and began answering requests when it started; the
    // signalling connection is held for its lifetime.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let started = Instant::now();
    let b_to_a: PeerClient = b_conn.reach(desktop.identity.fingerprint()).await?;
    println!("    B reached A at {} in {:?}", b_to_a.remote_address(), started.elapsed());

    let a_to_b: PeerClient = a_conn.reach(laptop.identity.fingerprint()).await?;
    println!("    A reached B at {} in {:?}", a_to_b.remote_address(), started.elapsed());
    println!("\n    Identity is pinned at both ends: each proved it holds the key");
    println!("    behind the fingerprint the other recorded when pairing.");

    // -- sync ----------------------------------------------------------------
    step(6, "Syncing, then watching for changes");
    println!("    Drop files into either directory. Ctrl-C to stop.\n");

    let mut round = 0u32;
    loop {
        round += 1;
        let mut moved = false;

        for (from, to, client) in
            [(&desktop, &laptop, &b_to_a), (&laptop, &desktop, &a_to_b)]
        {
            from.engine().reconcile()?;

            let tree = match client.tree().await {
                Ok(tree) => tree,
                Err(e) => {
                    println!("    {} -> {}: connection lost ({e})", from.name, to.name);
                    return Ok(());
                }
            };

            let mut engine = to.engine();
            let plan = engine.plan_against(&tree)?;
            if plan.is_empty() {
                continue;
            }

            let reader = to.reader();
            let mut source = qurb_peer::NetworkSource::new(client, &reader);
            let stats = engine.apply_plan(&plan, &mut source)?;
            moved = true;

            for action in &plan {
                let kind = match action {
                    qurb_sync::Action::Adopt { remote } if remote.is_deleted() => "deleted",
                    qurb_sync::Action::Adopt { .. } => "received",
                    qurb_sync::Action::Conflict { .. } => "conflict",
                    qurb_sync::Action::Resurrect { .. } => "restored",
                    qurb_sync::Action::Merge { .. } => "merged",
                    qurb_sync::Action::Offer { .. } => continue,
                };
                println!("    {} -> {}  {kind:<9} {}", from.name, to.name, action.path());
            }
            if !stats.failures.is_empty() {
                for failure in &stats.failures {
                    println!("      failed: {} -- {}", failure.path.display(), failure.error);
                }
            }
        }

        if moved {
            println!(
                "      (relay has carried {} bytes in total; direct transfers do not touch it)",
                relay_stats.bytes()
            );
        }
        if round == 1 && !moved {
            println!("    nothing to do -- both directories already agree");
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
