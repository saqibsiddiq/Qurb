//! qurb in a window.
//!
//! The same [`Daemon`] `qurb run` starts, hosted inside a Tauri shell rather
//! than a terminal — an interface should display the engine, not reimplement
//! it. See [decision 0032] for why it hosts the daemon instead of talking to
//! one, and why there are two mechanisms rather than one for getting data into
//! the window.
//!
//! [decision 0032]: ../../../docs/decisions/0032-the-interface-hosts-the-daemon.md
//!
//! # Threads
//!
//! The daemon owns a tokio runtime on its own threads; the window owns the main
//! thread, because every windowing system requires its event loop there. The
//! same split `qurb-tray` uses, for the same reason.
//!
//! # Two store handles
//!
//! The daemon has one and writes through it. This process opens a second,
//! read-only in practice, for the window's queries. SQLite in WAL mode allows
//! exactly that — a reader does not block the writer and is not blocked by it —
//! which is what lets a folder be browsed while it is being synced.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use qurb_cli::status::{Status, Watcher};
use qurb_cli::{open_with, store_dir, Daemon};
use qurb_storage::Store;
use std::path::PathBuf;
use std::sync::Mutex;

mod commands;

/// Everything a command needs, handed to Tauri as managed state.
pub struct Hosted {
    pub root: PathBuf,
    /// The window's own handle on the index. Behind a mutex because Tauri
    /// serves commands from a pool and `rusqlite::Connection` is not `Sync`;
    /// every query here is short, so contention is not a concern.
    pub store: Mutex<Store>,
    /// The daemon's live account of itself.
    pub status: Watcher,
}

impl Hosted {
    /// The configured storage allowance, freshly read.
    ///
    /// From the file rather than from a cached value, because `qurb config` can
    /// change it while the window is open and a stale number would make the
    /// storage screen quietly wrong.
    pub fn limit(&self) -> u64 {
        qurb_cli::Config::load(&store_dir(&self.root)).map(|c| c.limit).unwrap_or(0)
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "qurb=info,qurb_desktop=info".into()),
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

    // Opened on this thread, before the window exists, so a wrong passphrase or
    // a locked keystore fails with a message on stderr rather than after a
    // window has appeared claiming everything is fine.
    let (master, identity, store, config) = open_with(&root, ask_passphrase)
        .with_context(|| format!("opening {}", root.display()))?;
    let key = store.chunk_key();
    drop(store);

    let (publisher, watcher) =
        qurb_cli::status::channel(Status::starting(root.clone(), identity.fingerprint().short()));

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

    // The window's own handle. Attached to the folder, so that a file the
    // daemon materialised reads as materialised here too -- a reader without
    // the tree would call every synced file "not here".
    let reader = Store::open(&store_dir(&root), key)
        .context("opening the index for the window")?
        .in_tree(&root);

    let hosted = Hosted { root, store: Mutex::new(reader), status: watcher };

    tauri::Builder::default()
        .manage(hosted)
        .invoke_handler(tauri::generate_handler![
            commands::summary,
            commands::storage,
            commands::files,
            commands::search,
            commands::devices,
            commands::activity,
            commands::outgoing,
            commands::fetch,
            commands::set_limit,
        ])
        .run(tauri::generate_context!())
        .context("running the window")
}

/// Which directory to sync, from the argument or the configured default.
///
/// The same resolution the CLI uses, so launching from the applications menu
/// and running `qurb status` in a terminal address the same folder.
fn directory() -> Result<PathBuf> {
    if let Some(given) = std::env::args().nth(1) {
        return Ok(PathBuf::from(given));
    }

    qurb_cli::profiles::current().ok_or_else(|| {
        let default = qurb_cli::profiles::default_root()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|_| "~/Downloads/qurb".into());
        anyhow::anyhow!(
            "no folder is set up yet.\n\
             Run `qurb init` to make one at {default}, or pass a path:\n\
             \n    qurb-desktop /path/to/folder"
        )
    })
}

/// The size of the filesystem the folder is on, in bytes.
///
/// The ceiling for a storage allowance: offering to let somebody reserve more
/// than the disk holds is offering a setting that cannot mean anything. Zero on
/// failure, which the window reads as "no ceiling known" rather than as "no
/// space".
pub fn disk_size(root: &std::path::Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(root.as_os_str().as_bytes()) else { return 0 };

    // SAFETY: `statvfs` writes into the struct and reads a NUL-terminated path,
    // both of which hold here. A failure leaves the struct untouched, which is
    // why the return value is checked before anything is read out of it.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) != 0 {
            return 0;
        }
        (stat.f_blocks as u64).saturating_mul(stat.f_frsize as u64)
    }
}

/// Ask for a passphrase on the terminal this was launched from, if there is
/// one.
///
/// A graphical prompt would be better and needs the window, which does not
/// exist yet — and cannot, because opening the key is what decides whether
/// there is anything to show. Launched from a menu with a passphrase-protected
/// key, this fails with a message telling the person to start it from a
/// terminal, which is honest and rare.
fn ask_passphrase() -> Result<String> {
    use std::io::{IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "this key is protected by a passphrase, and there is no terminal to ask on.\n\
             Start it from a terminal, or switch to the keystore with `qurb protect <dir> keystore`."
        );
    }
    print!("passphrase: ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}
