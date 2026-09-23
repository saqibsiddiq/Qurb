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

/// The headline, from the daemon's live state.
///
/// Read from the watch channel rather than computed, because "is a device
/// reachable" is true for as long as a connection is open and false a moment
/// later. Nothing in the index can answer it.
#[tauri::command]
pub fn summary(hosted: tauri::State<'_, Hosted>) -> Answer<Summary> {
    let status = hosted.status.borrow().clone();
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
pub fn storage(hosted: tauri::State<'_, Hosted>) -> Answer<Storage> {
    let store = hosted.store.lock().map_err(|_| "the store is unavailable")?;
    let numbers = View::new(&store, hosted.limit()).storage().map_err(failed)?;
    Ok(Storage {
        files: big(numbers.files),
        chunks: big(numbers.chunks),
        used: big(numbers.used()),
        limit: big(numbers.limit),
        disk: big(crate::disk_size(&hosted.root)),
        over: numbers.over(),
        file_count: numbers.file_count,
        evicted: numbers.evicted,
        only_here: numbers.only_here,
    })
}

#[tauri::command]
pub fn files(
    hosted: tauri::State<'_, Hosted>,
    under: Option<String>,
    limit: usize,
    offset: usize,
) -> Answer<Vec<File>> {
    let store = hosted.store.lock().map_err(|_| "the store is unavailable")?;
    let listed = View::new(&store, 0)
        .files(under.as_deref(), limit.min(500), offset)
        .map_err(failed)?;
    Ok(listed.into_iter().map(as_file).collect())
}

#[tauri::command]
pub fn search(hosted: tauri::State<'_, Hosted>, text: String) -> Answer<Vec<File>> {
    // An empty box is not a search for everything; it is somebody who has not
    // typed yet, and answering it with the whole folder makes the list jump.
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let store = hosted.store.lock().map_err(|_| "the store is unavailable")?;
    let hits = View::new(&store, 0).search(text.trim(), 200).map_err(failed)?;
    Ok(hits.into_iter().map(as_file).collect())
}

#[tauri::command]
pub fn devices(hosted: tauri::State<'_, Hosted>) -> Answer<Vec<Device>> {
    let store = hosted.store.lock().map_err(|_| "the store is unavailable")?;
    Ok(View::new(&store, 0)
        .devices()
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
    hosted: tauri::State<'_, Hosted>,
    path: Option<String>,
    limit: usize,
    before: Option<i64>,
) -> Answer<Vec<Happened>> {
    let store = hosted.store.lock().map_err(|_| "the store is unavailable")?;
    let view = View::new(&store, 0);
    let rows = match &path {
        Some(path) => view.history_of(path, limit.min(200)).map_err(failed)?,
        None => view.activity(limit.min(200), before).map_err(failed)?,
    };

    // History records a device by id, which is the right key and the wrong
    // thing to show somebody. Resolved once for the page rather than per row.
    let named = view.devices().map_err(failed)?;
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
pub fn outgoing(hosted: tauri::State<'_, Hosted>) -> Answer<Vec<Outgoing>> {
    let store = hosted.store.lock().map_err(|_| "the store is unavailable")?;
    let view = View::new(&store, 0);

    // Resolved to the name the person gave the device. A vault is scoped by
    // device id, which is the right key and the wrong thing to show somebody:
    // "waiting for 4cef0d89" tells them nothing they can act on.
    let named = view.devices().map_err(failed)?;
    Ok(view
        .outgoing()
        .map_err(failed)?
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
pub fn fetch(hosted: tauri::State<'_, Hosted>, path: String) -> Answer<bool> {
    let store = hosted.store.lock().map_err(|_| "the store is unavailable")?;
    store.db().want(&path).map_err(failed)
}

/// Change how much disk this folder may use.
///
/// Written to the config file, which the daemon re-reads every maintenance
/// pass. Not sent to the daemon directly: the file is the setting, and a
/// window that told the daemon without writing it down would lose the change on
/// the next restart.
#[tauri::command]
pub fn set_limit(hosted: tauri::State<'_, Hosted>, bytes: String) -> Answer<()> {
    let bytes: u64 = bytes.parse().map_err(|_| "that is not a number of bytes".to_string())?;
    let dir = qurb_cli::store_dir(&hosted.root);
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
