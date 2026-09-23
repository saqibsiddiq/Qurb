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
  qurb replica <dir> [--only <path>]  hold content for devices that are asleep
  qurb status [dir]                   what this device holds and trusts
  qurb verify [dir] [--deep]          check the store against itself
  qurb reclaim [dir]                  free space the folder itself already holds
  qurb fetch [dir] <path>             ask for a dropped file's contents back
  qurb send [dir] <file> to <device>  send a file to one device, privately
  qurb activity [dir] [path]          what happened, newest first
  qurb ls [dir] [path]                what this folder holds, and where
  qurb find [dir] <text>              files whose name contains something
  qurb config [dir] [key=value ...]   show or change settings
  qurb protect [dir] <how>            change how the key is kept
                                        file | keystore | passphrase

Running the services yourself:

  qurb signal [addr] [--push <json>]  the rendezvous service (default :9000)
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
        "replica" => {
            let only: Vec<String> = args
                .iter()
                .skip_while(|a| a.as_str() != "--only")
                .skip(1)
                .take(1)
                .cloned()
                .collect();
            block_on(replica(directory(&args)?, only))
        }
        "status" => status(&directory(&args)?),
        "verify" => verify(&directory(&args)?, args.iter().any(|a| a == "--deep")),
        "reclaim" => reclaim(&directory(&args)?),
        "fetch" => {
            // Both `qurb fetch <path>` and `qurb fetch <dir> <path>`. The file
            // path always comes last; whether a folder was named is what the
            // count tells us.
            let wanted = args.last().context("give a path to fetch")?.clone();
            let root = match args.len() {
                0 | 1 => bail!("give a path to fetch"),
                2 => qurb_cli::profiles::current().context(
                    "no folder given, and none is set up yet.\n\
                     Run `qurb init` to make one, or pass a path.",
                )?,
                _ => PathBuf::from(&args[1]),
            };
            fetch(&root, &wanted)
        }
        "send" => {
            // `qurb send report.pdf to laptop`, or with an explicit directory
            // first. The word `to` is what tells the two apart, so the
            // arguments either side of it are found rather than counted.
            let rest = &args[1..];
            let at = rest
                .iter()
                .position(|a| a == "to")
                .context("say which device: qurb send <file> to <device>")?;
            let (before, after) = rest.split_at(at);
            let recipient =
                after.get(1).context("say which device: qurb send <file> to <device>")?;
            let file = before.last().context("give a file to send")?;
            let root = match before.len() {
                0 | 1 => directory(&args)?,
                _ => PathBuf::from(&before[0]),
            };
            send(&root, Path::new(file), recipient)
        }
        "activity" => {
            let (root, about) = split_path(&args)?;
            activity(&root, about.as_deref())
        }
        "ls" | "list" => {
            let (root, under) = split_path(&args)?;
            list(&root, under.as_deref())
        }
        "find" | "search" => {
            let (root, text) = split_path(&args)?;
            let text = text.context("give something to look for")?;
            find(&root, &text)
        }
        "config" => configure(&directory(&args)?, &args[2..]),
        "protect" => protect(&directory(&args)?, args.get(2).map(String::as_str)),
        "signal" => {
            let credentials = args
                .iter()
                .skip_while(|a| a.as_str() != "--push")
                .nth(1)
                .cloned();
            let addr = args.get(1).filter(|a| !a.starts_with("--")).cloned();
            block_on(signal(addr, credentials))
        }
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

/// Hold content so that other devices do not have to be awake together.
///
/// The device this system most needs and the one nobody wants to be: always
/// on, holding chunks it cannot read, so a phone can send a photo at midnight
/// and a laptop can collect it on Tuesday. See
/// [decision 0006](../../docs/decisions/0006-availability-gap.md).
///
/// It has no folder. The directory given is where its store lives, and nothing
/// is ever materialised inside it — a replica that wrote files out would be a
/// second copy of someone's library on a machine they do not sit at, which is
/// the opposite of the point.
async fn replica(root: PathBuf, only: Vec<String>) -> Result<()> {
    let (master, identity, store, config) = open(&root)?;
    drop(store);

    let pins = if only.is_empty() {
        qurb_engine::PinSet::everything()
    } else {
        qurb_engine::PinSet::under(only.clone())
    };

    println!("qurb: holding content at {}", root.display());
    println!("  identity  {}", identity.fingerprint().short());
    println!("  signal    {}", config.signal);
    match pins.is_everything() {
        true => println!("  holding   everything its peers have"),
        false => println!("  holding   only {}", only.join(", ")),
    }
    println!();
    println!("  This device stores content it cannot read, and shows nobody any files.");
    println!("  Nothing is written into {} except the store itself.", root.display());
    println!();

    Daemon::new(&root, &store_dir(&root), master, identity, config)
        .holding(pins)
        .run()
        .await
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

    let usage = store.usage()?;
    let evicted = store.evicted()?;
    if config.limit == 0 {
        println!("  using      {} — no limit set", human(usage.total()));
    } else {
        let percent = (usage.total() as f64 / config.limit as f64 * 100.0).round();
        println!(
            "  using      {} of {} ({percent:.0}%)",
            human(usage.total()),
            human(config.limit)
        );
        if usage.total() > config.limit {
            println!("    over the limit, and holding content nothing else has —");
            println!("    qurb keeps the only copy of a file rather than honour a number");
        }
    }
    if !evicted.is_empty() {
        println!("  not here   {} file(s) — contents dropped, `qurb fetch` to get one back", evicted.len());
    }

    // Files this device made that no other device is known to hold. Worth
    // saying out loud: while this is non-empty, losing this device loses work.
    let waiting = store.undelivered()?;
    if !waiting.is_empty() {
        let bytes: u64 = waiting.iter().map(|(_, size)| size).sum();
        println!("  only here  {} file(s), {} — no other device has these yet", waiting.len(), human(bytes));
        for (path, _) in waiting.iter().take(3) {
            println!("               {path}");
        }
        if waiting.len() > 3 {
            println!("               and {} more", waiting.len() - 3);
        }
    }

    let peers = store.db().trusted_peers()?;

    // Files sent to another device that it has not collected. These are not in
    // the folder and appear in none of the counts above, so without this line
    // a send is invisible until it lands.
    let sent = store.pending_deliveries()?;
    if !sent.is_empty() {
        let bytes: u64 = sent.iter().map(|(_, size, _)| size).sum();
        println!("  sending    {} file(s), {} — not collected yet", sent.len(), human(bytes));
        for (path, _, to) in sent.iter().take(3) {
            let name = peers
                .iter()
                .find(|p| &p.device_id == to)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| to.short());
            println!("               {path} → {name}");
        }
        if sent.len() > 3 {
            println!("               and {} more", sent.len() - 3);
        }
    }

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

/// Ask for the contents of a file this device dropped to stay under its limit.
///
/// Records the request rather than performing it. The daemon is a different
/// process and may not be running, and even if it is, no peer may be reachable
/// right now. A request written to the index is acted on whenever one next is —
/// which is also what makes it work to ask for a file while offline.
fn fetch(root: &Path, logical: &str) -> Result<()> {
    let (_, _, store, _) = open(root)?;
    let logical = logical.trim_start_matches("./");

    match store.is_materialised(logical)? {
        None => bail!("{logical} is not a file this folder knows about"),
        Some(true) => {
            println!("{logical} is already here");
            return Ok(());
        }
        Some(false) => {}
    }

    store.db().want(logical)?;
    println!("asked for {logical}");
    println!("  it will arrive the next time a device holding it is reachable");
    Ok(())
}

/// Put a file into one device's private vault.
///
/// Not a copy into the shared folder: the file goes to that device and to no
/// other, and nothing about it is advertised to the rest of the fleet. The
/// bytes are held here until the recipient confirms it arrived, which is what
/// makes sending to a phone that is switched off work at all.
///
/// The recipient is named the way the user names it — `qurb status` shows the
/// list. A short id is accepted too, so that two devices sharing a name can
/// still be told apart; both the fingerprint shown by `qurb status` and the
/// device id work, because a person reading either should not have to know
/// which one they are looking at.
fn send(root: &Path, file: &Path, recipient: &str) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;

    let peers = store.db().trusted_peers()?;
    let matches: Vec<_> = peers
        .iter()
        .filter(|p| {
            p.name.eq_ignore_ascii_case(recipient)
                || hex_short(&p.fingerprint).eq_ignore_ascii_case(recipient)
                || p.device_id.short().eq_ignore_ascii_case(recipient)
        })
        .collect();

    let peer = match matches.as_slice() {
        [one] => *one,
        [] => {
            let known: Vec<String> = peers
                .iter()
                .map(|p| format!("{} ({})", p.name, hex_short(&p.fingerprint)))
                .collect();
            if known.is_empty() {
                bail!("this device has not been paired with anything yet");
            }
            bail!("no paired device called {recipient} — known: {}", known.join(", "));
        }
        several => {
            let ids: Vec<String> = several.iter().map(|p| hex_short(&p.fingerprint)).collect();
            bail!("more than one device is called {recipient} — use one of: {}", ids.join(", "));
        }
    };

    let name = file
        .file_name()
        .context("give a file, not a directory")?
        .to_string_lossy()
        .into_owned();

    let stats = store.send_to_vault(&name, file, &peer.device_id)?;
    println!("sending {name} to {} ({})", peer.name, hex_short(&peer.fingerprint));
    println!("  {} stored, waiting for the device to collect it", human(stats.bytes_written));
    println!("  it stays here until then, even if this device restarts");
    Ok(())
}

/// Split `<command> [dir] [argument]` into the two, without a flag to say which.
///
/// A single argument is ambiguous — `qurb ls photos` could mean a directory to
/// open or a folder inside one — and asking people to remember an order they
/// never think about is worse than looking. A directory is a directory on disk
/// that has a store in it; anything else is the argument.
fn split_path(args: &[String]) -> Result<(PathBuf, Option<String>)> {
    match &args[1..] {
        [] => Ok((directory(args)?, None)),
        [one] => match PathBuf::from(one).join(".qurb").is_dir() {
            true => Ok((PathBuf::from(one), None)),
            false => Ok((directory(args)?, Some(one.clone()))),
        },
        [dir, rest, ..] => Ok((PathBuf::from(dir), Some(rest.clone()))),
    }
}

/// What this folder holds, and whether the bytes are actually here.
fn list(root: &Path, under: Option<&str>) -> Result<()> {
    const PAGE: usize = 200;
    let (_, _, store, _) = open(root)?;
    let limit = Config::load(&qurb_cli::store_dir(root)).map(|c| c.limit).unwrap_or(0);
    let view = qurb_cli::View::new(&store, limit);

    let under = under.map(|u| u.trim_start_matches("./").to_string());
    let files = view.files(under.as_deref(), PAGE, 0)?;
    if files.is_empty() {
        match &under {
            Some(path) => println!("nothing under {path}"),
            None => println!("this folder is empty"),
        }
        return Ok(());
    }

    for file in &files {
        println!("  {:<9} {:>10}  {}", mark(file.availability), human(file.size), file.path);
    }

    let total = view.storage()?.file_count;
    if under.is_none() && total > files.len() {
        println!("\n  showing {} of {total} — name a folder to narrow it", files.len());
    }
    Ok(())
}

/// Files whose name contains something.
fn find(root: &Path, text: &str) -> Result<()> {
    const SHOWN: usize = 100;
    let (_, _, store, _) = open(root)?;
    let view = qurb_cli::View::new(&store, 0);

    let hits = view.search(text, SHOWN)?;
    if hits.is_empty() {
        println!("nothing matching {text}");
        println!("  names only, and case is folded for ASCII — `CAFÉ` will not find `café`");
        return Ok(());
    }
    for file in &hits {
        println!("  {:<9} {:>10}  {}", mark(file.availability), human(file.size), file.path);
    }
    if hits.len() == SHOWN {
        println!("\n  stopped at {SHOWN} — narrow it to see the rest");
    }
    Ok(())
}

/// One word for where a file's bytes are.
///
/// "only here" is the one worth a column of its own: it means losing this
/// device loses the file, and it is otherwise indistinguishable from a file
/// that is safely on three devices.
fn mark(availability: qurb_cli::Availability) -> &'static str {
    match availability {
        qurb_cli::Availability::Here => "here",
        qurb_cli::Availability::Elsewhere => "not here",
        qurb_cli::Availability::OnlyHere => "only here",
    }
}

/// What this device did, newest first.
///
/// The daemon's account of itself used to be its log, which is gone the moment
/// the process is. This reads the history the index keeps, so it answers after
/// a restart — and with a path, it answers the question the log never could:
/// why is this file not here?
fn activity(root: &Path, about: Option<&str>) -> Result<()> {
    const SHOWN: usize = 40;
    let (_, _, store, _) = open(root)?;

    let rows = match about {
        Some(path) => store.db().activity_for(path.trim_start_matches("./"), SHOWN)?,
        None => store.db().activity(SHOWN, None)?,
    };

    if rows.is_empty() {
        match about {
            Some(path) => println!("nothing recorded about {path}"),
            None => println!("nothing recorded yet"),
        }
        return Ok(());
    }

    // Names, so a line reads "sent to phone" rather than as a hex string.
    let peers = store.db().trusted_peers()?;
    let name_of = |id: &qurb_sync::DeviceId| {
        peers
            .iter()
            .find(|p| &p.device_id == id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| id.short())
    };

    for row in rows {
        let who = row.device.as_ref().map(&name_of);
        let subject = match (&row.path, &who) {
            (Some(path), Some(name)) => format!("{path}  ({name})"),
            (Some(path), None) => path.clone(),
            (None, Some(name)) => name.clone(),
            (None, None) => String::new(),
        };
        let size = row.size.map(|n| format!("  {}", human(n))).unwrap_or_default();
        println!("  {:>14}  {:<10} {subject}{size}", ago(row.at), row.kind.as_str());
        if let Some(detail) = &row.detail {
            println!("                                {detail}");
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
        println!(
            "limit  = {}",
            if config.limit == 0 {
                "none".to_string()
            } else {
                qurb_cli::config::human_size(config.limit)
            }
        );
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
            "limit" => config.limit = qurb_cli::config::parse_size(value)?,
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
async fn signal(addr: Option<String>, push_credentials: Option<String>) -> Result<()> {
    let addr: std::net::SocketAddr =
        addr.unwrap_or_else(|| "0.0.0.0:9000".into()).parse().context("bad address")?;
    // Validated before the port is taken, so a mistyped credentials path says
    // so rather than surfacing as whatever the socket complains about.
    let waker = push_waker(push_credentials).await?;
    let server = qurb_signal::SignalServer::bind(addr).await?;
    let server = match waker {
        Some(waker) => server.waking_with(waker),
        None => server,
    };

    println!("rendezvous service on {}", server.local_addr()?);
    println!();
    println!("Devices reach it as  ws://<this machine>:{}", server.local_addr()?.port());
    println!();
    println!("Put it behind TLS before it faces the internet. The identifiers");
    println!("devices announce under are bearer secrets: anyone who sees one can");
    println!("list that group's addresses. The client refuses plain ws:// to");
    println!("anywhere but the local network for exactly that reason.");
    server.serve().await;
    Ok(())
}

/// Give the service a way to wake devices that are not connected.
///
/// Without credentials it behaves as it always has: devices sync when they
/// next look. Push only shortens that wait; nothing depends on it.
#[cfg(feature = "push")]
async fn push_waker(
    credentials: Option<String>,
) -> Result<Option<qurb_signal::wake::SharedWaker>> {
    let Some(path) = credentials else { return Ok(None) };
    let account = qurb_signal::fcm::ServiceAccount::from_file(std::path::Path::new(&path))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let project = account.project_id.clone();
    let sender = qurb_signal::fcm::Fcm::new(account);

    // Checked now rather than on the first device that needs waking. A key
    // that does not work is a thing to find out at startup, not months later
    // when somebody's phone quietly stops being prompt.
    sender
        .check()
        .await
        .map_err(|e| anyhow::anyhow!("the push credentials do not work: {e}"))?;
    println!("  waking sleeping devices through Firebase project {project}");

    Ok(Some(std::sync::Arc::new(sender)))
}

#[cfg(not(feature = "push"))]
async fn push_waker(
    credentials: Option<String>,
) -> Result<Option<qurb_signal::wake::SharedWaker>> {
    if credentials.is_some() {
        bail!(
            "this build cannot send push notifications.\n\
             Rebuild with:  cargo build --release --features push"
        );
    }
    Ok(None)
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
        91..=5400 => plural(seconds / 60, "minute"),
        5401..=172_800 => plural(seconds / 3600, "hour"),
        _ => plural(seconds / 86_400, "day"),
    }
}

fn plural(n: i64, unit: &str) -> String {
    match n {
        1 => format!("1 {unit} ago"),
        n => format!("{n} {unit}s ago"),
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
