//! What the window is allowed to ask for.
//!
//! Every one of these is a thin wrapper over [`qurb_cli::View`] or the daemon's
//! status channel. That is the whole design: the window is a *display* of the
//! engine, so nothing here may decide anything about syncing, and anything it
//! wants to know has to be a question the engine can already answer.
//!
//! # Why the shapes are duplicated
//!
//! The types here look like the ones in `view`, with `serde` on them and
//! numbers widened to what JSON can carry. They are deliberately separate:
//! making the engine's own types serialisable would let a change in the
//! interface's needs reach back and reshape the index, which is the wrong
//! direction for that pressure to travel.
//!
//! `u64` becomes a string for anything that could exceed 2^53 — a byte count on
//! a large disk does. JavaScript has one number type and it is a double;
//! sending a terabyte as a number silently loses the low bits.

use crate::Hosted;
use qurb_cli::view::{Availability, View};
use serde::Serialize;
use std::sync::Arc;

/// Shorthand for the managed state every command takes.
type Host<'a> = tauri::State<'a, Arc<Hosted>>;

/// The result type every command returns: a message, not a panic.
///
/// Tauri turns an `Err` into a rejected promise, which the window shows in the
/// place the data would have been. A failure to read the index is a thing to
/// tell somebody about, not a thing to crash a window over.
type Answer<T> = std::result::Result<T, String>;

fn failed(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Big numbers as strings, small ones as numbers.
///
/// Used for byte counts. See the module note on why.
fn big(n: u64) -> String {
    n.to_string()
}

#[derive(Serialize)]
pub struct Summary {
    /// One word: starting, up to date, syncing, no devices reachable, needs
    /// attention.
    state: &'static str,
    root: String,
    identity: String,
    peers: usize,
    peers_reachable: usize,
    /// Unix seconds, or null if nothing has synced yet this run.
    last_sync: Option<i64>,
    problem: Option<String>,
    recent: Vec<RecentFile>,
}

#[derive(Serialize)]
pub struct RecentFile {
    path: String,
    at: i64,
    from_peer: bool,
}

#[derive(Serialize)]
pub struct Storage {
    files: String,
    chunks: String,
    used: String,
    limit: String,
    /// The size of the filesystem the folder is on: the most a limit could be.
    disk: String,
    over: bool,
    file_count: usize,
    evicted: usize,
    only_here: usize,
}

#[derive(Serialize)]
pub struct File {
    path: String,
    size: String,
    updated_at: i64,
    /// "here", "not here", or "only here".
    availability: &'static str,
}

#[derive(Serialize)]
pub struct Device {
    id: String,
    name: String,
    fingerprint: String,
    paired_at: i64,
    last_seen: Option<i64>,
}

#[derive(Serialize)]
pub struct Happened {
    id: i64,
    at: i64,
    kind: String,
    path: Option<String>,
    size: Option<String>,
    device: Option<String>,
    detail: Option<String>,
}

#[derive(Serialize)]
pub struct Outgoing {
    path: String,
    size: String,
    to: String,
}

#[derive(Serialize)]
pub struct Situation {
    /// Whether this folder already holds a device.
    set_up: bool,
    /// Whether the daemon is running and the rest of the commands will answer.
    running: bool,
    root: String,
    /// Why it is not running, when it should have been. Almost always a
    /// passphrase-protected key with no terminal to ask on.
    problem: Option<String>,
}

#[derive(Serialize)]
pub struct Folder {
    path: String,
    exists: bool,
    set_up: bool,
    writable: bool,
    existing_files: usize,
    /// Whether the count stopped short. See `setup::COUNT_CAP`.
    counted_all: bool,
    disk: String,
    free: String,
}

#[derive(Serialize)]
pub struct Settings {
    name: String,
    signal: String,
    relay: Option<String>,
    port: u16,
    /// How the key is kept: file, keystore, passphrase, platform.
    protection: String,
    root: String,
    identity: String,
}

/// What screen the window should be on.
///
/// Asked first, before anything else, because the answer decides whether there
/// is a device to ask about at all.
#[tauri::command]
pub fn situation(hosted: Host<'_>) -> Answer<Situation> {
    let root = hosted.root();
    let set_up = qurb_cli::is_set_up(&root);
    let running = hosted.is_running();
    Ok(Situation {
        set_up,
        running,
        root: root.display().to_string(),
        // A folder with a key that is not running got that way for a reason,
        // and the reason is on the terminal nobody launched this from.
        problem: (set_up && !running).then(|| {
            "this folder has a key that could not be opened — if it is protected by a \
             passphrase, start qurb from a terminal so it can ask"
                .to_string()
        }),
    })
}

/// What is true of a folder somebody is considering.
///
/// Answered for paths that do not exist yet, because the commonest first action
/// is to type somewhere new. Called on every keystroke, so it counts at most a
/// couple of thousand files before giving up.
#[tauri::command]
pub fn inspect_folder(path: String) -> Answer<Folder> {
    let expanded = expand(&path);
    let looked = qurb_cli::setup::inspect(&expanded);
    Ok(Folder {
        path: looked.path.display().to_string(),
        exists: looked.exists,
        set_up: looked.set_up,
        writable: looked.writable,
        existing_files: looked.existing_files,
        counted_all: looked.existing_files < qurb_cli::setup::COUNT_CAP,
        disk: big(looked.disk),
        free: big(looked.free),
    })
}

/// Make a new device, and hold its recovery phrase until it is confirmed.
///
/// The phrase is not returned here. It is held in the session and fetched by
/// [`shown_phrase`], so that the two are separate actions: creating a device is
/// not the same event as putting somebody's key on a screen, and keeping them
/// apart means the second can be repeated without the first.
#[tauri::command]
pub fn create_device(hosted: Host<'_>, path: String) -> Answer<()> {
    let root = expand(&path);
    hosted.aim_at(root.clone()).map_err(failed)?;
    let phrase = qurb_cli::setup::create(&root).map_err(failed)?;
    hosted.hold_phrase(phrase);
    Ok(())
}

/// The words, to put on the screen.
///
/// The window is expected to drop its copy as soon as it has drawn them. It
/// does not need to keep them: confirmation is checked here, against the copy
/// held in the session, which is itself dropped the moment it has served.
#[tauri::command]
pub fn shown_phrase(hosted: Host<'_>) -> Answer<Vec<String>> {
    hosted.pending_words().ok_or_else(|| "there is no phrase to show".to_string())
}

/// Check some of the words, and on success forget the phrase and start syncing.
///
/// `answers` are `[position, word]` pairs with one-based positions, as they
/// were shown. Getting them right is what the step is for: somebody who has not
/// actually written the words down cannot answer, and finding that out now is
/// the entire point of asking.
#[tauri::command]
pub fn confirm_phrase(hosted: Host<'_>, answers: Vec<(usize, String)>) -> Answer<bool> {
    if !hosted.phrase_matches(&answers) {
        return Ok(false);
    }
    hosted.forget_phrase();
    // A key created here is protected by a file, so nothing has to be typed.
    hosted.start(|| Ok(String::new())).map_err(failed)?;
    Ok(true)
}

/// Set this folder up with a key that already exists on another device.
#[tauri::command]
pub fn enrol_device(hosted: Host<'_>, path: String, phrase: String) -> Answer<()> {
    let parsed = qurb_keys::RecoveryPhrase::parse(&phrase).map_err(|_| {
        "those are not 24 valid words — check the spelling and the order".to_string()
    })?;
    let root = expand(&path);
    hosted.aim_at(root.clone()).map_err(failed)?;
    qurb_cli::setup::enrol(&root, &parsed).map_err(failed)?;
    hosted.start(|| Ok(String::new())).map_err(failed)
}

/// Show the recovery phrase for a device that is already set up.
///
/// Derived from the stored key rather than remembered, because it was never
/// remembered. Offered at all because the key is already in this folder:
/// somebody who can read it can read the files, so showing them the words gives
/// away nothing they did not already have.
#[tauri::command]
pub fn reveal_phrase(hosted: Host<'_>) -> Answer<Vec<String>> {
    let root = hosted.root();
    let vault = qurb_keys::Vault::at(&qurb_cli::store_dir(&root));
    let protection = vault.protection().map_err(failed)?;
    if protection.needs_passphrase() {
        return Err("this key is protected by a passphrase; use `qurb status` in a terminal"
            .to_string());
    }
    let key = vault.unlock(None).map_err(failed)?;
    Ok(qurb_cli::setup::reveal(&key).words().to_vec())
}

#[tauri::command]
pub fn settings(hosted: Host<'_>) -> Answer<Settings> {
    let root = hosted.root();
    let dir = qurb_cli::store_dir(&root);
    let config = qurb_cli::Config::load(&dir).map_err(failed)?;
    let protection = qurb_keys::Vault::at(&dir)
        .protection()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(Settings {
        name: config.name,
        signal: config.signal,
        relay: config.relay.map(|r| r.to_string()),
        port: config.port,
        protection,
        root: root.display().to_string(),
        identity: hosted.status().map(|s| s.identity).unwrap_or_default(),
    })
}

/// Change the settings that can be changed while running.
///
/// Written to the config file, which is the setting: a window that told the
/// daemon without writing it down would lose the change on the next restart.
/// The daemon re-reads what it can and the rest takes effect on restart, which
/// the screen says.
#[tauri::command]
pub fn save_settings(
    hosted: Host<'_>,
    name: String,
    signal: String,
    relay: Option<String>,
    port: u16,
) -> Answer<()> {
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;

    if name.trim().is_empty() {
        return Err("a device needs a name — it is what the others will call it".to_string());
    }
    config.name = name.trim().to_string();
    config.signal = signal.trim().to_string();
    config.port = port;
    config.relay = match relay.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        None => None,
        Some(text) => Some(
            text.parse()
                .map_err(|_| format!("{text} is not an address and port, like 1.2.3.4:9001"))?,
        ),
    };
    config.save(&dir).map_err(failed)
}

/// Expand a leading `~`, which people type and no filesystem understands.
fn expand(path: &str) -> std::path::PathBuf {
    let trimmed = path.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(trimmed)
}

/// The headline, from the daemon's live state.
///
/// Read from the watch channel rather than computed, because "is a device
/// reachable" is true for as long as a connection is open and false a moment
/// later. Nothing in the index can answer it.
#[tauri::command]
pub fn summary(hosted: Host<'_>) -> Answer<Summary> {
    let status = hosted.status().ok_or("this device is not set up yet")?;
    Ok(Summary {
        state: status.state.summary(),
        root: status.root.display().to_string(),
        identity: status.identity,
        peers: status.peers,
        peers_reachable: status.peers_reachable,
        last_sync: status.last_sync.and_then(unix),
        problem: status.problem,
        recent: status
            .recent
            .into_iter()
            .filter_map(|r| {
                Some(RecentFile { path: r.path, at: unix(r.at)?, from_peer: r.from_peer })
            })
            .collect(),
    })
}

#[tauri::command]
pub fn storage(hosted: Host<'_>) -> Answer<Storage> {
    let limit = hosted.limit();
    let numbers = hosted
        .with_store(|store| Ok(View::new(store, limit).storage()?))
        .map_err(failed)?;
    Ok(Storage {
        files: big(numbers.files),
        chunks: big(numbers.chunks),
        used: big(numbers.used()),
        limit: big(numbers.limit),
        disk: big(crate::disk_size(&hosted.root())),
        over: numbers.over(),
        file_count: numbers.file_count,
        evicted: numbers.evicted,
        only_here: numbers.only_here,
    })
}

#[tauri::command]
pub fn files(
    hosted: Host<'_>,
    under: Option<String>,
    limit: usize,
    offset: usize,
) -> Answer<Vec<File>> {
    let listed = hosted
        .with_store(|store| {
            Ok(View::new(store, 0).files(under.as_deref(), limit.min(500), offset)?)
        })
        .map_err(failed)?;
    Ok(listed.into_iter().map(as_file).collect())
}

#[tauri::command]
pub fn search(hosted: Host<'_>, text: String) -> Answer<Vec<File>> {
    // An empty box is not a search for everything; it is somebody who has not
    // typed yet, and answering it with the whole folder makes the list jump.
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let hits = hosted
        .with_store(|store| Ok(View::new(store, 0).search(text.trim(), 200)?))
        .map_err(failed)?;
    Ok(hits.into_iter().map(as_file).collect())
}

#[tauri::command]
pub fn devices(hosted: Host<'_>) -> Answer<Vec<Device>> {
    Ok(hosted
        .with_store(|store| Ok(View::new(store, 0).devices()?))
        .map_err(failed)?
        .into_iter()
        .map(|d| Device {
            id: d.id.short(),
            name: d.name,
            fingerprint: d.fingerprint,
            paired_at: d.paired_at,
            last_seen: d.last_seen,
        })
        .collect())
}

#[tauri::command]
pub fn activity(
    hosted: Host<'_>,
    path: Option<String>,
    limit: usize,
    before: Option<i64>,
) -> Answer<Vec<Happened>> {
    // History records a device by id, which is the right key and the wrong
    // thing to show somebody. Resolved once for the page rather than per row.
    let (rows, named) = hosted
        .with_store(|store| {
            let view = View::new(store, 0);
            let rows = match &path {
                Some(path) => view.history_of(path, limit.min(200))?,
                None => view.activity(limit.min(200), before)?,
            };
            Ok((rows, view.devices()?))
        })
        .map_err(failed)?;
    Ok(rows
        .into_iter()
        .map(|r| Happened {
            id: r.id,
            at: r.at,
            kind: r.kind.as_str().to_string(),
            path: r.path,
            size: r.size.map(big),
            device: r.device.map(|id| {
                named
                    .iter()
                    .find(|d| d.id == id)
                    .map(|d| d.name.clone())
                    // A device that has since been forgotten still happened.
                    .unwrap_or_else(|| id.short())
            }),
            detail: r.detail,
        })
        .collect())
}

#[tauri::command]
pub fn outgoing(hosted: Host<'_>) -> Answer<Vec<Outgoing>> {
    // Resolved to the name the person gave the device. A vault is scoped by
    // device id, which is the right key and the wrong thing to show somebody:
    // "waiting for 4cef0d89" tells them nothing they can act on.
    let (sending, named) = hosted
        .with_store(|store| {
            let view = View::new(store, 0);
            Ok((view.outgoing()?, view.devices()?))
        })
        .map_err(failed)?;
    Ok(sending
        .into_iter()
        .map(|o| Outgoing {
            path: o.path,
            size: big(o.size),
            to: named
                .iter()
                .find(|d| d.id == o.to)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| o.to.short()),
        })
        .collect())
}

/// Ask for a file whose local copy was dropped.
///
/// Records the request rather than performing it, exactly as `qurb fetch` does:
/// no peer may be reachable right now, and a request written to the index is
/// acted on whenever one next is. That is also what makes asking for a file
/// while offline work.
#[tauri::command]
pub fn fetch(hosted: Host<'_>, path: String) -> Answer<bool> {
    hosted.with_store(|store| Ok(store.db().want(&path)?)).map_err(failed)
}

/// Change how much disk this folder may use.
///
/// Written to the config file, which the daemon re-reads every maintenance
/// pass. Not sent to the daemon directly: the file is the setting, and a
/// window that told the daemon without writing it down would lose the change on
/// the next restart.
#[tauri::command]
pub fn set_limit(hosted: Host<'_>, bytes: String) -> Answer<()> {
    let bytes: u64 = bytes.parse().map_err(|_| "that is not a number of bytes".to_string())?;
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.limit = bytes;
    config.save(&dir).map_err(failed)
}

fn as_file(f: qurb_cli::view::File) -> File {
    File {
        path: f.path,
        size: big(f.size),
        updated_at: f.updated_at,
        availability: match f.availability {
            Availability::Here => "here",
            Availability::Elsewhere => "not here",
            Availability::OnlyHere => "only here",
        },
    }
}

/// A `SystemTime` as unix seconds, or nothing if the clock says it is before
/// 1970 — which is not worth propagating an error over.
fn unix(at: std::time::SystemTime) -> Option<i64> {
    at.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}
