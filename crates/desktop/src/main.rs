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

mod autostart;
mod commands;
mod instance;
mod notify;
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
    // `--hidden` is how it starts at login: syncing, with no window until
    // somebody opens one from the applications menu.
    let hidden = std::env::args().any(|a| a == "--hidden");

    // Already running: show that one's window, and leave.
    let listener = match instance::claim(&instance::socket_path()) {
        Ok(instance::Instance::AskedToShow) => return Ok(()),
        Ok(instance::Instance::First(listener)) => Some(listener),
        // No runtime directory to listen in, or it refused: run anyway,
        // without being findable. Better than not running.
        Err(e) => {
            tracing::warn!(error = %e, "cannot listen for other launches");
            None
        }
    };
    // Files opened from a guest's folder by a run that ended without locking
    // it -- a crash, a power cut -- go now, as locking would have done
    // (decision 0060). A running instance would have kept them: this is the
    // first.
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        let _ = std::fs::remove_dir_all(std::path::PathBuf::from(runtime).join("qurb-open"));
    }
    let root = directory();
    let hosted = Arc::new(Hosted::new(root.clone()));
    // A key protected by a passphrase waits for the window to ask for it
    // (decision 0046) -- at login too, which is why the window then shows:
    // nothing syncs until it is typed.
    let locked = qurb_cli::is_set_up(&root)
        && commands::key_protection(&root) == Some(qurb_keys::Protection::Passphrase);
    // A device that is not set up yet has nothing to sync, and a window hidden
    // at login would leave setting it up to nobody.
    let show_now = !hidden || !qurb_cli::is_set_up(&root) || locked;

    // A device that already exists starts syncing immediately; the window is
    // showing something that is already true, not waiting to be told to begin.
    //
    // A failure here is reported on the screen rather than on the way up:
    // exiting with a message nobody sees is the worst of the options.
    if qurb_cli::is_set_up(&root) && !locked {
        if let Err(e) = hosted.start(|| anyhow::bail!("this key needs a passphrase")) {
            tracing::error!(error = %e, "could not start");
        }
    }

    // Watching for the few things worth interrupting somebody about. Started
    // before the window, because a notification is most useful when nobody is
    // looking at one.
    notify::watch(Arc::clone(&hosted));

    tauri::Builder::default()
        .setup(move |app| {
            use tauri::Manager;
            let window = app.get_webview_window("main").expect("the window in tauri.conf.json");
            if show_now {
                window.show()?;
                window.set_focus()?;
            }
            if let Some(listener) = listener {
                let handle = app.handle().clone();
                instance::listen(listener, move || {
                    if let Some(window) = handle.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                });
            }
            Ok(())
        })
        // Closing the window hides it; qurb keeps syncing. Quitting is a
        // button in Settings, since on GNOME there is no tray to quit from.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                notify::still_running();
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::clone(&hosted))
        .invoke_handler(tauri::generate_handler![
            commands::situation,
            commands::send_files,
            commands::sent_before,
            commands::guest_folder,
            commands::ask_guest_folder,
            commands::open_guest_file,
            commands::lock_guest_folder,
            commands::cancel_send,
            commands::removal_plan,
            commands::unlock,
            commands::security,
            commands::protect_key,
            commands::sharing,
            commands::set_sharing,
            commands::keep_remotely,
            commands::keep_locally,
            commands::conflicts,
            commands::settle_conflict,
            commands::recently_deleted,
            commands::restore_deleted,
            commands::forget_deleted,
            commands::remove_device,
            commands::open_downloads,
            commands::show_received,
            commands::start_pairing,
            commands::pairing_state,
            commands::stop_pairing,
            commands::join_device,
            commands::inspect_folder,
            commands::create_device,
            commands::read_allowance,
            commands::join_new_device,
            commands::answer_pairing,
            commands::pairing_number,
            commands::setup_pairing_number,
            commands::enrol_device,
            commands::reveal_phrase,
            commands::settings,
            commands::save_settings,
            commands::summary,
            commands::storage,
            commands::browse,
            commands::find,
            commands::details,
            commands::free_file,
            commands::move_file_area,
            commands::preview,
            commands::vault_keepers,
            commands::freeable,
            commands::delete_file,
            commands::open_file,
            commands::show_file,
            commands::show_root,
            commands::set_privacy,
            commands::set_notifications,
            commands::devices,
            commands::activity,
            commands::outgoing,
            commands::fetch,
            commands::set_limit,
            commands::quit,
            commands::starts_at_login,
            commands::set_starts_at_login,
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
    if let Some(given) = std::env::args().skip(1).find(|a| !a.starts_with("--")) {
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

