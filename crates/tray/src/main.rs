//! qurb in the corner of the screen.
//!
//! The daemon with somewhere to show itself. It runs exactly the same
//! [`Daemon`] the `qurb run` command does — an interface should display the
//! engine, not reimplement it — and subscribes to the status it publishes.
//!
//! # One process, not two
//!
//! This *runs* the daemon rather than talking to one. The alternative is a
//! socket and a protocol between them, which buys the ability to attach to an
//! already-running daemon and costs a second surface to design, version and
//! secure. Two daemons on one store would collide on the SQLite lock anyway, so
//! the choice is really "which one process runs" — and `qurb run` remains for
//! servers and replicas that have no screen.
//!
//! # When there is no tray
//!
//! GNOME removed the system tray and does not ship a StatusNotifier host, so on
//! a stock GNOME desktop a tray icon is not merely ugly, it is *invisible*.
//! Failing silently there would be the worst outcome: a background program that
//! is running fine and cannot be seen or quit.
//!
//! So the tray is attempted, and when it cannot be shown the program says so on
//! stderr and keeps running with the daemon in the foreground, printing status
//! changes as they happen. Degrading to a working terminal program beats
//! pretending.

use anyhow::{Context, Result};
use qurb_cli::status::Status;
use qurb_cli::{open_with, store_dir, Daemon};
use std::path::{Path, PathBuf};

mod host;
mod icon;
mod ui;
#[cfg(target_os = "linux")]
mod window;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "qurb=info,qurb_tray=info".into()),
        )
        .init();

    if let Err(error) = run() {
        eprintln!("error: {error}");
        for cause in error.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let root = directory()?;

    if !qurb_cli::is_set_up(&root) {
        anyhow::bail!(
            "{} is not set up yet — run `qurb init {}` first",
            root.display(),
            root.display()
        );
    }

    // Opened on this thread, before anything else starts, so a wrong passphrase
    // or a locked keystore fails immediately with a message rather than after a
    // tray icon has already appeared claiming everything is fine.
    let (master, identity, store, config) = open_with(&root, ask_passphrase)
        .with_context(|| format!("opening {}", root.display()))?;
    drop(store);

    let (publisher, watcher) = qurb_cli::status::channel(Status::starting(
        root.clone(),
        identity.fingerprint().short(),
    ));

    // The daemon owns a tokio runtime on its own threads; the interface owns
    // the main thread, because every platform requires its event loop there.
    let daemon_root = root.clone();
    let daemon_store_dir = store_dir(&root);
    std::thread::Builder::new()
        .name("qurb-daemon".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(e) => {
                    tracing::error!(error = %e, "could not start the runtime");
                    return;
                }
            };
            let daemon = Daemon::new(&daemon_root, &daemon_store_dir, master, identity, config)
                .reporting_to(publisher);
            if let Err(e) = runtime.block_on(daemon.run()) {
                tracing::error!(error = %e, "the daemon stopped");
            }
        })
        .context("starting the daemon thread")?;

    ui::show(root, watcher)
}

/// Which directory to sync, from the argument or the configured default.
fn directory() -> Result<PathBuf> {
    if let Some(given) = std::env::args().nth(1) {
        return Ok(PathBuf::from(given));
    }

    // The same default the CLI uses, so running one and then the other does
    // not silently address two different stores.
    let home = std::env::var("HOME").context("no HOME set, and no directory given")?;
    let default = Path::new(&home).join("qurb");
    if qurb_cli::is_set_up(&default) {
        return Ok(default);
    }

    anyhow::bail!(
        "no directory given, and {} is not set up.\n\
         Pass one: qurb-tray <dir>",
        default.display()
    )
}

/// Ask for a passphrase.
///
/// Only called for a vault that is passphrase-protected. A graphical prompt
/// belongs here eventually; until there is one, this is honest about needing a
/// terminal rather than failing with something cryptic.
fn ask_passphrase() -> Result<String> {
    anyhow::bail!(
        "this store is protected by a passphrase, which qurb-tray cannot yet ask for.\n\
         Run `qurb run <dir>` in a terminal, or `qurb protect <dir> keystore` to \
         let the system unlock it."
    )
}
