//! qurb — private cloud storage.
//!
//! The program a person runs. Everything else in this repository is a library;
//! this is the daemon and the handful of commands around it.

mod config;
mod daemon;

use anyhow::{bail, Context, Result};
use config::Config;
use daemon::Daemon;
use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_peer::{Identity, PairingHost};
use qurb_storage::{ChunkKey, Store};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const USAGE: &str = "\
qurb — private cloud storage

  qurb init <dir>                     set up a device and create a key
  qurb enrol <dir> \"<24 words>\"       set up a device with an existing key
  qurb pair <dir>                     show a code and wait for a device to join
  qurb join <dir> <code>              join a device that is showing a code
  qurb run <dir>                      watch, sync, and keep running
  qurb status <dir>                   what this device holds and trusts
  qurb verify <dir> [--deep]          check the store against itself
  qurb config <dir> [key=value ...]   show or change settings

Running the services yourself:

  qurb signal [addr]                  the rendezvous service (default :9000)
  qurb relay [addr]                   the relay (default :9001)

Settings live in <dir>/.qurb/config and can be edited by hand.
";

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "qurb=info,warn".into()),
        )
        .with_target(false)
        .init();

    if let Err(error) = run() {
        // The chain, because the useful part is usually not the outermost
        // message: "opening the store" matters less than "permission denied".
        eprintln!("error: {error}");
        for cause in error.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        std::process::exit(2);
    };

    match command {
        "init" => init(&directory(&args)?),
        "enrol" | "enroll" => {
            let phrase = args.get(2).context("give the 24 words, in quotes")?;
            enrol(&directory(&args)?, phrase)
        }
        "pair" => block_on(pair(directory(&args)?)),
        "join" => {
            let code = args.get(2).context("give the code the other device is showing")?.clone();
            block_on(join(directory(&args)?, code))
        }
        "run" => block_on(start(directory(&args)?)),
        "status" => status(&directory(&args)?),
        "verify" => verify(&directory(&args)?, args.iter().any(|a| a == "--deep")),
        "config" => configure(&directory(&args)?, &args[2..]),
        "signal" => block_on(signal(args.get(1).cloned())),
        "relay" => block_on(relay(args.get(1).cloned())),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print!("{USAGE}");
            std::process::exit(2);
        }
    }
}

fn directory(args: &[String]) -> Result<PathBuf> {
    let dir = args.get(1).context("give the directory to sync")?;
    Ok(PathBuf::from(dir))
}

/// The multi-threaded scheduler is required, not preferred: storage work runs
/// inside `block_in_place`, which the current-thread scheduler cannot do.
fn block_on<F: std::future::Future<Output = Result<()>>>(future: F) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the runtime")?
        .block_on(future)
}

fn store_dir(root: &Path) -> PathBuf {
    root.join(".qurb")
}

/// Open everything a command needs, failing clearly if the device is not set up.
fn open(root: &Path) -> Result<(MasterKey, Identity, Store, Config)> {
    let store_dir = store_dir(root);
    let vault = Vault::at(&store_dir);
    if !vault.exists() {
        bail!(
            "{} is not set up yet — run `qurb init {}` first",
            root.display(),
            root.display()
        );
    }

    let master = vault.open_or_create()?.key().clone();
    let identity = Identity::load_or_create(&store_dir)?;
    let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
    let store = Store::open(&store_dir, chunk_key)?;
    let config = Config::load(&store_dir)?;
    Ok((master, identity, store, config))
}

// -- commands ----------------------------------------------------------------

fn init(root: &Path) -> Result<()> {
    let store_dir = store_dir(root);
    std::fs::create_dir_all(root)
        .with_context(|| format!("creating {}", root.display()))?;

    let vault = Vault::at(&store_dir);
    if vault.exists() {
        bail!("{} already has a key. Use `qurb status` to see it.", root.display());
    }

    match vault.open_or_create()? {
        Opened::Created { key, phrase } => {
            Identity::load_or_create(&store_dir)?;
            Config::default().save(&store_dir)?;
            let chunk_key =
                ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
            Store::open(&store_dir, chunk_key)?;

            println!("Set up {}\n", root.display());
            println!("{}", "=".repeat(68));
            println!("{}", phrase.numbered());
            println!("{}", "=".repeat(68));
            println!();
            println!("Write these 24 words down on paper, in order, now.");
            println!();
            println!("They are not a backup of your key. They ARE your key, in a form you");
            println!("can hold. Nobody else has a copy — not us, not a server. If you lose");
            println!("them and lose this device, your files cannot be recovered by anyone,");
            println!("including us. That is not a policy we could choose to relax.");
            println!();
            println!("To add another device:");
            println!("  qurb enrol <dir> \"{} ...\"", phrase.words()[..3].join(" "));
            Ok(())
        }
        Opened::Existing(_) => bail!("a key appeared while we were creating one"),
    }
}

fn enrol(root: &Path, phrase: &str) -> Result<()> {
    let store_dir = store_dir(root);
    std::fs::create_dir_all(root)
        .with_context(|| format!("creating {}", root.display()))?;

    let phrase = RecoveryPhrase::parse(phrase).context("that is not a valid recovery phrase")?;
    let key = Vault::at(&store_dir)
        .restore(&phrase)
        .context("installing the key")?;

    Identity::load_or_create(&store_dir)?;
    Config::default().save(&store_dir)?;
    let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
    Store::open(&store_dir, chunk_key)?;

    println!("Set up {} with an existing key.\n", root.display());
    println!("This device shares a key with your others, which is what makes them");
    println!("yours. They still have to be introduced: run `qurb pair` on one and");
    println!("`qurb join` here with the code it shows.");
    Ok(())
}

async fn pair(root: PathBuf) -> Result<()> {
    let (_, identity, store, config) = open(&root)?;
    let store = Arc::new(Mutex::new(store));

    let host = PairingHost::open(
        format!("0.0.0.0:{}", config.port).parse()?,
        &identity,
        now(),
    )?;

    println!("On the other device, run:\n");
    println!("  qurb join <dir> {}\n", host.invite().encode());
    println!("Or read this out:\n");
    println!("  {}\n", host.invite().for_humans());
    println!("The code carries this device's full identity, which is why it has to");
    println!("travel outside the network — someone able to change what is on your");
    println!("screen has already won.\n");
    println!("It expires in 5 minutes and works once. Waiting...");

    let peer = host.wait(Arc::clone(&store), &config.name, now()).await?;
    println!("\nPaired with {} ({})", peer.name, peer.fingerprint.short());
    Ok(())
}

async fn join(root: PathBuf, code: String) -> Result<()> {
    let (_, identity, store, config) = open(&root)?;
    let store = Arc::new(Mutex::new(store));

    let invite = qurb_peer::Invite::parse(&code).context("that is not a valid pairing code")?;
    println!("Joining {} ...", invite.address);

    let peer = qurb_peer::accept(&invite, &identity, store, &config.name, now()).await?;
    println!("Paired with {} ({})", peer.name, peer.fingerprint.short());
    Ok(())
}

async fn start(root: PathBuf) -> Result<()> {
    let (master, identity, store, config) = open(&root)?;
    drop(store);

    println!("qurb: syncing {}", root.display());
    println!("  identity  {}", identity.fingerprint().short());
    println!("  signal    {}", config.signal);
    match config.relay {
        Some(relay) => println!("  relay     {relay}"),
        None => println!("  relay     none — devices must reach each other directly"),
    }
    println!();

    Daemon::new(&root, &store_dir(&root), master, identity, config).run().await
}

fn status(root: &Path) -> Result<()> {
    let (_, identity, store, config) = open(root)?;

    let live = store.db().live_paths()?;
    let (plaintext, stored) = store.db().size_totals()?;
    let chunks = store.db().chunk_count()?;

    println!("{}", root.display());
    println!("  identity   {}", identity.fingerprint().short());
    println!("  name       {}", config.name);
    println!("  files      {}", live.len());
    println!("  chunks     {chunks}");
    println!("  content    {} ({} on disk)", human(plaintext), human(stored));

    let peers = store.db().trusted_peers()?;
    if peers.is_empty() {
        println!("\n  no paired devices — run `qurb pair` here and `qurb join` there");
    } else {
        println!("\n  paired with:");
        for peer in peers {
            println!(
                "    {}  {}  ({})",
                hex_short(&peer.fingerprint),
                peer.name,
                match peer.last_seen {
                    Some(at) => format!("last reached {}", ago(at)),
                    None => "not reached yet".to_string(),
                }
            );
        }
    }

    let collisions = qurb_watcher::case_collisions(&live);
    if !collisions.is_empty() {
        println!("\n  warning: these differ only in case, and a macOS or Windows");
        println!("  device could not hold both:");
        for group in collisions {
            println!("    {}", group.join("  "));
        }
    }
    Ok(())
}

fn verify(root: &Path, deep: bool) -> Result<()> {
    let (_, _, store, _) = open(root)?;

    println!("Checking {}{}", root.display(), if deep { " (reading every chunk)" } else { "" });
    let report = store.verify(deep)?;

    if report.is_healthy() && report.orphaned.is_empty() {
        println!("  everything agrees");
        return Ok(());
    }

    if !report.missing.is_empty() {
        println!("  {} chunk(s) referenced but not on disk", report.missing.len());
        println!("    these files cannot be read here; `qurb run` will refetch them from a peer");
    }
    if !report.corrupt.is_empty() {
        println!("  {} chunk(s) damaged", report.corrupt.len());
        println!("    same: a peer holding the same content can replace them");
    }
    if !report.refcount_drift.is_empty() {
        println!("  {} reference count(s) disagree — please report this", report.refcount_drift.len());
    }
    if !report.orphaned.is_empty() {
        println!("  {} chunk(s) on disk that nothing references (wasted space, not damage)", report.orphaned.len());
    }

    if !report.is_healthy() {
        std::process::exit(1);
    }
    Ok(())
}

fn configure(root: &Path, settings: &[String]) -> Result<()> {
    let store_dir = store_dir(root);
    let mut config = Config::load(&store_dir)?;

    if settings.is_empty() {
        println!("signal = {}", config.signal);
        println!("relay  = {}", config.relay.map(|r| r.to_string()).unwrap_or_default());
        println!("name   = {}", config.name);
        println!("port   = {}", config.port);
        println!("\n{}", Config::path(&store_dir).display());
        return Ok(());
    }

    for setting in settings {
        let (key, value) = setting
            .split_once('=')
            .with_context(|| format!("expected key=value, got `{setting}`"))?;
        match key.trim() {
            "signal" => config.signal = value.trim().to_string(),
            "relay" => {
                config.relay = if value.trim().is_empty() {
                    None
                } else {
                    Some(value.trim().parse().context("relay should be address:port")?)
                }
            }
            "name" => config.name = value.trim().to_string(),
            "port" => config.port = value.trim().parse().context("port should be a number")?,
            other => bail!("unknown setting `{other}`"),
        }
    }

    config.save(&store_dir)?;
    println!("saved to {}", Config::path(&store_dir).display());
    Ok(())
}

/// Run the rendezvous service.
///
/// It introduces devices to each other and tells both to punch at the same
/// moment. It never learns a filename, and the identifiers devices announce
/// under are derived from a key it does not hold, so it cannot tell whose
/// devices these are.
async fn signal(addr: Option<String>) -> Result<()> {
    let addr: std::net::SocketAddr =
        addr.unwrap_or_else(|| "0.0.0.0:9000".into()).parse().context("bad address")?;
    let server = qurb_signal::SignalServer::bind(addr).await?;

    println!("rendezvous service on {}", server.local_addr()?);
    println!();
    println!("Devices reach it as  ws://<this machine>:{}", server.local_addr()?.port());
    println!();
    println!("Put it behind TLS before it faces the internet. The identifiers");
    println!("devices announce under are bearer secrets: anyone who sees one can");
    println!("list that group's addresses. The client refuses plain ws:// to");
    println!("anywhere but this machine for exactly that reason.");
    server.serve().await;
    Ok(())
}

/// Run the relay.
///
/// It forwards bytes between devices that cannot reach each other directly. A
/// full encrypted session runs inside, so it can neither read what it carries
/// nor forge it.
async fn relay(addr: Option<String>) -> Result<()> {
    let addr: std::net::SocketAddr =
        addr.unwrap_or_else(|| "0.0.0.0:9001".into()).parse().context("bad address")?;
    let server = Arc::new(qurb_relay::RelayServer::bind(addr).await?);

    println!("relay on {}", server.local_addr()?);
    println!();
    println!("Devices use it as  relay = <this machine>:{}", server.local_addr()?.port());
    println!();
    println!("It carries ciphertext it has no key for. Every byte through it is a");
    println!("byte somebody pays for, so it is the fallback rather than the path.");

    // Say what it has carried, since that is the number that becomes a bill.
    let stats = server.stats();
    tokio::spawn(async move {
        let mut last = 0u64;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let now = stats.bytes();
            if now != last {
                println!("  carried {} in total", human(now));
                last = now;
            }
        }
    });

    server.serve().await;
    Ok(())
}

// -- odds and ends -----------------------------------------------------------

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// How long ago, roughly. Precision past "a few minutes" helps nobody.
fn ago(then: i64) -> String {
    let seconds = (now() - then).max(0);
    match seconds {
        0..=90 => "just now".to_string(),
        91..=5400 => format!("{} minutes ago", seconds / 60),
        5401..=172_800 => format!("{} hours ago", seconds / 3600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

fn hex_short(bytes: &[u8; 32]) -> String {
    bytes[..4].iter().map(|b| format!("{b:02x}")).collect()
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
