//! qurb — private cloud storage.
//!
//! The terminal front end. The daemon itself lives in the library beside this,
//! so that something other than a terminal can run the same one — an interface
//! should display the engine rather than reimplement it.

use anyhow::{bail, Context, Result};
use qurb_cli::config::Config;
use qurb_cli::daemon::Daemon;
use qurb_keys::{MasterKey, Purpose, RecoveryPhrase, Vault};
use qurb_peer::{Identity, PairingHost};
use qurb_storage::{ChunkKey, Store};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const USAGE: &str = "\
qurb — private cloud storage

  qurb init [dir]                     set up a device and create a key
                                    (defaults to ~/qurb)
  qurb enrol <dir> \"<24 words>\"       set up a device with an existing key
  qurb pair [dir] [--guest]           show a code and wait for a device to join;
                                        --guest: another person's, keeping its key
  qurb visit [dir] <code>             visit another person's computer as a guest
  qurb join [dir] <code>              join a device showing a code; a folder not
                                        set up yet takes that device's key
  qurb run [dir]                      watch, sync, and keep running
  qurb replica <dir> [--only <path>]  hold content for devices that are asleep
  qurb status [dir]                   what this device holds and trusts
  qurb verify [dir] [--deep]          check the store against itself
  qurb reclaim [dir]                  free space the folder itself already holds
  qurb fetch [dir] <path>             ask for a dropped file's contents back
  qurb send [dir] <file or folder>... to <device> [--again]
                                      send files to one device, privately;
                                        --again: ones sent there before too
  qurb cancel [dir] <name> to <device>
                                      take back a send not yet collected
  qurb free [dir] <path>              free the local copy of a file another
                                        device keeps; `fetch` brings it back
  qurb private [dir] <path>           move a file into this device's Private
                                        Vault; `unprivate` moves it back out
  qurb conflicts [dir] [keep <copy> this|other|both]
                                      files two devices changed at once, and
                                        settling one: nothing is lost either way
  qurb share [dir] [<folder> with <device>,... | everyone]
                                      which devices a folder is shared with
                                        (this = this device)
  qurb keep [dir] <folder> here|remote
                                      keep a folder here, or only list it here
                                        and fetch each file when asked for
  qurb deleted [dir]                  recently deleted files, restorable for 30 days
  qurb restore [dir] <#n or path>     put a recently deleted file back where it was
  qurb forget [dir] <#n or path>      delete a recently deleted file for good, here
  qurb holders [dir] [add|remove <device>]
                                      the devices that keep this one's own files
  qurb remove-device [dir] <device> [--delete-kept] [--yes]
                                      stop trusting a paired device; says what
                                        that does first, and does it with --yes
  qurb activity [dir] [path]          what happened, newest first
  qurb ls [dir] [path]                what this folder holds, and where
  qurb find [dir] <text>              files whose name contains something
  qurb config [dir] [key=value ...]   show or change settings
  qurb version                        this build, its protocol, its index schema
  qurb protect [dir] <how>            change how the key is kept
                                        file | keystore | passphrase

Running the services yourself:

  qurb signal [addr] [options]        the rendezvous service (default :9000)
                                        --tls            present its own certificate
                                        --cert p --key p use a real one instead
                                        --host name      what devices will type
                                        --push <json>    wake sleeping devices
  qurb relay [addr]                   the relay (default :9001)
  qurb netcheck                       what this network will let you do

Settings live in <dir>/.qurb/config and can be edited by hand.
";

fn main() {
    allow_a_closed_pipe();

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
        "pair" => {
            // `--guest` anywhere: a code for another person's device to visit
            // this computer, keeping its own key (decision 0060).
            let guest = args.iter().any(|a| a == "--guest");
            let rest: Vec<String> = args.iter().filter(|a| *a != "--guest").cloned().collect();
            block_on(pair(directory(&rest)?, guest))
        }
        "visit" => {
            // `qurb visit <code>`, or with the folder first: this device, with
            // its own key, visiting another person's computer as a guest.
            let (root, code) = match &args[1..] {
                [code] => (qurb_cli::profiles::current().context("set up a folder first: qurb init")?, code.clone()),
                [dir, code, ..] => (PathBuf::from(dir), code.clone()),
                [] => bail!("give the guest code the other computer is showing"),
            };
            block_on(visit(root, code))
        }
        "join" => {
            // Two words are a folder and a code, whether or not the folder is
            // set up yet: joining is how a new one gets its key (decision
            // 0052). One word is the code, for the folder already in use or,
            // on a machine with none, the usual place for one.
            let (root, code) = match &args[1..] {
                [code] => (
                    qurb_cli::profiles::current().map_or_else(qurb_cli::profiles::default_root, Ok)?,
                    code.clone(),
                ),
                [dir, code, ..] => (PathBuf::from(dir), code.clone()),
                [] => bail!("give the code the other device is showing"),
            };
            block_on(join(root, code))
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
            // `qurb send report.pdf Photos to laptop`, optionally with the qurb
            // folder first. The word `to` ends the list of things to send, and
            // the first of them is the folder only if it has a store in it --
            // counting arguments could not tell `qurb send a.pdf to x` from
            // `qurb send ~/qurb a.pdf to x`, and used to open `a.pdf` as the
            // folder. `--again` anywhere sends what went to that device before
            // without asking (decision 0059).
            let again = args.iter().any(|a| a == "--again");
            let rest: Vec<String> = args[1..].iter().filter(|a| *a != "--again").cloned().collect();
            let rest = &rest[..];
            let at = rest
                .iter()
                .position(|a| a == "to")
                .context("say which device: qurb send <file or folder>... to <device>")?;
            let (before, after) = rest.split_at(at);
            let recipient = after
                .get(1)
                .context("say which device: qurb send <file or folder>... to <device>")?;
            let (root, picked) = match before.first() {
                Some(first) if before.len() > 1 && PathBuf::from(first).join(".qurb").is_dir() => {
                    (PathBuf::from(first), &before[1..])
                }
                _ => (
                    qurb_cli::profiles::current().context(
                        "no folder given, and none is set up yet.\n\
                         Run `qurb init` to make one, or give its path first.",
                    )?,
                    before,
                ),
            };
            if picked.is_empty() {
                bail!("give a file or folder to send");
            }
            let picked: Vec<PathBuf> = picked.iter().map(PathBuf::from).collect();
            send(&root, &picked, recipient, again)
        }
        "conflicts" => {
            let (root, rest) = folder_first(&args)?;
            conflicts(&root, rest)
        }
        "share" => {
            let (root, rest) = folder_first(&args)?;
            share(&root, rest)
        }
        "keep" => {
            let rest = &args[1..];
            let (root, rest) = match rest.first() {
                Some(first) if rest.len() == 3 && PathBuf::from(first).join(".qurb").is_dir() => {
                    (PathBuf::from(first), &rest[1..])
                }
                _ => (qurb_cli::profiles::current().context("no folder is set up yet")?, rest),
            };
            let [folder, how] = rest else { bail!("qurb keep [dir] <folder> here|remote") };
            let (_, _, mut store, _) = open(&root)?;
            match how.as_str() {
                "remote" => {
                    let (freed, bytes, kept) = store.keep_remotely(folder)?;
                    println!("{folder}: freed {freed} files ({}); new files are listed, not downloaded", human(bytes));
                    for path in &kept {
                        println!("  kept {path}: this device has the only copy");
                    }
                }
                "here" => {
                    let asked = store.keep_locally(folder)?;
                    println!("{folder} is kept here; {asked} files asked for, arriving at the next sync");
                }
                _ => bail!("qurb keep [dir] <folder> here|remote"),
            }
            Ok(())
        }
        "deleted" => {
            let (root, _) = split_path(&args)?;
            deleted(&root)
        }
        "restore" => {
            let (root, which) = split_path(&args)?;
            restore(&root, &which.context("say which: qurb restore <#n or path> — see `qurb deleted`")?)
        }
        "forget" => {
            let (root, which) = split_path(&args)?;
            forget(&root, &which.context("say which: qurb forget <#n or path> — see `qurb deleted`")?)
        }
        "free" => {
            let (root, path) = split_path(&args)?;
            free(&root, &path.context("give a file to free: qurb free <path>")?)
        }
        "private" | "unprivate" => {
            let (root, path) = split_path(&args)?;
            let path = path.with_context(|| format!("give a file: qurb {} <path>", args[0]))?;
            move_area(&root, &path, args[0] == "private")
        }
        "holders" => {
            // `qurb holders`, `qurb holders add phone`, or with the folder first.
            let (root, rest) = folder_first(&args)?;
            holders(&root, rest)
        }
        "version" | "--version" | "-V" => {
            println!("{}", qurb_cli::version());
            Ok(())
        }
        "remove-device" => {
            // `qurb remove-device phone`, the folder optionally first, flags
            // anywhere.
            let rest = &args[1..];
            let flag = |name: &str| rest.iter().any(|a| a == name);
            let words: Vec<&String> = rest.iter().filter(|a| !a.starts_with("--")).collect();
            let (root, device) = match words.as_slice() {
                [dir, device] => (PathBuf::from(dir), device.as_str()),
                [device] => {
                    (qurb_cli::profiles::current().context("no folder is set up yet")?, device.as_str())
                }
                _ => bail!("qurb remove-device [dir] <device> [--delete-kept] [--yes]"),
            };
            remove_device(&root, device, flag("--delete-kept"), flag("--yes"))
        }
        "cancel" => {
            // `qurb cancel report.pdf to laptop`: the name as `qurb send`
            // printed it, which is the name the other device would have seen.
            let rest = &args[1..];
            let at = rest
                .iter()
                .position(|a| a == "to")
                .context("say which device: qurb cancel <name> to <device>")?;
            let (before, after) = rest.split_at(at);
            let recipient =
                after.get(1).context("say which device: qurb cancel <name> to <device>")?;
            let (root, name) = match before {
                [dir, name] => (PathBuf::from(dir), name),
                [name] => (qurb_cli::profiles::current().context("no folder is set up yet")?, name),
                _ => bail!("give the name of one send: qurb cancel <name> to <device>"),
            };
            cancel(&root, name, recipient)
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
        // `qurb config`, `qurb config limit=10G`, or with the folder first --
        // which used to be required, and `qurb config` alone panicked.
        "config" => {
            let (root, settings) = folder_first(&args)?;
            configure(&root, settings)
        }
        "protect" => {
            let (root, rest) = folder_first(&args)?;
            protect(&root, rest.first().map(String::as_str))
        }
        "signal" => {
            let after = |flag: &str| {
                args.iter().skip_while(|a| a.as_str() != flag).nth(1).cloned()
            };
            let addr = args.get(1).filter(|a| !a.starts_with("--")).cloned();
            block_on(signal(
                addr,
                after("--push"),
                Tls {
                    on: args.iter().any(|a| a == "--tls"),
                    cert: after("--cert"),
                    key: after("--key"),
                    // The name devices will use, which is what makes the
                    // printed URL something to copy rather than fill in.
                    host: after("--host"),
                },
            ))
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

/// The folder a command acts on, and the arguments after it.
///
/// The first argument is the folder only when it has qurb in it; otherwise it
/// is the command's own first word, and the folder is the one set up most
/// recently. Counting arguments cannot tell `qurb protect keystore` from
/// `qurb protect ~/qurb`, and `config`, `protect` and `join` once tried: each
/// took its first word as the folder, so they worked only with a path.
fn folder_first(args: &[String]) -> Result<(PathBuf, &[String])> {
    let rest = &args[1..];
    match rest.first() {
        Some(first) if PathBuf::from(first).join(".qurb").is_dir() => {
            Ok((PathBuf::from(first), &rest[1..]))
        }
        _ => Ok((
            qurb_cli::profiles::current().context(
                "no folder given, and none is set up yet.\n\
                 Run `qurb init` to make one, or pass a path.",
            )?,
            rest,
        )),
    }
}

/// Where `qurb init` should put a folder when told no path.
fn new_directory(args: &[String]) -> Result<PathBuf> {
    match args.get(1) {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => qurb_cli::profiles::default_root(),
    }
}

/// Let `qurb ls | head` end quietly instead of panicking.
///
/// Rust ignores `SIGPIPE` at startup so that a write to a closed pipe returns
/// an error rather than killing the process — which is right for a library and
/// wrong for a command-line program, because `println!` then panics on that
/// error. Piping any of these listings into `head` or `less` and closing it
/// early produced a backtrace, which reads as a crash.
///
/// Restoring the default gets the behaviour every other command-line tool has:
/// the process ends when the reader goes away, silently.
fn allow_a_closed_pipe() {
    // SAFETY: `signal` with `SIG_DFL` restores the disposition the process
    // started with before Rust changed it. It touches no memory and is the
    // documented way to undo that.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
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
    if qurb_cli::is_set_up(root) {
        bail!("{} already has a key. Use `qurb status` to see it.", root.display());
    }

    // The phrase is made, and not shown: nobody is asked to write 24 words
    // down any more (decision 0052). A device is added with a code, which
    // carries the key; the words remain the key's spelling, for `enrol`.
    let _phrase = qurb_cli::setup::create(root)?;

    println!("Set up {}\n", root.display());
    println!("To add another device, run `qurb pair` here and, on the other one:");
    println!("  qurb join <the code it shows>");
    println!();
    println!("The code carries this device's key, so there is nothing to write down.");
    println!("Your files live on your devices: lose every one that holds them and");
    println!("they are gone, unless a replica keeps a copy (`qurb replica`).");
    Ok(())
}

fn enrol(root: &Path, phrase: &str) -> Result<()> {
    let phrase = RecoveryPhrase::parse(phrase).context("that is not a valid recovery phrase")?;
    qurb_cli::setup::enrol(root, &phrase)?;

    println!("Set up {} with an existing key.\n", root.display());
    println!("This device shares a key with your others, which is what makes them");
    println!("yours. They still have to be introduced: run `qurb pair` on one and");
    println!("`qurb join` here with the code it shows.");
    Ok(())
}

async fn pair(root: PathBuf, guest: bool) -> Result<()> {
    let (master, identity, store, config) = open(&root)?;
    let store = Arc::new(Mutex::new(store));

    let bind = format!("0.0.0.0:{}", config.port).parse()?;
    let host = match guest {
        true => PairingHost::open_for_guest(bind, &identity, now())?,
        false => PairingHost::open(bind, &identity, now())?,
    };

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

    if guest {
        println!("This is a guest code: for another person's device to visit this computer.");
        println!("It keeps its own key, and sees only what is sent to it.\n");
        println!("Or on their computer:\n");
        println!("  qurb visit {code}\n");
    } else {
        println!("Or on another computer:\n");
        println!("  qurb join <dir> {code}\n");
    }
    println!("Or read this out:\n");
    println!("  {}\n", host.invite().for_humans());
    println!("The code carries this device's full identity, which is why it has to");
    println!("travel outside the network — someone able to change what is on your");
    println!("screen has already won.\n");
    println!("It expires in 5 minutes and works once. Waiting...");

    // A device with no key that joins with this code gets this one's
    // (decision 0052), so setting up another computer is `qurb join <code>`
    // -- once the person here approves it, comparing numbers (decision 0053).
    let kind = kind_of(&store);
    let ours = qurb_peer::Ours { name: &config.name, kind: &kind, key: &master };
    match host.wait(Arc::clone(&store), &ours, now(), approve_in_terminal).await {
        Ok(peer) if guest => {
            println!("\n{} ({}) is a guest of this computer now.", peer.name, peer.fingerprint.short());
            println!("Send them files with `qurb send <file> to {}`.", peer.name);
            Ok(())
        }
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

/// Visit another person's computer as a guest, with the guest code it showed
/// (decision 0060): this device keeps its key, shows the number while the
/// person there approves, and from then on can send that computer files and
/// be sent them.
async fn visit(root: PathBuf, code: String) -> Result<()> {
    let invite = qurb_peer::Invite::parse(&code)?;
    if !invite.guest {
        bail!("that code adds one of your own devices; use `qurb join` for that");
    }
    let (_, identity, store, config) = open(&root)?;
    let store = Arc::new(Mutex::new(store));
    let kind = kind_of(&store);
    println!("Visiting as {}: check that the other computer shows {}.", config.name, invite.number_for(&identity.fingerprint()));
    match qurb_peer::visit(&invite, &identity, store, &config.name, &kind, now()).await {
        Ok(host) => {
            println!("A guest of {} ({}) now.", host.name, host.fingerprint.short());
            println!("Send it files with `qurb send <file> to {}`.", host.name);
            Ok(())
        }
        Err(qurb_peer::Error::Declined) => bail!("the other computer said no"),
        Err(e) => Err(e.into()),
    }
}

/// Asked in the terminal: who wants to pair, the number it should be showing,
/// and yes or no (decision 0053). Anything but yes is no, including a closed
/// input, so an unattended `qurb pair` lets nobody in.
async fn approve_in_terminal(asking: qurb_peer::Asking) -> bool {
    let what = if asking.guest {
        "visit this computer as a guest, with its own key"
    } else if asking.wants_key {
        "join, and take this device's key"
    } else {
        "pair"
    };
    let kind = asking.kind.as_deref().map(|k| format!(", a {k}")).unwrap_or_default();
    println!("\n{} ({}{kind}) wants to {what}.", asking.name, asking.fingerprint.short());
    println!("It should be showing {}. If it is not, someone else has this code.", asking.number);
    print!("Approve? [y/N] ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok();
        matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
    })
    .await
    .unwrap_or(false)
}

/// What this folder is to the devices it pairs with: what its daemon last
/// said it was, or a computer.
fn kind_of(store: &Arc<Mutex<qurb_storage::Store>>) -> String {
    store
        .lock()
        .ok()
        .and_then(|s| s.db().local_kind().ok().flatten())
        .unwrap_or_else(|| "computer".to_string())
}

async fn join(root: PathBuf, code: String) -> Result<()> {
    if !qurb_cli::is_set_up(&root) {
        let number = qurb_cli::setup::number_for(&root, &code)?;
        println!("Joining your other device. It will ask you to approve this one:");
        println!("check that it shows {number}.");
        let peer = qurb_cli::setup::join(&root, &code).await?;
        println!("Set up {} as another of your devices.", root.display());
        println!("Paired with {} ({}). Run `qurb run` to start syncing.", peer.name, peer.fingerprint.short());
        return Ok(());
    }
    let (master, identity, store, config) = open(&root)?;
    let store = Arc::new(Mutex::new(store));

    let invite = qurb_peer::Invite::parse(&code).context("that is not a valid pairing code")?;
    println!("Joining {} ...", invite.address);
    println!("Approve it there: it should show {}.", invite.number_for(&identity.fingerprint()));

    let kind = kind_of(&store);
    let ours = qurb_peer::Ours { name: &config.name, kind: &kind, key: &master };
    let peer = qurb_peer::accept(&invite, &identity, store, &ours, now()).await?;
    println!("Paired with {} ({})", peer.name, peer.fingerprint.short());
    Ok(())
}

async fn start(root: PathBuf) -> Result<()> {
    let (master, identity, store, config) = open(&root)?;
    drop(store);

    println!("qurb: syncing {}", root.display());
    println!("  identity  {}", identity.fingerprint().short());
    println!("  signal    {}", config.signal);
    match &config.relay {
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
    match config.downloads.resolve(root) {
        Ok(Some(dir)) => println!("  sent here  saved to {}", dir.display()),
        Ok(None) => println!("  sent here  kept in the folder"),
        Err(_) => println!("  sent here  refused — see `qurb config`"),
    }
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
fn send(root: &Path, picked: &[PathBuf], recipient: &str, again: bool) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;

    let peer = match qurb_cli::View::new(&store, 0).device_named(recipient)? {
        qurb_cli::Recipient::One(device) => device,
        qurb_cli::Recipient::Unknown { known } if known.is_empty() => {
            bail!("this device has not been paired with anything yet")
        }
        qurb_cli::Recipient::Unknown { known } => {
            let names: Vec<String> =
                known.iter().map(|d| format!("{} ({})", d.name, d.fingerprint)).collect();
            bail!("no paired device called {recipient} — known: {}", names.join(", "));
        }
        qurb_cli::Recipient::Several(devices) => {
            let ids: Vec<String> = devices.iter().map(|d| d.fingerprint.clone()).collect();
            bail!("more than one device is called {recipient} — use one of: {}", ids.join(", "));
        }
    };

    let mut plan = qurb_cli::send::plan(picked);
    for (path, why) in &plan.skipped {
        eprintln!("  not sending {}: {why}", path.display());
    }
    if plan.files.is_empty() {
        bail!("nothing to send");
    }

    // Sent there before: said, and sent again only when that is what the
    // person wants (decision 0059). A send is never dropped quietly.
    let sources: Vec<String> =
        plan.files.iter().map(|(_, source)| source.to_string_lossy().into_owned()).collect();
    let before = store.sent_before(&sources, &peer.id)?;
    if !before.is_empty() {
        println!("sent to {} before:", peer.name);
        for earlier in &before {
            println!("  {} — as {}, {}", earlier.source, earlier.sent_as, ago(earlier.at));
        }
        let yes = again || {
            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() {
                print!("Send {} again? [y/N] ", if before.len() == 1 { "it" } else { "them" });
                let _ = std::io::Write::flush(&mut std::io::stdout());
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok();
                matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
            } else {
                false
            }
        };
        if !yes {
            let skip: std::collections::HashSet<&str> = before.iter().map(|b| b.source.as_str()).collect();
            plan.files.retain(|(_, source)| !skip.contains(source.to_string_lossy().as_ref()));
            println!("  left out; `--again` sends them anyway");
            if plan.files.is_empty() {
                return Ok(());
            }
        }
    }

    println!("sending to {} ({})", peer.name, peer.fingerprint);
    let mut sent = 0usize;
    for (name, source) in &plan.files {
        // One file failing is reported and the rest still go, for the same
        // reason a folder is not refused for one unreadable file in it.
        match store.send_to_vault(name, source, &peer.id) {
            Ok(_) => {
                println!("  {name}");
                sent += 1;
            }
            Err(e) => eprintln!("  not sending {name}: {e}"),
        }
    }
    let (it, they) = if sent == 1 { ("it", "it is") } else { ("them", "they are") };
    println!("{sent} file{} waiting for {} to collect", if sent == 1 { "" } else { "s" }, peer.name);
    // No copy is kept (decision 0060).
    println!("  read from where {they} when {} collects {it}, even after a restart;", peer.name);
    println!("  change or delete one before then and that one is not sent");
    Ok(())
}

/// Free a file's local copy: the file stays known, and `qurb fetch` brings it
/// back. Refused when no other device is known to hold these bytes -- the same
/// rule the storage cap follows, and the difference between freeing space and
/// deleting the only copy.
/// Move a file into this device's Private Vault, or out of it to every device
/// (decision 0057).
fn move_area(root: &Path, logical: &str, private: bool) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;
    let logical = logical.trim_start_matches("./");
    let keepers = store.db().holders()?.len();
    match store.move_area(logical, private) {
        Ok(false) if private => println!("{logical} is already in Private Vault"),
        Ok(false) => println!("{logical} is already shared"),
        Ok(true) if private => {
            println!("moved {logical} into Private Vault");
            println!("  your other devices remove their copies at their next sync");
            if keepers == 0 {
                println!("  no device keeps this one's Private Vault: this is now the only copy");
            }
        }
        Ok(true) => println!("moved {logical} out of Private Vault; it goes to all your devices"),
        Err(qurb_storage::Error::NotHere { .. }) => {
            bail!("{logical} is not on this device: `qurb fetch {logical}` brings it back first")
        }
        Err(qurb_storage::Error::NotFound { .. }) => bail!("{logical} is not a file here"),
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn free(root: &Path, logical: &str) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;
    let logical = logical.trim_start_matches("./");
    match store.free_local(logical) {
        Ok(freed) => {
            println!("freed {logical} ({})", human(freed));
            println!("  it is still yours; `qurb fetch {logical}` brings it back");
            Ok(())
        }
        Err(qurb_storage::Error::CannotEvict { .. }) => bail!(
            "not freeing {logical}: no other device is known to have it, so this is the \
             only copy. Add a holder with `qurb holders add <device>`, or send it somewhere."
        ),
        Err(qurb_storage::Error::NotFound { .. }) => bail!("{logical} is not a file here"),
        Err(e) => Err(e.into()),
    }
}

/// Files two devices changed without either seeing the other (brief §24).
fn conflicts(root: &Path, rest: &[String]) -> Result<()> {
    let (_, _, mut store, config) = open(root)?;
    let mut names = store.db().device_names()?;
    names.insert(store.device_id()?, config.name.clone());
    let who = |id: Option<qurb_sync::DeviceId>| -> String {
        id.map(|id| names.get(&id).cloned().unwrap_or_else(|| id.short()))
            .unwrap_or_else(|| "another device".into())
    };
    match rest {
        [] => {
            let found = store.conflicts()?;
            if found.is_empty() {
                println!("no conflicts");
                return Ok(());
            }
            for c in &found {
                println!("{}: two devices changed it. Nothing was lost.", c.original_path);
                match &c.original {
                    Some(v) => println!(
                        "  this:  {}  {}, by {} {}",
                        v.path,
                        human(v.size),
                        who(v.modified_by),
                        ago(v.updated_at)
                    ),
                    None => println!("  this:  (since deleted or renamed)"),
                }
                println!(
                    "  other: {}  {}, by {} {}{}",
                    c.copy.path,
                    human(c.copy.size),
                    who(c.copy.modified_by),
                    ago(c.copy.updated_at),
                    if c.copy.here { "" } else { "  (not on this device)" }
                );
            }
            println!("\n`qurb conflicts keep <other> this|other|both` settles one, on every device.");
            println!("The version not kept goes to Recently deleted.");
            Ok(())
        }
        [verb, copy, choice] if verb == "keep" => {
            let keep = match choice.as_str() {
                "this" => qurb_storage::Keep::Original,
                "other" => qurb_storage::Keep::Copy,
                "both" => qurb_storage::Keep::Both,
                _ => bail!("keep this, other, or both"),
            };
            let copy = copy.trim_start_matches("./");
            let maker = store.db().folder_row(copy)?.and_then(|(row, _)| row.modified_by);
            let kept = store.settle_conflict(copy, keep, &who(maker))?;
            println!("kept {kept}");
            if keep != qurb_storage::Keep::Both {
                println!("  the other version is in Recently deleted — `qurb deleted`");
            }
            Ok(())
        }
        _ => bail!("qurb conflicts [dir] [keep <copy> this|other|both]"),
    }
}

/// Which devices each folder is shared with, or change one (decision 0044).
fn share(root: &Path, rest: &[String]) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;
    let names = store.db().device_names()?;
    let me = store.device_id()?;
    let name = |id: &qurb_sync::DeviceId| -> String {
        match *id == me {
            true => "this device".into(),
            false => names.get(id).cloned().unwrap_or_else(|| id.short()),
        }
    };
    match rest {
        [] => {
            let folders = store.folder_sharing()?;
            if folders.is_empty() {
                println!("no folders yet");
            }
            for (folder, members) in folders {
                match members {
                    None => println!("  {folder}  — every device"),
                    Some(members) => println!(
                        "  {folder}  — {}",
                        members.iter().map(name).collect::<Vec<_>>().join(", ")
                    ),
                }
            }
            println!("\n`qurb share <folder> with this,phone` or `... everyone` changes one, on every device.");
            Ok(())
        }
        [folder, with, everyone] if with == "with" && everyone == "everyone" => {
            store.clear_sharing(folder)?;
            println!("{folder} is shared with every device");
            Ok(())
        }
        [folder, with, list] if with == "with" => {
            let view = qurb_cli::View::new(&store, 0);
            let mut members = std::collections::BTreeSet::new();
            for text in list.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                if text == "this" {
                    members.insert(me);
                    continue;
                }
                match view.device_named(text)? {
                    qurb_cli::Recipient::One(device) => members.insert(device.id),
                    _ => bail!("no single paired device called {text} — see `qurb status`"),
                };
            }
            store.set_sharing(folder, &members)?;
            println!(
                "{folder} is shared with {}",
                members.iter().map(name).collect::<Vec<_>>().join(", ")
            );
            println!("  a device left out keeps what it has; nothing new reaches it");
            Ok(())
        }
        _ => bail!("qurb share [dir] [<folder> with <device>,... | everyone]"),
    }
}

/// What is in Recently deleted here (decision 0042).
fn deleted(root: &Path) -> Result<()> {
    let (_, _, store, _) = open(root)?;
    let entries = store.recently_deleted()?;
    if entries.is_empty() {
        println!("nothing recently deleted here");
        return Ok(());
    }
    let names = store.db().device_names()?;
    let me = store.device_id()?;
    for entry in &entries {
        let by = match entry.deleted_by {
            Some(id) if id == me => "here".to_string(),
            Some(id) => format!("on {}", names.get(&id).cloned().unwrap_or_else(|| id.short())),
            None => String::new(),
        };
        let why = entry.why.as_deref().map(|w| format!("  ({w})")).unwrap_or_default();
        println!(
            "  #{:<5} {:>12}  {:>9}  {}  deleted {by}{why}",
            entry.id,
            ago(entry.deleted_at),
            human(entry.size),
            entry.path
        );
    }
    println!("\n`qurb restore #n` puts one back where it was. Kept for 30 days.");
    Ok(())
}

/// The Recently deleted entry `which` names: `#n`, or a path, meaning its most
/// recent deletion.
fn trashed<'a>(entries: &'a [qurb_storage::db::Trashed], which: &str) -> Result<&'a qurb_storage::db::Trashed> {
    match which.strip_prefix('#').and_then(|n| n.parse::<i64>().ok()) {
        Some(id) => entries.iter().find(|e| e.id == id),
        None => entries.iter().find(|e| e.path == which.trim_start_matches("./")),
    }
    .with_context(|| format!("{which} is not in Recently deleted here — see `qurb deleted`"))
}

fn restore(root: &Path, which: &str) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;
    let entries = store.recently_deleted()?;
    let entry = trashed(&entries, which)?;
    let at = store.restore_from_trash(entry.id)?;
    match at == entry.path {
        true => println!("restored {at}"),
        false => println!("restored {} as {at}: something is at its old path now", entry.path),
    }
    println!("  it returns on your other devices at their next sync");
    Ok(())
}

/// Delete one Recently deleted file for good, on this device only: each
/// device's list is its own (decision 0042), as *Delete for good* is in the
/// window and on the phone. The command line had restore and not this, so
/// what a test put there could be cleared only from a window.
fn forget(root: &Path, which: &str) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;
    let entries = store.recently_deleted()?;
    let entry = trashed(&entries, which)?;
    let (id, path, size) = (entry.id, entry.path.clone(), entry.size);
    store.forget_deleted(id)?;
    println!("deleted {path} for good ({}), here", human(size));
    println!("  other devices keep their own Recently deleted");
    Ok(())
}

/// The devices that keep this device's own files for it (decision 0036).
fn holders(root: &Path, rest: &[String]) -> Result<()> {
    let (_, _, store, _) = open(root)?;
    let view = qurb_cli::View::new(&store, 0);
    let named = |text: &str| -> Result<qurb_cli::Device> {
        match view.device_named(text)? {
            qurb_cli::Recipient::One(device) => Ok(device),
            _ => bail!("no single paired device called {text} — see `qurb status`"),
        }
    };
    match rest {
        [] => {
            let devices = view.devices()?;
            let holders = store.db().holders()?;
            if holders.is_empty() {
                println!("no device keeps this one's files");
            }
            for id in holders {
                let name = devices.iter().find(|d| d.id == id).map(|d| d.name.clone());
                println!("{}  {}", id.short(), name.unwrap_or_else(|| "(no longer paired)".into()));
            }
            Ok(())
        }
        [verb, device] if verb == "add" => {
            let device = named(device)?;
            store.db().add_holder(&device.id)?;
            println!("{} now keeps this device's own files for it", device.name);
            println!("  it never shows them; they come back here with `qurb fetch`");
            Ok(())
        }
        [verb, device] if verb == "remove" => {
            let device = named(device)?;
            store.db().remove_holder(&device.id)?;
            println!("{} is no longer shown this device's files", device.name);
            println!("  what it already keeps, it keeps: nothing here reaches into it");
            Ok(())
        }
        _ => bail!("qurb holders [dir] [add|remove <device>]"),
    }
}

/// Say what removing a device does, then -- with `--yes` -- do it.
///
/// Said first because two of its consequences cannot be undone from here: a
/// file freed on the strength of a copy that device keeps has nowhere else to
/// come back from, and what this device keeps for it may be its only backup.
fn remove_device(root: &Path, text: &str, delete_kept: bool, yes: bool) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;
    let device = match qurb_cli::View::new(&store, 0).device_named(text)? {
        qurb_cli::Recipient::One(device) => device,
        _ => bail!("no single paired device called {text} — see `qurb status`"),
    };
    let plan = store.removal_plan(&device.id)?;

    println!("Removing {} stops this device trusting it: it can no longer connect", device.name);
    println!("here or sync with this device. It keeps its key and everything already");
    println!("on it — removing it deletes nothing there.");
    if !plan.waiting.is_empty() {
        println!("
  {} waiting for it to collect will be cancelled.", files(plan.waiting.len()));
    }
    if !plan.kept_for_it.is_empty() {
        let bytes: u64 = plan.kept_for_it.iter().map(|(_, size)| size).sum();
        let what = files(plan.kept_for_it.len());
        match delete_kept {
            true => println!("
  {what} this device keeps for it ({}) will be deleted.", human(bytes)),
            false => println!(
                "
  {what} this device keeps for it ({}) stay; --delete-kept deletes them.",
                human(bytes)
            ),
        }
    }
    if plan.holds_ours {
        println!("
  It keeps this device's own files; after this, it no longer will.");
    }
    if !plan.only_there.is_empty() {
        println!(
            "
  {} freed here are kept only on {} and could not be fetched back:",
            files(plan.only_there.len()),
            device.name
        );
        for path in plan.only_there.iter().take(10) {
            println!("    {path}");
        }
        println!("  `qurb fetch` them first to keep them.");
    }
    println!("
Only here: your other devices go on trusting it until it is removed there too.");

    if !yes {
        println!("
Run again with --yes to remove it.");
        return Ok(());
    }
    store.remove_device(&device.id, &device.name, delete_kept)?;
    println!("
{} removed.", device.name);
    Ok(())
}

fn files(n: usize) -> String {
    match n {
        1 => "1 file".into(),
        n => format!("{n} files"),
    }
}

fn cancel(root: &Path, name: &str, recipient: &str) -> Result<()> {
    let (_, _, mut store, _) = open(root)?;
    let peer = match qurb_cli::View::new(&store, 0).device_named(recipient)? {
        qurb_cli::Recipient::One(device) => device,
        _ => bail!("no single paired device called {recipient} — see `qurb status`"),
    };
    store.cancel_send(name, &peer.id).map_err(|e| match e {
        qurb_storage::Error::NotFound { .. } => {
            anyhow::anyhow!("nothing called {name} is waiting for {}", peer.name)
        }
        other => other.into(),
    })?;
    println!("{name} will not be sent to {}", peer.name);
    Ok(())
}

/// Split `<command> [dir] [argument]` into the two, without a flag to say which.
///
/// A single argument is ambiguous — `qurb ls photos` could mean a directory to
/// open or a folder inside one — and asking people to remember an order they
/// never think about is worse than looking. A directory is a directory on disk
/// that has a store in it; anything else is the argument.
///
/// The folder by the same rule as [`folder_first`]. This once read a lone
/// argument that was not a folder as the folder anyway, so `qurb find
/// holiday`, `qurb ls docs` and `qurb restore #1` each said the word was "not
/// set up yet" and worked only with a path in front (found 2026-10-05).
fn split_path(args: &[String]) -> Result<(PathBuf, Option<String>)> {
    let (root, rest) = folder_first(args)?;
    Ok((root, rest.first().cloned()))
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
        // Decision 0055: no device this one can ask has it.
        qurb_cli::Availability::Nowhere => "nowhere",
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
        println!("relay  = {}", config.relay.clone().unwrap_or_default());
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
        println!("own-files = {}", if config.own_files_private { "private" } else { "shared" });
        println!(
            "downloads = {}",
            match config.downloads.resolve(root) {
                Ok(Some(dir)) => dir.display().to_string(),
                Ok(None) => "off — kept in the folder".to_string(),
                Err(e) => format!("refused: {e}"),
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
                    qurb_peer::relay_address_ok(value).map_err(|e| anyhow::anyhow!(e))?;
                    Some(value.trim().to_string())
                }
            }
            "name" => config.name = value.trim().to_string(),
            "port" => config.port = value.trim().parse().context("port should be a number")?,
            "limit" => config.limit = qurb_cli::config::parse_size(value)?,
            "own-files" => {
                config.own_files_private = match value.trim() {
                    "private" => true,
                    "shared" => false,
                    other => bail!("own-files is `shared` or `private`, not `{other}`"),
                }
            }
            "downloads" => {
                let downloads = qurb_cli::config::Downloads::parse(value);
                // Refused here, where the person can fix it, rather than at the
                // next start, where the daemon would refuse to run.
                downloads.resolve(root)?;
                config.downloads = downloads;
            }
            "notifications" => {
                config.notifications = match value.trim() {
                    "on" => true,
                    "off" => false,
                    other => bail!("notifications is `on` or `off`, not `{other}`"),
                }
            }
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
/// How the rendezvous service should present itself.
struct Tls {
    /// Terminate TLS here. With no certificate given, one is made and kept.
    on: bool,
    cert: Option<String>,
    key: Option<String>,
    /// What devices will type. Only affects the URL printed at startup.
    host: Option<String>,
}

async fn signal(
    addr: Option<String>,
    push_credentials: Option<String>,
    tls: Tls,
) -> Result<()> {
    let addr: std::net::SocketAddr =
        addr.unwrap_or_else(|| "0.0.0.0:9000".into()).parse().context("bad address")?;
    // Validated before the port is taken, so a mistyped credentials path says
    // so rather than surfacing as whatever the socket complains about.
    let waker = push_waker(push_credentials).await?;

    let certificate = match (&tls.cert, &tls.key, tls.on) {
        (Some(cert), Some(key), _) => Some(
            qurb_signal::Certificate::load(Path::new(cert), Path::new(key))
                .context("loading the certificate")?,
        ),
        (Some(_), None, _) | (None, Some(_), _) => {
            bail!("--cert and --key go together")
        }
        // Made once and kept, because the fingerprint is what every device has
        // been told to expect: a service that generated a new certificate on
        // each restart would lock out every device it had.
        (None, None, true) => {
            let dir = qurb_signal::tls::default_state_dir();
            let names = vec![
                tls.host.clone().unwrap_or_else(|| "qurb-rendezvous".to_string()),
                "qurb-rendezvous".to_string(),
            ];
            Some(
                qurb_signal::Certificate::kept_in(&dir, names)
                    .with_context(|| format!("preparing a certificate in {}", dir.display()))?,
            )
        }
        (None, None, false) => None,
    };

    let server = qurb_signal::SignalServer::bind(addr).await?;
    let server = match waker {
        // With push, the tokens are kept across restarts: without them a
        // restarted service could wake nobody until each phone's own next
        // scheduled pass. Beside the certificate, in the service's state.
        Some(waker) => {
            let dir = qurb_signal::tls::default_state_dir();
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("preparing {}", dir.display()))?;
            server.waking_with(waker).keeping_wake_tokens_in(dir.join("wake-tokens.json"))
        }
        None => server,
    };

    let port = server.local_addr()?.port();
    println!("rendezvous service on {}", server.local_addr()?);
    println!();

    let server = match certificate {
        Some(certificate) => {
            let host = tls.host.clone().unwrap_or_else(|| "<this machine>".to_string());
            println!("Devices reach it as:");
            println!();
            println!("  {}", certificate.url_for(&host, port));
            println!();
            println!("Everything after the # is this service's certificate fingerprint.");
            println!("A device checks it and accepts nothing else, which is why this works");
            println!("on a bare IP address with no domain name and no certificate authority.");
            println!();
            println!("Copy the whole line:");
            println!();
            println!("  qurb config <dir> signal=<that line>");
            println!();
            println!("and on the phone, ⋮ → Rendezvous service.");
            server.behind(certificate)?
        }
        None => {
            println!("Devices reach it as  ws://<this machine>:{port}");
            println!();
            println!("Unencrypted, so devices will refuse it from anywhere but the local");
            println!("network. The identifiers they announce under are bearer secrets:");
            println!("anyone who sees one can list that group's addresses.");
            println!();
            println!("For a host facing the internet, either put a reverse proxy in front");
            println!("of it, or add --tls and let this service present its own certificate.");
            server
        }
    };

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
