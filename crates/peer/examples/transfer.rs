//! Measure what a sync actually costs on the wire.
//!
//! ```bash
//! cargo run --release --example transfer -- <dir-a> <dir-b>
//! ```
//!
//! Serves A, pulls everything into B, and reports how many chunks and bytes
//! crossed. Run it twice with a small edit in between: the second run is where
//! content-defined chunking either earns its complexity or does not.

use qurb_engine::Engine;
use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_peer::{Identity, NetworkSource, PeerClient, PeerServer};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (a_root, b_root) = match (args.get(1), args.get(2)) {
        (Some(a), Some(b)) => (PathBuf::from(a), PathBuf::from(b)),
        _ => {
            eprintln!("usage: transfer <dir-a> <dir-b>");
            std::process::exit(2);
        }
    };

    // A is the first device: it creates the key. B is a second device the user
    // has enrolled with A's recovery phrase, which is what makes them a set
    // rather than two strangers -- they derive the same chunk key and can
    // therefore read each other's data.
    let (a_key, phrase) = match Vault::at(&a_root.join(".qurb")).open_or_create()? {
        Opened::Created { key, phrase } => {
            println!("A created a key. Its recovery phrase begins: {} ...",
                phrase.words()[..3].join(" "));
            (key, Some(phrase.to_string()))
        }
        Opened::Existing(key) => (key, None),
    };

    let b_vault = Vault::at(&b_root.join(".qurb"));
    let b_key = if b_vault.exists() {
        match b_vault.open_or_create()? {
            Opened::Existing(key) => key,
            Opened::Created { key, .. } => key,
        }
    } else {
        match &phrase {
            Some(words) => {
                println!("B enrolled with A's recovery phrase.");
                b_vault.restore(&RecoveryPhrase::parse(words)?)?
            }
            None => {
                eprintln!("A already has a key and B does not. Enrol B with A's phrase first:");
                eprintln!("  cargo run -p qurb-keys --example enrol -- {} \"<phrase>\"",
                    b_root.join(".qurb").display());
                std::process::exit(1);
            }
        }
    };

    let (mut a_engine, a_identity) = open(&a_root, &a_key)?;
    let (mut b_engine, b_identity) = open(&b_root, &b_key)?;

    let a_stats = a_engine.reconcile()?;
    let b_stats = b_engine.reconcile()?;
    println!(
        "A: {} stored, {} unchanged | B: {} stored, {} unchanged",
        a_stats.stored, a_stats.unchanged, b_stats.stored, b_stats.unchanged
    );

    // Serve A from its own connection, so its engine stays usable.
    let served_store = Store::open(&a_root.join(".qurb"), chunk_key(&a_key))?;
    let server = PeerServer::bind(
        "127.0.0.1:0".parse()?,
        &a_identity,
        &[b_identity.fingerprint()],
    )?;
    let addr = server.local_addr()?;
    let wire = server.stats();
    tokio::spawn(async move { server.serve(Arc::new(Mutex::new(served_store))).await });

    println!("\nA is serving on {addr}");
    println!("  A fingerprint {}", a_identity.fingerprint().short());
    println!("  B fingerprint {}", b_identity.fingerprint().short());

    let started = Instant::now();
    let client = PeerClient::connect(addr, &b_identity, a_identity.fingerprint()).await?;
    println!("  connected in {:?}, rtt {:?}", started.elapsed(), client.rtt());

    let tree = client.tree().await?;
    let plan = b_engine.plan_against(&tree)?;
    println!("\nB has {} action(s) against A's {} path(s)", plan.len(), tree.len());

    let reader = Store::open(&b_root.join(".qurb"), chunk_key(&b_key))?;
    let transfer = Instant::now();
    let stats = {
        let mut source = NetworkSource::new(&client, &reader);
        b_engine.apply_plan(&plan, &mut source)?
    };
    let elapsed = transfer.elapsed();

    println!(
        "\nadopted {} merged {} conflicts {} resurrected {} failed {}",
        stats.adopted, stats.merged, stats.conflicts, stats.resurrected, stats.failures.len()
    );
    for f in &stats.failures {
        println!("  failed: {} -- {}", f.path.display(), f.error);
    }

    let bytes = wire.bytes();
    println!("\non the wire");
    println!("  chunks requested  {}", wire.chunks());
    println!("  bytes transferred {}", human(bytes));
    println!("  elapsed           {elapsed:.2?}");

    let on_disk: u64 = b_engine.store().db().size_totals()?.0;
    if on_disk > 0 {
        println!(
            "  transferred {:.1}% of the {} B now holds",
            bytes as f64 / on_disk as f64 * 100.0,
            human(on_disk)
        );
    }

    client.close();
    Ok(())
}

/// The chunk key is derived, never stored. Only the master key is on disk, and
/// only in one place.
fn chunk_key(master: &MasterKey) -> ChunkKey {
    ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes())
}

fn open(root: &Path, master: &MasterKey) -> Result<(Engine, Identity), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root)?;
    let store_dir = root.join(".qurb");
    let store = Store::open(&store_dir, chunk_key(master))?;
    let identity = Identity::load_or_create(&store_dir)?;
    let ignore = IgnoreRules::new().with_store_dir(&store_dir);
    Ok((Engine::new(root, store, ignore), identity))
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", UNITS[i])
}
