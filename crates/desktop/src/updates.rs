//! Updating the window wherever the computer is, during the beta (decision
//! 0078).
//!
//! The latest release on GitHub says what it offers (`latest.json`, written
//! by `.github/workflows/release.yml`), and each build it names is signed
//! with the updater key, whose public half is in `tauri.conf.json`: a build
//! not signed with it is refused, wherever it came from. Looked for a minute
//! after starting and every six hours. Where an update can be put in place
//! while the window runs -- an AppImage on Linux -- it is, and takes over at
//! the next start; on Windows, where installing closes the window, it is
//! fetched and waits to be installed from Settings. In place, it restarts
//! itself into the new one once nobody is using the window (N-2). Installed by a package
//! manager, the window cannot replace itself, and says a newer one exists.
//!
//! The check sends nothing about the person: one request for a public file.

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// Where updating stands, for the window.
#[derive(Clone, Serialize, Default)]
pub struct UpdateState {
    /// This build's version.
    current: String,
    /// "idle", "checking", "downloading", "ready" (fetched, to install),
    /// "installed" (in place, taking over at the next start), "manual" (a
    /// newer one exists; this install cannot replace itself), or "failed".
    state: &'static str,
    /// The newer version, when there is one.
    version: Option<String>,
    /// How much of it has come, 0 to 1, while downloading.
    progress: Option<f64>,
    error: Option<String>,
}

struct Held {
    state: UpdateState,
    /// A fetched build waiting to be installed (Windows).
    bytes: Option<Vec<u8>>,
}

static HELD: Mutex<Option<Held>> = Mutex::new(None);

const FIRST_LOOK: Duration = Duration::from_secs(60);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

fn set(change: impl FnOnce(&mut Held)) {
    let mut held = HELD.lock().expect("update state poisoned");
    let held = held.get_or_insert_with(|| Held { state: UpdateState::default(), bytes: None });
    change(held);
}

pub fn state(app: &AppHandle) -> UpdateState {
    let held = HELD.lock().expect("update state poisoned");
    let mut state = held.as_ref().map(|h| h.state.clone()).unwrap_or_default();
    state.current = app.package_info().version.to_string();
    if state.state.is_empty() {
        state.state = "idle";
    }
    state
}

/// Whether this install can replace itself: an AppImage on Linux, the
/// installed app on Windows and macOS. A binary placed by hand or by a
/// package manager cannot.
fn replaceable() -> bool {
    !cfg!(target_os = "linux") || std::env::var_os("APPIMAGE").is_some()
}

/// Look now, and fetch what is found. Never fails: how it went is the state.
pub async fn look(app: AppHandle) {
    set(|h| {
        h.state.state = "checking";
        h.state.error = None;
    });
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let update = match found {
        Ok(Some(update)) => update,
        Ok(None) => return set(|h| h.state.state = "idle"),
        Err(e) => {
            tracing::warn!(error = %e, "could not look for an update");
            return set(|h| {
                h.state.state = "failed";
                h.state.error = Some(e.to_string());
            });
        }
    };
    let version = update.version.clone();
    tracing::info!(%version, "an update is available");
    if !replaceable() {
        return set(|h| {
            h.state.state = "manual";
            h.state.version = Some(version);
        });
    }
    set(|h| {
        h.state.state = "downloading";
        h.state.version = Some(version.clone());
        h.state.progress = Some(0.0);
    });
    let mut got = 0usize;
    let fetched = update
        .download(
            |chunk, total| {
                got += chunk;
                if let Some(total) = total.filter(|t| *t > 0) {
                    set(|h| h.state.progress = Some(got as f64 / total as f64));
                }
            },
            || {},
        )
        .await;
    let bytes = match fetched {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(error = %e, "could not fetch the update");
            return set(|h| {
                h.state.state = "failed";
                h.state.error = Some(e.to_string());
            });
        }
    };
    // In place now where the running window is not disturbed by it: the
    // AppImage is a file, and the one running keeps its own copy open.
    if cfg!(target_os = "linux") {
        match update.install(&bytes) {
            Ok(()) => {
                tracing::info!(%version, "update in place; it takes over at the next start");
                set(|h| {
                    h.state.state = "installed";
                    h.state.progress = None;
                });
            }
            Err(e) => set(|h| {
                h.state.state = "failed";
                h.state.error = Some(e.to_string());
            }),
        }
        return;
    }
    set(|h| {
        h.state.state = "ready";
        h.state.progress = None;
        h.bytes = Some(bytes);
    });
    *PENDING.lock().expect("pending update poisoned") = Some(update);
}

/// The update fetched and waiting to be installed (Windows), kept with what
/// it needs to install itself.
static PENDING: Mutex<Option<tauri_plugin_updater::Update>> = Mutex::new(None);

/// Put a fetched update in place and start the new one. On Linux it is in
/// place already: a restart is all.
pub fn apply(app: &AppHandle) -> Result<(), String> {
    let state = state(app);
    match state.state {
        "installed" => app.restart(),
        "ready" => {
            let update = PENDING.lock().expect("pending update poisoned").take().ok_or("nothing is waiting to be installed")?;
            let bytes = {
                let mut held = HELD.lock().expect("update state poisoned");
                held.as_mut().and_then(|h| h.bytes.take()).ok_or("nothing is waiting to be installed")?
            };
            update.install(bytes).map_err(|e| e.to_string())?;
            app.restart()
        }
        _ => Err("there is no update to install".to_string()),
    }
}

/// How often a waiting update looks for a quiet moment to restart in.
const QUIET_LOOK: Duration = Duration::from_secs(120);

/// An update in place restarts the program by itself once nobody is using
/// it -- the window closed, nothing moving, no person's folder open -- as
/// the phone installs while the app is away (the owner's review of
/// 2026-11-03, N-2): a computer that is everyone's server would otherwise
/// keep the old one for weeks, waiting for a button.
fn restart_when_quiet(app: AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(QUIET_LOOK).await;
            if state(&app).state != "installed" {
                continue;
            }
            let shown = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(true);
            let moving = app
                .try_state::<std::sync::Arc<crate::session::Hosted>>()
                .and_then(|hosted| hosted.status())
                .map(|s| !s.incoming.is_empty() || !s.outgoing.is_empty())
                .unwrap_or(true);
            if shown || moving || crate::mount::any() {
                continue;
            }
            tracing::info!("restarting into the update, nobody using the window");
            app.restart();
        }
    });
}

/// Look a minute after starting, and every six hours after.
pub fn watch(app: AppHandle) {
    restart_when_quiet(app.clone());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_LOOK).await;
        loop {
            let busy = matches!(state(&app).state, "downloading" | "installed" | "ready");
            if !busy {
                look(app.clone()).await;
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}
