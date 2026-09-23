//! qurb in a window.
//!
//! The same [`Daemon`](qurb_cli::Daemon) `qurb run` starts, hosted inside a
//! Tauri shell rather than a terminal — an interface should display the engine,
//! not reimplement it. See [decision 0032] for why it hosts the daemon instead
//! of talking to one, and why there are two mechanisms rather than one for
//! getting data into the window.
//!
//! [decision 0032]: ../../../docs/decisions/0032-the-interface-hosts-the-daemon.md
//!
//! The window opens whether or not there is a device yet. Setting one up is the
//! job of a screen, so it cannot be a precondition of the screen existing; see
//! [`session`] for the two situations and the one transition between them.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

mod commands;
mod session;

pub use session::Hosted;

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
    let root = directory();
    let hosted = Arc::new(Hosted::new(root.clone()));

    // A device that already exists starts syncing immediately; the window is
    // showing something that is already true, not waiting to be told to begin.
    //
    // A failure here is reported on the screen rather than on the way up. The
    // commonest cause is a passphrase-protected key with no terminal to ask on,
    // and exiting with a message nobody sees is the worst of the options.
    if qurb_cli::is_set_up(&root) {
        if let Err(e) = hosted.start(ask_passphrase) {
            tracing::error!(error = %e, "could not start");
        }
    }

    tauri::Builder::default()
        .manage(Arc::clone(&hosted))
        .invoke_handler(tauri::generate_handler![
            commands::situation,
            commands::start_pairing,
            commands::pairing_state,
            commands::stop_pairing,
            commands::join_device,
            commands::inspect_folder,
            commands::create_device,
            commands::shown_phrase,
            commands::confirm_phrase,
            commands::enrol_device,
            commands::reveal_phrase,
            commands::settings,
            commands::save_settings,
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
        .map_err(|e| anyhow::anyhow!("running the window: {e}"))
}

/// Which folder to open: the argument, the one last used, or a suggestion.
///
/// Unlike the terminal, this never fails. A folder that does not exist yet is
/// exactly what the setting-up screen is for, and refusing to open at all would
/// mean the only way to make one is a command line.
fn directory() -> PathBuf {
    if let Some(given) = std::env::args().nth(1) {
        return PathBuf::from(given);
    }

    // The same resolution the CLI uses, so launching from the applications menu
    // and running `qurb status` in a terminal address the same folder.
    qurb_cli::profiles::current()
        .or_else(|| qurb_cli::profiles::default_root().ok())
        .unwrap_or_else(|| PathBuf::from("qurb"))
}

/// The size of the filesystem the folder is on, in bytes.
///
/// The ceiling for a storage allowance: offering to let somebody reserve more
/// than the disk holds is offering a setting that cannot mean anything. Zero on
/// failure, which the window reads as "no ceiling known" rather than as "no
/// space".
pub fn disk_size(root: &std::path::Path) -> u64 {
    qurb_cli::setup::inspect(root).disk
}

/// Ask for a passphrase on the terminal this was launched from, if there is
/// one.
///
/// A graphical prompt would be better and needs the window, which does not
/// exist yet — and cannot, because opening the key is what decides whether
/// there is anything to show. Launched from a menu with a passphrase-protected
/// key, this fails with a message the window then displays, which is honest and
/// rare.
fn ask_passphrase() -> Result<String> {
    use std::io::{IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "this key is protected by a passphrase, and there is no terminal to ask on. \
             Start it from a terminal, or switch to the keystore with \
             `qurb protect <dir> keystore`."
        );
    }
    print!("passphrase: ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}
