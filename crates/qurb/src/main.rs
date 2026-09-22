//! qurb — private cloud storage.
//!
//! The terminal front end. The daemon itself lives in the library beside this,
//! so that something other than a terminal can run the same one — an interface
//! should display the engine rather than reimplement it.

use anyhow::{bail, Context, Result};
use qurb_cli::config::Config;
use qurb_cli::daemon::Daemon;
use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_peer::{Identity, PairingHost};
use qurb_storage::{ChunkKey, Store};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const USAGE: &str = "\
qurb — private cloud storage

  qurb init [dir]                     set up a device and create a key
                                    (defaults to ~/Downloads/qurb)
  qurb enrol <dir> \"<24 words>\"       set up a device with an existing key
  qurb pair [dir]                     show a code and wait for a device to join
  qurb join <dir> <code>              join a device that is showing a code
  qurb run [dir]                      watch, sync, and keep running
  qurb status [dir]                   what this device holds and trusts
  qurb verify [dir] [--deep]          check the store against itself
  qurb reclaim [dir]                  free space the folder itself already holds
  qurb config [dir] [key=value ...]   show or change settings
  qurb protect [dir] <how>            change how the key is kept
                                        file | keystore | passphrase

Running the services yourself:

  qurb signal [addr]                  the rendezvous service (default :9000)
  qurb relay [addr]                   the relay (default :9001)
  qurb netcheck                       what this network will let you do

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
        "init" => init(&new_directory(&args)?),
        "enrol" | "enroll" => {
            let phrase = args.get(2).context("give the 24 words, in quotes")?;
            enrol(&new_directory(&args)?, phrase)
        }
        "pair" => block_on(pair(directory(&args)?)),
        "join" => {
            let code = args.get(2).context("give the code the other device is showing")?.clone();
            block_on(join(directory(&args)?, code))
        }
        "run" => block_on(start(directory(&args)?)),
        "status" => status(&directory(&args)?),
        "verify" => verify(&directory(&args)?, args.iter().any(|a| a == "--deep")),
        "reclaim" => reclaim(&directory(&args)?),
        "config" => configure(&directory(&args)?, &args[2..]),
        "protect" => protect(&directory(&args)?, args.get(2).map(String::as_str)),
        "signal" => block_on(signal(args.get(1).cloned())),
        "relay" => block_on(relay(args.get(1).cloned())),
        "netcheck" => netcheck(),
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

/// Which folder a command should act on.
///
/// A path if one was given. Otherwise the folder this person last used, or the
/// default location if it is set up — so the common case is `qurb status` with
/// no arguments rather than retyping a path every time.
fn directory(args: &[String]) -> Result<PathBuf> {
    if let Some(dir) = args.get(1) {
        return Ok(PathBuf::from(dir));
    }
    qurb_cli::profiles::current().context(
        "no folder given, and none is set up yet.\n\
         Run `qurb init` to make one, or pass a path.",
    )
}

/// Where `qurb init` should put a folder when told no path.
fn new_directory(args: &[String]) -> Result<PathBuf> {
    match args.get(1) {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => qurb_cli::profiles::default_root(),
    }
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

    let master = match vault.protection()? {
        qurb_keys::Protection::Passphrase => {
            let passphrase = prompt_passphrase("Passphrase: ")?;
            vault.unlock(Some(&passphrase))?
        }
        _ => vault.unlock(None)?,
    };
    let identity = Identity::load_or_create(&store_dir)?;
    let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
    // Attach the sync folder, not just the store inside it. The folder holds
    // the payloads for every file it materialises, so a store opened without
    // it cannot read that content at all -- `verify` would call it missing and
    // `reclaim` would find nothing to free.
    let store = Store::open(&store_dir, chunk_key)?.in_tree(root);
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
            // Recorded so later commands can be run with no path at all.
    let _ = qurb_cli::profiles::remember(root);

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
    let _ = qurb_cli::profiles::remember(root);

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

    let code = host.invite().encode();

    // The QR first, because it is the way anyone will actually do this. The
    // text below it is the fallback for a device with no camera, and for
    // reading aloud down a phone line.
    match qurb_cli::qr::terminal(&code) {
        Ok(rendered) => {
            println!("Scan this with the qurb app on your phone:\n");
            print!("{rendered}");
            println!();
        }
        Err(e) => {
            // Not fatal. The code still works typed.
            tracing::debug!(error = %e, "could not render a QR code");
        }
    }

    println!("Or on another computer:\n");
    println!("  qurb join <dir> {code}\n");
    println!("Or read this out:\n");
    println!("  {}\n", host.invite().for_humans());
    println!("The code carries this device's full identity, which is why it has to");
    println!("travel outside the network — someone able to change what is on your");
    println!("screen has already won.\n");
    println!("It expires in 5 minutes and works once. Waiting...");

    match host.wait(Arc::clone(&store), &config.name, now()).await {
        Ok(peer) => {
            println!("\nPaired with {} ({})", peer.name, peer.fingerprint.short());
            Ok(())
        }
        // Said plainly rather than as an error trace. This is the ordinary
        // ending when nobody types the code in time, and the only useful thing
        // to tell someone is that the code is dead and how to get another.
        Err(qurb_peer::Error::InviteExpired) => {
            println!("\nThat code has expired. Run `qurb pair {}` again for a new one.", root.display());
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
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
    println!("  key kept   {}", Vault::at(&store_dir(root)).protection()?.as_str());
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

    // Accented names written two different ways. Only visible on a filesystem
    // that keeps them apart -- which is to say, only on the device where they
    // can still be renamed.
    let ignore = qurb_watcher::IgnoreRules::new().with_store_dir(store_dir(root));
    if let Ok(entries) = qurb_watcher::scan(root, &ignore) {
        let groups = qurb_watcher::normalization_collisions(&entries);
        if !groups.is_empty() {
            println!("\n  warning: these names are the same text written two ways, so");
            println!("  only the first of each is synced. Rename the others to fix it:");
            for group in groups {
                println!("    keeping  {}", group[0].path.display());
                for other in &group[1..] {
                    println!("    skipping {}", other.path.display());
                }
            }
        }
    }
    Ok(())
}

/// Drop encrypted copies of content the sync folder itself holds.
///
/// A store written before the single-copy rule kept both, and nothing
/// re-indexes a file that has not changed, so the duplicates need asking for.
fn reclaim(root: &Path) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;

    println!("Looking for content {} already holds", root.display());
    let freed = store.reclaim()?;

    if freed.chunks_removed == 0 {
        println!("  nothing to free — no second copies found");
    } else {
        println!(
            "  freed {} across {} chunk(s)",
            human(freed.bytes_reclaimed),
            freed.chunks_removed
        );
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

/// Change how the key is kept.
///
/// The key itself does not change, so nothing it protects becomes unreadable —
/// this changes the lock, not the contents.
fn protect(root: &Path, how: Option<&str>) -> Result<()> {
    use qurb_keys::Protection;

    let store_dir = store_dir(root);
    let vault = Vault::at(&store_dir);
    if !vault.exists() {
        bail!("{} is not set up yet", root.display());
    }

    let current = vault.protection()?;
    let Some(how) = how else {
        println!("This key is kept: {}", current.as_str());
        println!();
        println!("  file        a file only you can read. Protects against other users");
        println!("              of this machine, and against nothing that can read the");
        println!("              disk — a stolen laptop, an unencrypted backup.");
        println!("  keystore    the operating system's own store. {}",
            if qurb_keys::keystore_available() { "Available here." } else { "NOT available here." });
        println!("  passphrase  wrapped with something only you know. The only option");
        println!("              that survives someone taking the disk, and the only one");
        println!("              that cannot start unattended.");
        println!();
        println!("  qurb protect {} <how>", root.display());
        return Ok(());
    };

    let wanted: Protection = how.parse()?;
    if wanted == current {
        println!("Already kept in the {}.", current.as_str());
        return Ok(());
    }

    if wanted == Protection::Keystore && !qurb_keys::keystore_available() {
        bail!(
            "this machine has no usable keystore — a device that cannot unlock \
             itself is worse than one whose key sits in a file"
        );
    }

    // `platform` parses, because the vault format has it, and means nothing
    // here: it exists for phones, where the key is held by an app-supplied
    // store because neither Android's keystore nor iOS's Keychain is reachable
    // from Rust. Left to itself this would fail deep inside the vault with a
    // message about a store that was not supplied, which is true and unhelpful.
    if wanted == Protection::Platform {
        bail!(
            "`platform` is for phones, where an app supplies the keystore. \
             On a desktop use `keystore`, which is the same idea through the \
             operating system's own store."
        );
    }

    let current_passphrase = if current.needs_passphrase() {
        Some(prompt_passphrase("Current passphrase: ")?)
    } else {
        None
    };

    let new_passphrase = if wanted.needs_passphrase() {
        let first = prompt_passphrase("New passphrase: ")?;
        if first.trim().is_empty() {
            bail!("an empty passphrase protects nothing");
        }
        let again = prompt_passphrase("Again: ")?;
        if first != again {
            bail!("those do not match");
        }
        Some(first)
    } else {
        None
    };

    vault.protect(wanted, current_passphrase.as_deref(), new_passphrase.as_deref())?;

    println!("This key is now kept: {}", wanted.as_str());
    if wanted.needs_passphrase() {
        println!();
        println!("`qurb run` will ask for it at startup, so this device can no longer");
        println!("start unattended. Your recovery phrase is unaffected — it recovers the");
        println!("key, while the passphrase guards the copy on this disk.");
    }
    Ok(())
}

/// Read a passphrase without echoing it.
///
/// Falls back to a visible prompt where the terminal cannot be put into
/// no-echo mode, saying so, rather than silently showing what was typed.
fn prompt_passphrase(prompt: &str) -> Result<String> {
    use std::io::{BufRead, Write};

    eprint!("{prompt}");
    std::io::stderr().flush().ok();

    let hidden = set_echo(false);
    if !hidden {
        eprintln!("\n  (this terminal will show what you type)");
        eprint!("{prompt}");
        std::io::stderr().flush().ok();
    }

    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    if hidden {
        set_echo(true);
        eprintln!();
    }
    read.context("reading the passphrase")?;

    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(unix)]
fn set_echo(on: bool) -> bool {
    // Done with stty rather than a terminal crate: one command, no dependency,
    // and it fails visibly on anything that is not a terminal.
    std::process::Command::new("stty")
        .arg(if on { "echo" } else { "-echo" })
        .stdin(std::process::Stdio::inherit())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn set_echo(_on: bool) -> bool {
    false
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

/// Classify the router between this machine and the internet.
///
/// Whether devices can reach each other directly, or whether their traffic has
/// to be paid for on a relay. Worth running on every network that matters —
/// home, phone hotspot, office, café — because the worst answer is the one that
/// sets the bill.
fn netcheck() -> Result<()> {
    use qurb_peer::nat::{self, NatBehaviour};

    let socket = std::net::UdpSocket::bind("0.0.0.0:0").context("opening a socket")?;
    println!("local   {}", socket.local_addr()?);
    println!();

    // One socket for every query: the question is whether the external address
    // depends on who is being asked, which cannot be answered by asking one
    // server or by asking from two sockets.
    let mut servers = Vec::new();
    for name in nat::DEFAULT_STUN_SERVERS {
        use std::net::ToSocketAddrs;
        match name.to_socket_addrs() {
            Ok(mut addrs) => match addrs.find(|a| a.is_ipv4()) {
                Some(addr) => servers.push((name, addr)),
                None => println!("  {name:<28} no IPv4 address"),
            },
            Err(e) => println!("  {name:<28} could not resolve: {e}"),
        }
    }

    let mut seen = Vec::new();
    for (name, addr) in &servers {
        match nat::reflexive_address(&socket, *addr, std::time::Duration::from_secs(3)) {
            Ok(public) => {
                println!("  {name:<28} -> {public}");
                seen.push(public);
            }
            Err(e) => println!("  {name:<28} -> no answer: {e}"),
        }
    }

    let behaviour = match seen.len() {
        0 => NatBehaviour::Blocked,
        1 => NatBehaviour::Inconclusive,
        _ if seen.windows(2).all(|w| w[0] == w[1]) => NatBehaviour::EndpointIndependent,
        _ => NatBehaviour::Symmetric,
    };

    println!();
    match behaviour {
        NatBehaviour::EndpointIndependent => {
            println!("This network allows direct connections.");
            println!("  The same external address whoever is asked, so the router's mapping");
            println!("  does not depend on the destination and hole punching should work.");
        }
        NatBehaviour::Symmetric => {
            println!("This network needs a relay.");
            println!("  A different external port per destination, so the address a peer");
            println!("  learns is not the address it can reach. One end like this is");
            println!("  survivable if the other is not; two is not.");
        }
        NatBehaviour::Blocked => {
            println!("Nothing answered — UDP may be blocked outbound.");
            println!("  Every connection from here would need a relay. Worth retrying:");
            println!("  an outage looks exactly the same.");
        }
        NatBehaviour::Inconclusive => {
            println!("Inconclusive — only one server answered.");
            println!("  One answer cannot say whether the mapping depends on the");
            println!("  destination. Retry.");
        }
    }
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
