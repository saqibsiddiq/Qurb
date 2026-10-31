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
use anyhow::Context as _;
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
    /// Files arriving right now. Live, like the rest of this struct: each
    /// exists while its bytes are moving and is gone the moment they stop.
    incoming: Vec<InFlight>,
    /// Files this device is serving to a device that asked for them. Not
    /// removed when they finish -- see `updated`.
    outgoing: Vec<InFlight>,
}

#[derive(Serialize)]
pub struct InFlight {
    path: String,
    /// The device at the other end.
    device: String,
    size: String,
    done: String,
    /// Unix seconds, for the page to work out a rate and a time left.
    started: i64,
    /// Unix seconds when it last moved. A send that has not moved for a few
    /// seconds is not being collected right now.
    updated: i64,
}

fn in_flight(t: qurb_cli::status::Transfer) -> InFlight {
    InFlight {
        path: t.path,
        device: t.device,
        size: big(t.size),
        done: big(t.done),
        started: unix(t.started).unwrap_or(0),
        updated: unix(t.updated).unwrap_or(0),
    }
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
    /// What is still free on it.
    free_disk: String,
    over: bool,
    file_count: usize,
    evicted: usize,
    only_here: usize,
}

#[derive(Serialize)]
pub struct Device {
    id: String,
    name: String,
    fingerprint: String,
    paired_at: i64,
    last_seen: Option<i64>,
    /// "direct" or "relay" while this device holds a connection to it, and
    /// absent when it does not.
    route: Option<&'static str>,
    /// The address at the other end of that connection, for the details.
    address: Option<String>,
    /// "own", "guest" or "host" (decision 0060).
    relation: &'static str,
    /// For a guest: what this computer keeps for them, sealed, in bytes.
    kept: Option<String>,
    /// "phone", "computer" or "replica", once the device has said: what its
    /// icon is, rather than a guess from its name.
    kind: Option<String>,
    /// A phone's WhatsApp backup kept here, in its vault: files and bytes
    /// (decision 0062).
    whatsapp: Option<WhatsAppKept>,
    /// Everything kept here for it: files and bytes.
    held: Option<WhatsAppKept>,
    /// The device this one was set up again as, by id: listed as that one
    /// (the owner's item 6, 2026-10-14).
    replaced_by: Option<String>,
    /// The picture it chose for itself, as a `data:` address for an `<img>`
    /// (the owner's list of 2026-10-26, item 7), or none.
    picture: Option<String>,
    /// The device this one replaced, by id.
    earlier: Option<String>,
    /// That it is counted as another of the person's devices set up again
    /// because they said so, not by its name (`Db::set_same_device`): the
    /// word can be taken back.
    said_same: bool,
}

#[derive(Serialize)]
pub struct WhatsAppKept {
    files: u64,
    bytes: String,
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
    /// The device's short id, for naming it back in `cancel_send`: two devices
    /// may share a name.
    to_id: String,
}

#[derive(Serialize)]
pub struct Situation {
    /// Whether this folder already holds a device.
    set_up: bool,
    /// Whether the daemon is running and the rest of the commands will answer.
    running: bool,
    root: String,
    /// Set up, not running, and the key needs a passphrase: the window asks.
    locked: bool,
    /// Why it is not running, when it should have been and nothing here can
    /// ask for what is missing.
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
    /// Where files sent here go, as written: empty for the default, `off`, or
    /// a directory.
    downloads: String,
    /// Where that actually is, or why it is refused.
    downloads_at: Result<Option<String>, String>,
    /// This build: the window's version, its protocol and index schema.
    version: String,
    /// Files added here go to this device's Private Vault.
    own_files_private: bool,
    /// The three notifications are on.
    notifications: bool,
    /// The other devices holding this key, by name, and whether a phone is
    /// among them: the phone is what can give a new computer the key, and
    /// keeps it in its Google backup (decision 0053).
    key_also_on: Vec<String>,
    phone_holds_key: bool,
    /// How fast files may go to and come from other devices, bytes a
    /// second, as strings like every other size; "0" for no limit.
    upload_limit: String,
    download_limit: String,
    /// This computer's picture, as its paired devices are shown it: a
    /// `data:` address, or none.
    picture: Option<String>,
}

/// What screen the window should be on.
///
/// Asked first, before anything else, because the answer decides whether there
/// is a device to ask about at all.
#[tauri::command(async)]
pub fn situation(hosted: Host<'_>) -> Answer<Situation> {
    let root = hosted.root();
    let set_up = qurb_cli::is_set_up(&root);
    let running = hosted.is_running();
    let locked = set_up && !running && key_protection(&root) == Some(qurb_keys::Protection::Passphrase);
    Ok(Situation {
        set_up,
        running,
        locked,
        root: root.display().to_string(),
        problem: (set_up && !running && !locked)
            .then(|| "this folder has a key that could not be opened".to_string()),
    })
}

/// How the key in this folder is kept, if it can be told.
pub fn key_protection(root: &std::path::Path) -> Option<qurb_keys::Protection> {
    qurb_keys::Vault::at(&qurb_cli::store_dir(root)).protection().ok()
}

/// Open a passphrase-protected key and start syncing (decision 0046).
///
/// The passphrase goes to the key-derivation function and nowhere else: not
/// kept, not logged. A wrong one is said as such, and changes nothing.
#[tauri::command(async)]
pub fn unlock(hosted: Host<'_>, passphrase: String) -> Answer<()> {
    hosted.start(move || Ok(passphrase)).map_err(|e| {
        match e.chain().any(|cause| {
            matches!(cause.downcast_ref::<qurb_keys::Error>(), Some(qurb_keys::Error::WrongPassphrase))
        }) {
            true => "that passphrase does not open this key".to_string(),
            false => format!("{e:#}"),
        }
    })
}

/// The Security section (brief §33): what protects this device, and what has
/// happened that bears on trust.
#[derive(Serialize)]
pub struct Security {
    /// This device's identity: the fingerprint its paired devices pinned.
    identity: String,
    /// "file", "keystore" or "passphrase".
    protection: String,
    /// Pairings and removals, newest first.
    events: Vec<Happened>,
}

#[tauri::command(async)]
pub fn security(hosted: Host<'_>) -> Answer<Security> {
    let root = hosted.root();
    // Read, never made: looking at Settings must not create an identity in a
    // folder that has gone.
    let identity = qurb_peer::Identity::load(&qurb_cli::store_dir(&root))
        .map(|id| id.fingerprint().to_string())
        .map_err(failed)?;
    let protection = key_protection(&root).map(|p| p.as_str().to_string()).unwrap_or_default();
    let events = hosted
        .with_store(|store| {
            let names = store.db().device_names()?;
            Ok(store
                .db()
                .activity(200, None)?
                .into_iter()
                .filter(|r| {
                    matches!(r.kind, qurb_storage::db::Event::Paired | qurb_storage::db::Event::Removed)
                })
                .take(20)
                .map(|r| Happened {
                    id: r.id,
                    at: r.at,
                    kind: r.kind.as_str().to_string(),
                    path: r.path,
                    size: r.size.map(big),
                    device: r.device.map(|id| names.get(&id).cloned().unwrap_or_else(|| id.short())),
                    detail: r.detail,
                })
                .collect())
        })
        .map_err(failed)?;
    Ok(Security { identity, protection, events })
}

/// Change how the key is kept: "keystore", "passphrase" or "file". The key
/// itself is untouched -- this changes the lock, not what it protects -- and
/// the running daemon, which already holds it, carries on.
#[tauri::command(async)]
pub fn protect_key(
    hosted: Host<'_>,
    to: String,
    current: Option<String>,
    new: Option<String>,
) -> Answer<()> {
    let to: qurb_keys::Protection = to.parse().map_err(failed)?;
    if to == qurb_keys::Protection::Passphrase && new.as_deref().is_none_or(|p| p.chars().count() < 8) {
        return Err("a passphrase needs at least 8 characters".into());
    }
    qurb_keys::Vault::at(&qurb_cli::store_dir(&hosted.root()))
        .protect(to, current.as_deref(), new.as_deref())
        .map_err(|e| match e {
            qurb_keys::Error::WrongPassphrase => "the current passphrase is not right".to_string(),
            e => e.to_string(),
        })
}

/// What is true of a folder somebody is considering.
///
/// Answered for paths that do not exist yet, because the commonest first action
/// is to type somewhere new. Called on every keystroke, so it counts at most a
/// couple of thousand files before giving up.
#[tauri::command(async)]
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

/// Make a new device and start it.
///
/// Nothing to write down first (decision 0052): another device is added with
/// a code, which carries the key. The 24 words can still be shown, in
/// Settings, by [`reveal_phrase`].
///
/// `allowance` is the disk this device may use, in bytes, from the question
/// asked before the key is made (decision 0038). Written with the rest of the
/// device's settings, before anything syncs.
#[tauri::command(async)]
pub fn create_device(hosted: Host<'_>, path: String, allowance: String) -> Answer<()> {
    let bytes = bytes_of(&allowance)?;
    let root = expand(&path);
    hosted.aim_at(root.clone()).map_err(failed)?;
    qurb_cli::setup::create(&root).map_err(failed)?;
    qurb_cli::setup::allow(&root, bytes).map_err(failed)?;
    // A key created here is protected by a file, so nothing has to be typed.
    hosted.start(|| Ok(String::new())).map_err(failed)?;
    start_at_login_from_now();
    Ok(())
}

/// A device just set up starts at login from now on -- which the install
/// script used to arrange, and a package installed for every user cannot: the
/// phone can only reach this computer while qurb is running here. Settings
/// turns it off like any other time. A failure costs only the convenience.
fn start_at_login_from_now() {
    if let (Some(dir), Ok(program)) = (crate::autostart::config_dir(), std::env::current_exe()) {
        if let Err(e) = crate::autostart::enable(&dir, &program) {
            tracing::warn!(error = %e, "could not arrange to start at login");
        }
    }
}

/// What somebody typed as a custom allowance, in bytes — or why it is not one.
#[tauri::command(async)]
pub fn read_allowance(text: String) -> Answer<String> {
    qurb_cli::setup::allowance(&text).map(big).map_err(failed)
}

fn bytes_of(allowance: &str) -> Answer<u64> {
    match allowance.parse::<u64>() {
        Ok(0) | Err(_) => Err("choose how much space qurb may use".to_string()),
        Ok(bytes) => Ok(bytes),
    }
}

/// Set this folder up with a key that already exists on another device.
///
/// `allowance` is as for [`create_device`].
#[tauri::command(async)]
pub fn enrol_device(
    hosted: Host<'_>,
    path: String,
    phrase: String,
    allowance: String,
) -> Answer<()> {
    let bytes = bytes_of(&allowance)?;
    let parsed = qurb_keys::RecoveryPhrase::parse(&phrase).map_err(|_| {
        "those are not 24 valid words — check the spelling and the order".to_string()
    })?;
    let root = expand(&path);
    hosted.aim_at(root.clone()).map_err(failed)?;
    qurb_cli::setup::enrol(&root, &parsed).map_err(failed)?;
    qurb_cli::setup::allow(&root, bytes).map_err(failed)?;
    hosted.start(|| Ok(String::new())).map_err(failed)?;
    start_at_login_from_now();
    Ok(())
}

/// Set this folder up as another of the same person's devices, from the code
/// one of them is showing, and pair the two (decision 0052): the key comes
/// with the code, so nothing is typed but the code. Returns the other
/// device's name.
///
/// `allowance` is as for [`create_device`].
#[tauri::command]
pub async fn join_new_device(
    hosted: Host<'_>,
    path: String,
    code: String,
    allowance: String,
) -> Answer<String> {
    let bytes = bytes_of(&allowance)?;
    let root = expand(&path);
    hosted.aim_at(root.clone()).map_err(failed)?;
    let paired = qurb_cli::setup::join(&root, &code).await.map_err(|e| match e
        .downcast_ref::<qurb_peer::Error>()
    {
        Some(qurb_peer::Error::InviteExpired) => {
            "that code has expired — ask the other device for a new one".to_string()
        }
        Some(qurb_peer::Error::NoKeyGiven) => {
            "the other device did not give its key — the code may already have been used, or that device needs updating".to_string()
        }
        _ => format!("{e:#}"),
    })?;
    qurb_cli::setup::allow(&root, bytes).map_err(failed)?;
    hosted.start(|| Ok(String::new())).map_err(failed)?;
    start_at_login_from_now();
    Ok(paired.name)
}

/// Show the recovery phrase for a device that is already set up.
///
/// Derived from the stored key rather than remembered, because it was never
/// remembered. Offered at all because the key is already in this folder:
/// somebody who can read it can read the files, so showing them the words gives
/// away nothing they did not already have.
#[tauri::command(async)]
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

#[tauri::command(async)]
pub fn settings(hosted: Host<'_>) -> Answer<Settings> {
    let root = hosted.root();
    let dir = qurb_cli::store_dir(&root);
    let config = qurb_cli::Config::load(&dir).map_err(failed)?;
    let protection = qurb_keys::Vault::at(&dir)
        .protection()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    // Every paired device holds this key: pairing checks it (decision 0053).
    let (key_also_on, phone_holds_key, picture) = hosted
        .with_store(|store| {
            let peers = store.db().trusted_peers()?;
            let phone = peers.iter().any(|p| {
                store.db().peer_kind(&p.device_id).ok().flatten().as_deref() == Some("phone")
            });
            let picture = store.db().own_profile()?.and_then(|p| p.picture);
            Ok((peers.into_iter().map(|p| p.name).collect::<Vec<_>>(), phone, picture))
        })
        .unwrap_or_default();

    Ok(Settings {
        name: config.name,
        signal: config.signal,
        relay: config.relay.clone(),
        port: config.port,
        protection,
        downloads: config.downloads.as_setting(),
        downloads_at: config
            .downloads
            .resolve(&root)
            .map(|dir| dir.map(|d| d.display().to_string()))
            .map_err(|e| e.to_string()),
        root: root.display().to_string(),
        identity: hosted.status().map(|s| s.identity).unwrap_or_default(),
        version: qurb_cli::version().replacen("qurb ", &format!("window {} · engine ", env!("CARGO_PKG_VERSION")), 1),
        own_files_private: config.own_files_private,
        notifications: config.notifications,
        key_also_on,
        phone_holds_key,
        upload_limit: config.upload_limit.to_string(),
        download_limit: config.download_limit.to_string(),
        picture: picture.as_deref().map(picture_address),
    })
}

/// A device's picture as a `data:` address an `<img>` shows: a JPEG, as both
/// apps make them, or a PNG.
fn picture_address(bytes: &[u8]) -> String {
    use base64::Engine as _;
    let kind = if bytes.starts_with(b"\x89PNG") { "png" } else { "jpeg" };
    format!("data:image/{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Give this computer a picture -- a small JPEG the window made from the
/// one chosen -- or take it away with none. Every paired device is shown it
/// from its next sync (the owner's list of 2026-10-26, item 7).
#[tauri::command(async)]
pub fn set_picture(hosted: Host<'_>, picture: Option<Vec<u8>>) -> Answer<()> {
    if picture.as_ref().is_some_and(|p| p.len() > qurb_peer::wire::MAX_PICTURE) {
        return Err("that picture is too large".to_string());
    }
    hosted
        .with_store(|store| Ok(store.db().set_own_profile(None, Some(picture.as_deref()))?))
        .map(|_| ())
        .map_err(failed)
}

/// Change the settings that can be changed while running.
///
/// Written to the config file, which is the setting: a window that told the
/// daemon without writing it down would lose the change on the next restart.
/// The daemon re-reads what it can and the rest takes effect on restart, which
/// the screen says.
#[tauri::command(async)]
pub fn save_settings(
    hosted: Host<'_>,
    name: String,
    signal: String,
    relay: Option<String>,
    port: u16,
    downloads: String,
) -> Answer<()> {
    let root = hosted.root();
    let dir = qurb_cli::store_dir(&root);
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;

    // Refused here, where it can be corrected, rather than by the daemon,
    // which would keep the old setting and say so only in its log. A place
    // overlapping the folder would sync every file sent here to every device.
    let downloads = qurb_cli::config::Downloads::parse(&downloads);
    downloads.resolve(&root).map_err(failed)?;
    config.downloads = downloads;

    if name.trim().is_empty() {
        return Err("a device needs a name — it is what the others will call it".to_string());
    }
    config.name = name.trim().to_string();
    // Told to every paired device at its next sync, not only to the next
    // one to pair (the owner's list of 2026-10-26, item 7).
    hosted.with_store(|store| Ok(qurb_cli::daemon::tell_name(store, &config.name)?)).map_err(failed)?;
    config.signal = signal.trim().to_string();
    config.port = port;
    config.relay = match relay.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        None => None,
        Some(text) => {
            qurb_peer::relay_address_ok(text)?;
            Some(text.to_string())
        }
    };
    config.save(&dir).map_err(failed)
}

/// Open the folder files sent here are saved to, in the file manager.
#[tauri::command(async)]
pub fn open_downloads(hosted: Host<'_>) -> Answer<()> {
    let root = hosted.root();
    let config = qurb_cli::Config::load(&qurb_cli::store_dir(&root)).map_err(failed)?;
    let dir = config
        .downloads
        .resolve(&root)
        .map_err(failed)?
        .ok_or("files sent here are kept in the folder")?;
    std::fs::create_dir_all(&dir).map_err(failed)?;
    reveal(&dir)
}

/// Open the folder a received file was saved to, from its activity entry.
///
/// Takes the entry, not a path. The page never names somewhere to open: the
/// path comes from what the daemon recorded, and is opened only if it is
/// inside the downloads directory -- so nothing the page says can make this
/// open anywhere else on the disk.
#[tauri::command(async)]
pub fn show_received(hosted: Host<'_>, id: i64) -> Answer<()> {
    let root = hosted.root();
    let entry = hosted
        .with_store(|store| Ok(store.db().activity_entry(id)?))
        .map_err(failed)?
        .ok_or("that is no longer in the history")?;
    let config = qurb_cli::Config::load(&qurb_cli::store_dir(&root)).map_err(failed)?;
    let downloads = config.downloads.resolve(&root).map_err(failed)?.ok_or("no downloads folder")?;
    reveal(&folder_to_show(entry.detail.as_deref().unwrap_or(""), &downloads)?)
}

/// The folder to open for a received file, from what its activity entry says.
///
/// Only a folder inside `downloads`, compared after resolving links, so that
/// neither a changed setting, a moved file nor anything written into the
/// history can make "Show in folder" open somewhere else.
fn folder_to_show(detail: &str, downloads: &std::path::Path) -> Answer<std::path::PathBuf> {
    let saved = detail
        .split_once("; saved to ")
        .map(|(_, path)| std::path::PathBuf::from(path))
        .ok_or("that file was not saved to Downloads")?;
    let folder = saved.parent().ok_or("that file has no folder")?;
    let (Ok(folder), Ok(downloads)) = (folder.canonicalize(), downloads.canonicalize()) else {
        return Err("that folder is not there any more".to_string());
    };
    if !folder.starts_with(&downloads) {
        return Err("that file is no longer in the downloads folder".to_string());
    }
    Ok(folder)
}

/// Hand a folder to the desktop's file manager: `xdg-open` on Linux, which
/// every desktop answers with its own, and Explorer on Windows.
fn reveal(dir: &std::path::Path) -> Answer<()> {
    qurb_cli::platform::open(dir).map_err(|e| format!("could not open the file manager: {e}"))
}

/// Where a folder somebody typed is.
fn expand(path: &str) -> std::path::PathBuf {
    expand_from(path, std::env::var_os("HOME").map(std::path::PathBuf::from).as_deref())
}

/// The same, from a given home folder.
///
/// A leading `~` is the home folder, which people type and no filesystem
/// understands. A path that does not start at `/` is taken from the home folder
/// too, as a file chooser would. Left relative it was taken from whichever
/// directory the window happened to start in, and `home/project/qurb`, typed
/// once, made a second device at `~/home/project/qurb` that the window then
/// opened at every login instead of the folder paired with the phone. The
/// window shows the path this returns before anything is made there.
///
/// Nothing typed is nowhere, not the home folder: setting up qurb in the home
/// folder itself is never what an empty box meant.
fn expand_from(path: &str, home: Option<&std::path::Path>) -> std::path::PathBuf {
    let trimmed = path.trim();
    let Some(home) = home else { return std::path::PathBuf::from(trimmed) };
    if trimmed.is_empty() {
        std::path::PathBuf::new()
    } else if trimmed == "~" {
        home.to_path_buf()
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        home.join(rest)
    } else if std::path::Path::new(trimmed).is_relative() {
        home.join(trimmed)
    } else {
        std::path::PathBuf::from(trimmed)
    }
}

#[derive(Serialize)]
pub struct Invitation {
    /// The code itself: what a camera reads and what `qurb join` takes.
    code: String,
    /// The same thing in a form somebody can read down a telephone.
    spoken: String,
    /// Unix seconds. The code stops working at this point, and the screen
    /// counts down to it rather than leaving somebody reading out a dead code.
    expires_at: i64,
    /// An SVG of the code, to put on the screen.
    qr: Option<String>,
    /// Six digits a device on the same network types instead (decision 0066).
    digits: Option<String>,
}

#[derive(Serialize)]
pub struct PairingState {
    /// "none", "waiting", "asking", "paired", "expired" or "failed".
    state: &'static str,
    name: Option<String>,
    fingerprint: Option<String>,
    message: Option<String>,
    /// While "asking": the number the device asking should be showing
    /// (decision 0053).
    number: Option<String>,
    /// While "asking" and once "paired": "phone", "computer" or "replica",
    /// as the device said, for its icon.
    kind: Option<String>,
    wants_key: bool,
    /// Another person's device, asking to visit as a guest (decision 0060).
    guest: bool,
    /// The six digits were tried wrongly too often and no longer work.
    digits_withdrawn: bool,
}

impl PairingState {
    fn of(state: &'static str) -> Self {
        Self {
            state,
            name: None,
            fingerprint: None,
            message: None,
            number: None,
            kind: None,
            wants_key: false,
            guest: false,
            digits_withdrawn: false,
        }
    }
}

/// What a send did.
#[derive(Serialize)]
pub struct SendReport {
    /// How many files are now waiting for the device.
    sent: usize,
    /// Their total size.
    bytes: String,
    /// The name the recipient will see, when exactly one file was sent.
    only: Option<String>,
    /// What was picked and not sent, and why.
    skipped: Vec<Skipped>,
}

#[derive(Serialize)]
pub struct Skipped {
    path: String,
    why: String,
}

/// The paired device a name or fingerprint refers to: the same lookup
/// `qurb send` uses, so the two cannot disagree about which device is meant.
fn recipient(hosted: &Host<'_>, to: &str) -> Result<qurb_cli::Device, String> {
    match hosted.with_store(|store| Ok(View::new(store, 0).device_named(to)?)).map_err(failed)? {
        qurb_cli::Recipient::One(device) => Ok(*device),
        qurb_cli::Recipient::Unknown { .. } => Err(format!("no paired device {to}")),
        qurb_cli::Recipient::Several(_) => Err(format!("more than one device is called {to}")),
    }
}

/// A file picked to send that went to that device before (decision 0059).
#[derive(Serialize)]
pub struct Earlier {
    /// Where it is on this disk, which is how `send_files` is told to leave
    /// it out.
    path: String,
    /// The name it went under then.
    sent_as: String,
    /// When, in unix seconds.
    at: i64,
}

/// Which of the files picked went to `to` before: asked before sending, so
/// that the window can say so and let the person choose (decision 0059).
/// Folders are looked inside, as sending them would.
#[tauri::command(async)]
pub fn sent_before(hosted: Host<'_>, paths: Vec<String>, to: String) -> Answer<Vec<Earlier>> {
    let device = recipient(&hosted, &to)?;
    let picked: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    let sources: Vec<String> = qurb_cli::send::plan(&picked)
        .files
        .into_iter()
        .map(|(_, source)| source.to_string_lossy().into_owned())
        .collect();
    let found = hosted
        .with_store(|store| Ok(store.sent_before(&sources, &device.id)?))
        .map_err(failed)?;
    Ok(found
        .into_iter()
        .map(|e| Earlier { path: e.source, sent_as: e.sent_as, at: e.at })
        .collect())
}

/// Send files and folders to one device, and to nobody else.
///
/// A folder is sent whole, under its own name. Each file is stored separately
/// and the session let go of in between, so a folder of a thousand files does
/// not stop the window answering while it is chunked. The chunking happens on
/// this thread, a Tauri worker rather than the daemon's: the daemon is serving
/// peers while this runs, and a large file must not stop it.
///
/// The recipient is named by its short fingerprint, which is what the devices
/// screen shows — a device id would be the right key and the wrong thing to put
/// in front of somebody.
///
/// `leave_out` names files the person chose not to send again, having been
/// told by [`sent_before`] that they went there before.
#[tauri::command(async)]
pub fn send_files(
    hosted: Host<'_>,
    paths: Vec<String>,
    to: String,
    leave_out: Option<Vec<String>>,
) -> Answer<SendReport> {
    let device = recipient(&hosted, &to)?;
    let picked: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    let plan = qurb_cli::send::plan(&picked);
    // Files sent there before that the person chose not to send again
    // (decision 0059), by where they are on this disk.
    let leave_out: std::collections::HashSet<std::path::PathBuf> =
        leave_out.unwrap_or_default().into_iter().map(std::path::PathBuf::from).collect();

    let mut skipped: Vec<Skipped> = plan
        .skipped
        .into_iter()
        .map(|(path, why)| Skipped { path: path.display().to_string(), why })
        .collect();
    let (mut sent, mut bytes) = (Vec::new(), 0u64);
    for (name, source) in plan.files.into_iter().filter(|(_, source)| !leave_out.contains(source)) {
        let size = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);
        match hosted.with_store_mut(|store| Ok(store.send_to_vault(&name, &source, &device.id)?)) {
            Ok(_) => {
                sent.push(name);
                bytes += size;
            }
            Err(e) => {
                skipped.push(Skipped { path: source.display().to_string(), why: e.to_string() })
            }
        }
    }
    // Told to the device it is for now, rather than at the daemon's next check.
    if !sent.is_empty() {
        hosted.nudge();
    }

    Ok(SendReport {
        only: (sent.len() == 1).then(|| sent[0].clone()),
        sent: sent.len(),
        bytes: big(bytes),
        skipped,
    })
}

/// Show a code, and start answering it.
///
/// Returns as soon as there is something to put on the screen; the waiting
/// happens behind it and is asked about with [`pairing_state`]. A command that
/// only returned once somebody had paired would leave the code unobtainable
/// for the five minutes it is valid, which is the one window in which it is
/// any use.
///
/// `async` although nothing in it waits, because opening the pairing host
/// binds a QUIC endpoint, and that needs the Tokio runtime to register its
/// socket with. Tauri runs a synchronous command on the main thread, outside
/// the runtime; this one was synchronous until 2026-09-28 and failed every
/// time with "no async runtime found", which the fixture page could not show.
///
/// With `guest`, the code is for another person's device to visit this
/// computer as a guest, keeping its own key (decision 0060).
#[tauri::command]
pub async fn start_pairing(hosted: Host<'_>, guest: Option<bool>) -> Answer<Invitation> {
    let guest = guest.unwrap_or(false);
    let (store, identity, name) = hosted.for_pairing().map_err(failed)?;
    let master = hosted.master().map_err(failed)?;
    let now = now();

    // Port zero, not the configured one. The daemon in this process is already
    // listening on that, and a pairing host that failed to bind would be a
    // pairing screen that could not open while syncing was working — which is
    // every time somebody would use it. The invite carries whatever port this
    // gets, so the far end dials the right one either way.
    let bind = "0.0.0.0:0".parse().unwrap();
    let host = match guest {
        true => qurb_peer::PairingHost::open_for_guest(bind, &identity, now),
        false => qurb_peer::PairingHost::open(bind, &identity, now),
    }
    .map_err(failed)?;

    let code = host.invite().encode();
    let spoken = host.invite().for_humans();
    let expires_at = host.invite().expires_at;
    // A code that cannot be drawn is still a code. It can be typed, and it can
    // be read aloud, so failing to render is worth noting and not worth
    // refusing over.
    let qr = qurb_cli::qr::svg(&code).ok();
    let digits = host.digits().map(|d| qurb_peer::short::spaced(&d));

    let attempt = hosted.begin_pairing(code.clone(), spoken.clone(), expires_at);
    let digits_work = Arc::clone(&attempt.digits_work);
    let waiting = Arc::clone(&attempt);
    let asker = Arc::clone(&attempt);
    let nudge = hosted.nudger();
    attempt.watch(tauri::async_runtime::spawn(async move {
        // A device with no key joining with this code is given this one's
        // (decision 0052): adding a phone is scanning, not typing 24 words --
        // once the person here approves it, comparing the number it shows
        // (decision 0053). The window shows the request and answers.
        let ours = qurb_peer::Ours { name: &name, kind: "computer", key: &master };
        let approve = |asking: qurb_peer::Asking| {
            let told = asker.ask(&asking);
            async move { told.await.unwrap_or(false) }
        };
        // Whether the digits still work, kept current for the screen while
        // the code waits.
        let watching = async {
            while host.digits().is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            digits_work.store(false, std::sync::atomic::Ordering::SeqCst);
            std::future::pending::<()>().await
        };
        let outcome = tokio::select! {
            outcome = host.wait(store, &ours, now, approve) => outcome,
            _ = watching => unreachable!("watches until dropped"),
        };
        let outcome = match outcome {
            Ok(peer) => {
                // The daemon syncs with it now, not at its next check.
                nudge.notify_one();
                crate::session::Pairing::Paired {
                    name: peer.name,
                    fingerprint: peer.fingerprint.short(),
                    kind: peer.kind,
                }
            }
            // The ordinary ending when nobody types the code in time. Not an
            // error to report as one: the only useful thing to say is that the
            // code is dead and another can be had.
            Err(qurb_peer::Error::InviteExpired) => crate::session::Pairing::Expired,
            Err(e) => crate::session::Pairing::Failed(e.to_string()),
        };
        waiting.settle(outcome);
    }));

    Ok(Invitation { code, spoken, expires_at, qr, digits })
}

/// How the attempt on screen is going. Polled while the code is up.
#[tauri::command(async)]
pub fn pairing_state(hosted: Host<'_>) -> Answer<PairingState> {
    use crate::session::Pairing;
    let Some(attempt) = hosted.attempt() else {
        return Ok(PairingState::of("none"));
    };

    Ok(match attempt.state() {
        Pairing::Waiting => PairingState {
            digits_withdrawn: !attempt.digits_work.load(std::sync::atomic::Ordering::SeqCst),
            ..PairingState::of("waiting")
        },
        Pairing::Asking { name, kind, number, wants_key, guest } => PairingState {
            name: Some(name),
            number: Some(number),
            kind,
            wants_key,
            guest,
            ..PairingState::of("asking")
        },
        Pairing::Paired { name, fingerprint, kind } => PairingState {
            name: Some(name),
            fingerprint: Some(fingerprint),
            kind,
            ..PairingState::of("paired")
        },
        Pairing::Expired => PairingState::of("expired"),
        Pairing::Failed(message) => PairingState { message: Some(message), ..PairingState::of("failed") },
    })
}

/// The person's answer to the device asking to pair (decision 0053).
#[tauri::command(async)]
pub fn answer_pairing(hosted: Host<'_>, approve: bool) -> Answer<()> {
    match hosted.attempt() {
        Some(attempt) if attempt.answer(approve) => Ok(()),
        _ => Err("no device is asking any more".to_string()),
    }
}

/// The number this computer will show while the device showing `code`
/// approves it, before joining it (decision 0053). Six digits are found on
/// the network here (decision 0066), once: joining uses what was found.
#[tauri::command]
pub async fn pairing_number(hosted: Host<'_>, code: String) -> Answer<String> {
    let (_, identity, _) = hosted.for_pairing().map_err(failed)?;
    let invite = typed(&code, &identity).await?;
    Ok(invite.number_for(&identity.fingerprint()))
}

/// A code typed into the window, long or six digits, as an invite.
async fn typed(code: &str, identity: &qurb_peer::Identity) -> Answer<qurb_peer::Invite> {
    qurb_peer::short::resolve(code.trim(), identity).await.map_err(|e| match e {
        qurb_peer::Error::BadInvite { .. } => {
            "that is not a pairing code — type the six digits, or the whole code".to_string()
        }
        other => other.to_string(),
    })
}

/// The same, for a folder being set up by joining: its certificate is made
/// now if it has none, and is the one joining then presents.
#[tauri::command]
pub async fn setup_pairing_number(path: String, code: String) -> Answer<String> {
    qurb_cli::setup::number_for(&expand(&path), &code).await.map_err(failed)
}

/// Stop showing a code, and stop answering it.
///
/// Both halves matter. A cancelled invite that still worked would be worse
/// than having no cancel button at all: the code would be off the screen and
/// live, which is precisely the state somebody pressing cancel is trying to
/// avoid.
#[tauri::command(async)]
pub fn stop_pairing(hosted: Host<'_>) -> Answer<()> {
    hosted.end_pairing();
    Ok(())
}

/// Join a device that is showing a code.
///
/// Short enough to await directly: this dials an address that is on the screen
/// in front of somebody, and either works or does not within seconds.
#[tauri::command]
pub async fn join_device(hosted: Host<'_>, code: String) -> Answer<PairingState> {
    let (store, identity, name) = hosted.for_pairing().map_err(failed)?;
    let invite = typed(&code, &identity).await?;
    let master = hosted.master().map_err(failed)?;

    // The other device asks its person to approve this one, comparing the
    // number `pairing_number` gave this screen (decision 0053).
    let ours = qurb_peer::Ours { name: &name, kind: "computer", key: &master };
    let peer = qurb_peer::accept(&invite, &identity, store, &ours, now())
        .await
        .map_err(|e| match e {
            qurb_peer::Error::InviteExpired => {
                "that code has expired — ask the other device for a new one".to_string()
            }
            other => other.to_string(),
        })?;
    hosted.nudge();

    Ok(PairingState {
        name: Some(peer.name),
        fingerprint: Some(peer.fingerprint.short()),
        kind: peer.kind,
        ..PairingState::of("paired")
    })
}

/// Unix seconds. Pairing is bounded by wall-clock time on both sides, which is
/// the one place in this system a clock is load-bearing — an invite has to
/// expire whether or not anybody is looking at it.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The headline, from the daemon's live state.
///
/// Read from the watch channel rather than computed, because "is a device
/// reachable" is true for as long as a connection is open and false a moment
/// later. Nothing in the index can answer it.
#[tauri::command(async)]
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
        incoming: status.incoming.into_iter().map(in_flight).collect(),
        outgoing: status.outgoing.into_iter().map(in_flight).collect(),
    })
}

#[tauri::command(async)]
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
        free_disk: big(qurb_cli::setup::inspect(&hosted.root()).free),
        over: numbers.over(),
        file_count: numbers.file_count,
        evicted: numbers.evicted,
        only_here: numbers.only_here,
    })
}

/// What Qurb keeps on this computer, by kind, for Storage (the owner's item 9
/// of 2026-10-14). By the size of the files: on disk they take less, after
/// compression and what they share, which `storage` gives.
#[derive(Serialize)]
pub struct StorageParts {
    phones: Vec<PhoneParts>,
    guests: Vec<GuestPart>,
    /// The shared folder: how many files, and how much of them is here.
    shared_files: usize,
    shared_here: String,
    /// Recently deleted, kept thirty days.
    deleted_files: usize,
    deleted: String,
}

/// One of the owner's phones, and what is kept here for it.
#[derive(Serialize)]
pub struct PhoneParts {
    name: String,
    whatsapp_files: u64,
    whatsapp: String,
    folders_files: u64,
    folders: String,
    vault_files: u64,
    vault: String,
}

#[derive(Serialize)]
pub struct GuestPart {
    name: String,
    size: String,
}

#[tauri::command(async)]
pub fn storage_parts(hosted: Host<'_>) -> Answer<StorageParts> {
    let phones = owner_phones(&hosted)?;
    let mut parts = Vec::new();
    for (name, phone) in phones {
        let (all_files, all) = hosted.with_store(|s| Ok(s.db().held_for(&phone.id)?)).map_err(failed)?;
        if all_files == 0 {
            continue;
        }
        let outside = |prefix: &str| {
            hosted
                .with_store(|s| Ok(s.db().held_outside_for(&phone.id, &format!("{}/{prefix}", qurb_storage::db::OUTSIDE))?))
                .map_err(failed)
        };
        let (whatsapp_files, whatsapp) = outside("whatsapp")?;
        let (folders_files, folders) = outside("storage")?;
        parts.push(PhoneParts {
            name,
            whatsapp_files,
            whatsapp: big(whatsapp),
            folders_files,
            folders: big(folders),
            vault_files: all_files.saturating_sub(whatsapp_files + folders_files),
            vault: big(all.saturating_sub(whatsapp + folders)),
        });
    }
    let guests = everyone(&hosted)?
        .into_iter()
        .filter(|p| p.relation == "guest")
        .map(|p| GuestPart { name: p.name, size: p.size })
        .collect();
    let numbers = hosted.with_store(|s| Ok(View::new(s, 0).storage()?)).map_err(failed)?;
    let deleted = hosted.with_store(|s| Ok(s.recently_deleted()?)).map_err(failed)?;
    Ok(StorageParts {
        phones: parts,
        guests,
        shared_files: numbers.file_count,
        shared_here: big(numbers.files),
        deleted_files: deleted.len(),
        deleted: big(deleted.iter().map(|d| d.size).sum()),
    })
}

/// One file as the file browser draws it (docs/design/brief.md §3).
#[derive(Serialize)]
pub struct Entry {
    path: String,
    size: String,
    /// Unix seconds, when the file was last changed.
    modified: i64,
    /// "here", "elsewhere", "only here" or "nowhere" -- decision 0032's three,
    /// which the window words as *On this device*, *Available elsewhere* and
    /// *Only copy here*, and decision 0055's *On no device*.
    availability: &'static str,
    /// In this device's own vault rather than the shared area.
    private: bool,
}

fn as_entry(e: qurb_storage::db::FolderEntry) -> Entry {
    Entry {
        path: e.path,
        size: big(e.size),
        modified: e.mtime_ns.div_euclid(1_000_000_000),
        availability: match e.availability {
            Availability::Here => "here",
            Availability::Elsewhere => "elsewhere",
            Availability::OnlyHere => "only here",
            Availability::Nowhere => "nowhere",
        },
        private: e.private,
    }
}

/// One folder of the synced folder: the folders directly inside it, then the
/// files, as a file browser shows them.
#[derive(Serialize)]
pub struct Directory {
    folders: Vec<String>,
    files: Vec<Entry>,
}

/// A folder's contents, in the shared area or this device's Private Vault.
///
/// From the index, not the disk, so a file freed from here is still one of
/// its files -- which is the point of *Available elsewhere*. The index is
/// flat, so a folder is what the paths beneath it say it is. The same query
/// the phone's Files screen uses.
#[tauri::command(async)]
pub fn browse(hosted: Host<'_>, dir: String, private: bool) -> Answer<Directory> {
    let dir = dir.trim_matches('/').to_string();
    let entries = hosted
        .with_store(|store| Ok(store.db().folder_entries_under(&dir)?))
        .map_err(failed)?;
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let mut folders = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    for entry in entries.into_iter().filter(|e| e.private == private) {
        let Some(rest) = entry.path.strip_prefix(&prefix) else { continue };
        match rest.split_once('/') {
            Some((folder, _)) => {
                folders.insert(folder.to_string());
            }
            None => files.push(as_entry(entry)),
        }
    }
    Ok(Directory { folders: folders.into_iter().collect(), files })
}

/// Files in the shared area or the Private Vault whose path contains `text`.
#[tauri::command(async)]
pub fn find(hosted: Host<'_>, text: String, private: bool) -> Answer<Vec<Entry>> {
    // As `search`: an empty box is somebody who has not typed yet.
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let found = hosted
        .with_store(|store| Ok(store.db().folder_search(text.trim(), 300)?))
        .map_err(failed)?;
    Ok(found.into_iter().filter(|e| e.private == private).take(200).map(as_entry).collect())
}

/// Everything the details panel shows about one file (brief §19).
#[derive(Serialize)]
pub struct Details {
    file: Entry,
    /// The other devices holding it that would hand it back, by name.
    holders: Vec<String>,
    /// What happened to it, newest first.
    history: Vec<Happened>,
}

#[tauri::command(async)]
pub fn details(hosted: Host<'_>, path: String) -> Answer<Details> {
    hosted
        .with_store(|store| {
            let db = store.db();
            let file = db.folder_entry(&path)?.context("that file is not in qurb any more")?;
            let (row, _) = db.folder_row(&path)?.context("that file is not in qurb any more")?;
            let named = db.device_names()?;
            let name = |id: &qurb_sync::DeviceId| named.get(id).cloned().unwrap_or_else(|| id.short());
            let holders = db.holders_of_content(&row.content_hash)?.iter().map(name).collect();
            let history = qurb_cli::View::new(store, 0)
                .history_of(&path, 8)?
                .into_iter()
                .map(|r| Happened {
                    id: r.id,
                    at: r.at,
                    kind: r.kind.as_str().to_string(),
                    path: r.path,
                    size: r.size.map(big),
                    // Unnamed when never paired here, as in Activity.
                    device: r.device.and_then(|id| named.get(&id).cloned()),
                    detail: r.detail,
                })
                .collect();
            Ok(Details { file: as_entry(file), holders, history })
        })
        .map_err(failed)
}

/// *Free local space* for one file (brief §12): its bytes leave this device,
/// the file stays in qurb, and *Keep on this device* brings it back. Refused
/// for the only copy, by the store, whatever the window thought.
#[tauri::command(async)]
pub fn free_file(hosted: Host<'_>, path: String) -> Answer<String> {
    hosted
        .with_store_mut(|store| Ok(store.free_local(&path)?))
        .map(big)
        .map_err(failed)
}

/// Move a file into this computer's Private Vault, or out of it to every
/// device (decision 0057). Returns whether it moved. Refused, by the store,
/// for a file whose bytes are not here.
#[tauri::command(async)]
pub fn move_file_area(hosted: Host<'_>, path: String, private: bool) -> Answer<bool> {
    hosted.with_store_mut(|store| Ok(store.move_area(&path, private)?)).map_err(failed)
}

/// The devices keeping this computer's Private Vault, by name: what moving a
/// file into it says keeps a copy.
#[tauri::command(async)]
pub fn vault_keepers(hosted: Host<'_>) -> Answer<Vec<String>> {
    hosted
        .with_store(|store| {
            let db = store.db();
            let named = db.device_names()?;
            Ok(db.holders()?.iter().map(|id| named.get(id).cloned().unwrap_or_else(|| id.short())).collect())
        })
        .map_err(failed)
}

/// What could be freed without losing anything, and the largest files that
/// would free it (brief §20).
#[derive(Serialize)]
pub struct Freeable {
    count: usize,
    bytes: String,
    files: Vec<Entry>,
}

#[tauri::command(async)]
pub fn freeable(hosted: Host<'_>) -> Answer<Freeable> {
    let found = hosted.with_store(|store| Ok(store.db().freeable(50)?)).map_err(failed)?;
    Ok(Freeable {
        count: found.count,
        bytes: big(found.bytes),
        files: found.files.into_iter().map(as_entry).collect(),
    })
}

/// Delete a file from qurb, on every device, keeping it in Recently deleted
/// for thirty days (decision 0042).
#[tauri::command(async)]
pub fn delete_file(hosted: Host<'_>, path: String) -> Answer<()> {
    hosted
        .with_store_mut(|store| {
            store.delete_to_trash(&path, None)?;
            Ok(())
        })
        .map_err(failed)
}

/// The file on disk behind a path the page names, if it is one qurb has and
/// its bytes are here. Checked, because the page names it: a path that
/// climbs out of the folder, or into qurb's own store, is refused.
fn on_disk(hosted: &Hosted, path: &str) -> Answer<std::path::PathBuf> {
    if !qurb_sync::is_safe_path(path) {
        return Err("that is not a file in qurb".to_string());
    }
    let here = hosted
        .with_store(|store| Ok(store.db().folder_entry(path)?))
        .map_err(failed)?
        .ok_or("that file is not in qurb any more")?;
    match here.availability {
        Availability::Elsewhere => {
            return Err("that file is not on this device — keep it here first".to_string())
        }
        Availability::Nowhere => {
            return Err("no device this one syncs with has that file any more".to_string())
        }
        Availability::Here | Availability::OnlyHere => {}
    }
    Ok(hosted.root().join(path))
}

/// A look at one version of a file with two (brief §2): an image, or the
/// start of a text, read from the folder. `kind` is "image" (`data` a data
/// URL, which the window's content policy allows), "text", or "none" -- for a
/// version not here, an image too large to carry over, or neither.
#[derive(Serialize)]
pub struct Preview {
    kind: &'static str,
    data: String,
}

/// Past this an image is not previewed: it would cross to the page as a
/// string a third larger than the file.
const PREVIEW_IMAGE_MAX: u64 = 6 << 20;
/// What of a text is shown: the start, which is where two versions of a note
/// or a list usually differ enough to tell them apart.
const PREVIEW_TEXT_BYTES: usize = 2048;

///
/// With `small`, for a gallery, a camera's JPEG is answered with the
/// thumbnail it carries, read from the file's first bytes, when it has one.
#[tauri::command(async)]
pub fn preview(hosted: Host<'_>, path: String, small: Option<bool>) -> Answer<Preview> {
    let none = Preview { kind: "none", data: String::new() };
    let Ok(file) = on_disk(&hosted, &path) else { return Ok(none) };
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if small == Some(true) && matches!(ext.as_str(), "jpg" | "jpeg") {
        use std::io::Read as _;
        let mut start = Vec::with_capacity(THUMBNAIL_FROM);
        let read = std::fs::File::open(&file).and_then(|f| f.take(THUMBNAIL_FROM as u64).read_to_end(&mut start));
        if let Some(thumb) = read.ok().and_then(|_| exif_thumbnail(&start)) {
            return Ok(Preview { kind: "image", data: format!("data:image/jpeg;base64,{}", base64(thumb)) });
        }
    }
    if let Some(mime) = image_type(&ext) {
        let small = std::fs::metadata(&file).map(|m| m.len() <= PREVIEW_IMAGE_MAX).unwrap_or(false);
        return Ok(match small.then(|| std::fs::read(&file).ok()).flatten() {
            Some(bytes) => Preview { kind: "image", data: format!("data:{mime};base64,{}", base64(&bytes)) },
            None => none,
        });
    }
    use std::io::Read as _;
    let mut start = Vec::with_capacity(PREVIEW_TEXT_BYTES);
    let read = std::fs::File::open(&file)
        .and_then(|f| f.take(PREVIEW_TEXT_BYTES as u64).read_to_end(&mut start));
    if read.is_err() || start.contains(&0) {
        return Ok(none);
    }
    // Text if it is UTF-8, allowing a character cut off at the end.
    let text = match std::str::from_utf8(&start) {
        Ok(text) => text,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&start[..e.valid_up_to()]).unwrap_or(""),
        Err(_) => return Ok(none),
    };
    Ok(Preview { kind: "text", data: text.to_string() })
}

/// Standard base64, for a data URL: small enough to keep here rather than
/// take a dependency for.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = match *chunk {
            [a, b, c] => (a as u32) << 16 | (b as u32) << 8 | c as u32,
            [a, b] => (a as u32) << 16 | (b as u32) << 8,
            [a] => (a as u32) << 16,
            _ => unreachable!("chunks of at most three"),
        };
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Where quick preview shows a video, a song or a PDF from (the redesign's
/// item 11): an address on this computer only, at a token nobody else knows.
#[derive(Serialize)]
pub struct Peek {
    /// "video", "audio" or "pdf".
    kind: &'static str,
    url: String,
}

/// What quick preview plays or shows, by name: the kinds the window's own
/// player and PDF viewer take.
fn peek_type(name: &str) -> Option<(&'static str, &'static str)> {
    if name.to_ascii_lowercase().ends_with(".pdf") {
        return Some(("pdf", "application/pdf"));
    }
    let kind = crate::stream::media_type(name)?;
    Some((if kind.starts_with("video/") { "video" } else { "audio" }, kind))
}

/// A video, song or PDF of this computer's folder, for quick preview.
#[tauri::command(async)]
pub fn peek(hosted: Host<'_>, path: String) -> Answer<Peek> {
    let file = on_disk(&hosted, &path)?;
    let (kind, mime) = peek_type(&path).ok_or("quick preview shows pictures, text, video, songs and PDFs")?;
    Ok(Peek { kind, url: crate::stream::serve_disk(&file, mime).map_err(failed)? })
}

/// The same, for a file of one of the owner's phones kept here, while their
/// folder is open: served from the store, with no copy anywhere.
#[tauri::command(async)]
pub fn peek_kept(hosted: Host<'_>, device: String, path: String) -> Answer<Peek> {
    let id = person_of(&hosted, OWNER)?;
    let (kind, mime) = peek_type(&path).ok_or("quick preview shows pictures, text, video, songs and PDFs")?;
    qurb_peer::openings::key(&id).ok_or("the folder is locked")?;
    let phone = own_phone(&hosted, &device)?;
    if !qurb_sync::is_safe_path(&path) {
        return Err("that is not a file of the folder".to_string());
    }
    let row = hosted
        .with_store(|store| Ok(store.db().live_row_in(&path, Some(&phone.id))?))
        .map_err(failed)?
        .ok_or("that file is not in the folder")?;
    let name = std::path::Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    let (dir, key) = hosted.with_store(|store| Ok((store.root().to_path_buf(), store.chunk_key()))).map_err(failed)?;
    let url = crate::stream::serve(&dir, key, id, &row.content_hash, &name, mime).map_err(failed)?;
    Ok(Peek { kind, url })
}

/// Open a file with the program the desktop uses for it.
#[tauri::command(async)]
pub fn open_file(hosted: Host<'_>, path: String) -> Answer<()> {
    reveal(&on_disk(&hosted, &path)?)
}

// ------------------------------------------------- a phone's folders (0063)

/// One of a phone's folders as it last said, whether it is chosen to be
/// backed up here, and how much of it this computer keeps.
#[derive(Serialize)]
pub struct PhoneFolder {
    path: String,
    files: u64,
    size: String,
    /// Its own files backed up: the deepest choice at or above it
    /// (`qurb_storage::chosen`).
    chosen: bool,
    /// Some of what is inside it chosen otherwise than it is.
    partial: bool,
    kept_files: u64,
    kept: String,
}

/// A phone's folders, for choosing which it backs up here (decision 0063).
#[derive(Serialize)]
pub struct PhoneFolders {
    /// When the phone last said what folders it has. Absent when it never
    /// has: it has not been given "All files access", or does not keep its
    /// Private Vault here, or runs a build from before.
    seen_at: Option<i64>,
    folders: Vec<PhoneFolder>,
}

/// One of the person's own phones, never another person's device.
fn own_phone(hosted: &Host<'_>, phone: &str) -> Result<qurb_cli::Device, String> {
    let device = recipient(hosted, phone)?;
    if device.relation != qurb_storage::db::Relation::Own {
        return Err(format!("{} is another person's device", device.name));
    }
    Ok(device)
}

/// A phone's backups are the owner's folder's, opened only with a phone's
/// yes (decision 0067).
fn owner_folder_open(hosted: &Hosted) -> Result<(), String> {
    let owner = person_of(hosted, OWNER)?;
    match qurb_peer::openings::key(&owner) {
        Some(_) => Ok(()),
        None => Err("your folder is locked: open it in Files, and approve on your phone".to_string()),
    }
}

/// Where a phone's chosen folder is kept in its vault.
fn kept_prefix(folder: &str) -> String {
    format!("{}/storage/{folder}", qurb_storage::db::OUTSIDE)
}

#[tauri::command(async)]
pub fn phone_folders(hosted: Host<'_>, phone: String) -> Answer<PhoneFolders> {
    let device = own_phone(&hosted, &phone)?;
    hosted
        .with_store(|store| {
            let db = store.db();
            let (folders, seen_at) = db.known_folders(&device.id)?;
            // A phone says its folders at any depth since the tree of the
            // owner's list of 2026-10-24; this window still shows two levels
            // until the desktop's redesign is built.
            let listed: std::collections::HashSet<String> = folders.iter().map(|f| f.path.clone()).collect();
            let choices: Vec<_> =
                db.choices_for(&device.id)?.into_iter().filter(|c| listed.contains(&c.folder)).collect();
            // What is kept of every folder, from one pass over the phone's
            // kept files: once a folder, the same rows were scanned two dozen
            // times, every five seconds, on the window's thread (2026-10-28).
            let kept_files_all = db.held_outside_under(&device.id, &kept_prefix(""))?;
            let kept_of = |folder: &str| {
                let under = format!("{}/", kept_prefix(folder).trim_end_matches('/'));
                kept_files_all
                    .iter()
                    .filter(|(path, _)| path.starts_with(&under))
                    .fold((0u64, 0u64), |(n, bytes), (_, size)| (n + 1, bytes + size))
            };
            let folders = folders
                .into_iter()
                .filter(|f| f.path.matches('/').count() < 2)
                .map(|f| {
                    let (kept_files, kept) = kept_of(&f.path);
                    PhoneFolder {
                        chosen: qurb_storage::chosen::backed_up(&choices, &f.path),
                        partial: qurb_storage::chosen::tick(&choices, &f.path) == qurb_storage::chosen::Tick::Some,
                        size: big(f.bytes),
                        kept: big(kept),
                        path: f.path,
                        files: f.files,
                        kept_files,
                    }
                })
                .collect();
            Ok(PhoneFolders { seen_at, folders })
        })
        .map_err(failed)
}

/// Tick, or untick, a phone's folder. Only one the phone said it has: the
/// phone checks again what it is pointed at, and this keeps the list clean.
#[tauri::command(async)]
pub fn choose_phone_folder(hosted: Host<'_>, phone: String, folder: String, chosen: bool) -> Answer<()> {
    let device = own_phone(&hosted, &phone)?;
    hosted
        .with_store(|store| {
            let db = store.db();
            if chosen && !db.known_folders(&device.id)?.0.iter().any(|f| f.path == folder) {
                return Err(anyhow::anyhow!("{} has no folder {folder}", device.name));
            }
            Ok(db.choose_folder(&device.id, &folder, chosen)?)
        })
        .map_err(failed)
}

/// How much of a backed-up photo is read for its thumbnail: the start, where
/// a camera's JPEG carries one of its own (the EXIF thumbnail, inside the
/// first 64 KB by that standard's rule).
const THUMBNAIL_FROM: usize = 128 << 10;

/// A picture of one photo a phone backs up here, from the owner's folder as
/// `person_folder` lists it (`path` as it gives it). `small` for the gallery
/// (the redesign's item 11): the thumbnail a camera embeds in its JPEG, a few
/// kilobytes, so a page of photos crosses to the window in less than one
/// photo would. Otherwise, or for a picture with none -- a screenshot, say --
/// the picture itself, if it is small enough to preview at all.
#[tauri::command(async)]
pub fn backed_up_preview(hosted: Host<'_>, phone: String, path: String, small: bool) -> Answer<Preview> {
    let none = Preview { kind: "none", data: String::new() };
    owner_folder_open(&hosted)?;
    let device = own_phone(&hosted, &phone)?;
    if !qurb_sync::is_safe_path(&path) {
        return Err("that is not a file of the folder".to_string());
    }
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let Some(mime) = image_type(&ext) else { return Ok(none) };
    let read = |limit: usize| {
        let mut start = Start { bytes: Vec::new(), limit };
        // Stopped on purpose once `limit` is read: the error that stops it is
        // ours, and what was read is what was wanted.
        let whole = hosted.with_store(|store| Ok(store.read_held_into(&device.id, &path, &mut start)?)).is_ok();
        (start.bytes, whole)
    };
    let (start, whole) = read(THUMBNAIL_FROM);
    if small {
        if let Some(thumb) = exif_thumbnail(&start) {
            return Ok(Preview { kind: "image", data: format!("data:image/jpeg;base64,{}", base64(thumb)) });
        }
    }
    let bytes = if whole { start } else {
        let (all, whole) = read(PREVIEW_IMAGE_MAX as usize + 1);
        if !whole || all.len() > PREVIEW_IMAGE_MAX as usize { return Ok(none) }
        all
    };
    if bytes.is_empty() {
        return Ok(none);
    }
    Ok(Preview { kind: "image", data: format!("data:{mime};base64,{}", base64(&bytes)) })
}

/// The image types a webview shows, by extension.
fn image_type(ext: &str) -> Option<&'static str> {
    match ext {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

/// The start of a file, up to a limit, and then a stop: a write that fails
/// is how the reader is told no more is wanted.
struct Start {
    bytes: Vec<u8>,
    limit: usize,
}

impl std::io::Write for Start {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let room = self.limit - self.bytes.len();
        if room == 0 {
            return Err(std::io::Error::other("read as far as wanted"));
        }
        let n = buf.len().min(room);
        self.bytes.extend_from_slice(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The thumbnail a JPEG carries in its EXIF block, if it has one: IFD1's
/// JPEGInterchangeFormat and its length, inside the APP1 segment.
fn exif_thumbnail(jpeg: &[u8]) -> Option<&[u8]> {
    if jpeg.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut at = 2;
    while at + 4 <= jpeg.len() {
        if jpeg[at] != 0xFF {
            return None;
        }
        let marker = jpeg[at + 1];
        // The image itself begins: there was no EXIF block before it.
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        let body = jpeg.get(at + 4..at + 2 + len)?;
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            return thumbnail_in(&body[6..]);
        }
        at += 2 + len;
    }
    None
}

/// IFD1's thumbnail in a TIFF block, either byte order.
fn thumbnail_in(tiff: &[u8]) -> Option<&[u8]> {
    let little = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<usize> {
        let b: [u8; 2] = tiff.get(i..i + 2)?.try_into().ok()?;
        Some(if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) } as usize)
    };
    let u32_at = |i: usize| -> Option<usize> {
        let b: [u8; 4] = tiff.get(i..i + 4)?.try_into().ok()?;
        Some(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) } as usize)
    };
    let ifd0 = u32_at(4)?;
    let ifd1 = u32_at(ifd0 + 2 + u16_at(ifd0)? * 12)?;
    if ifd1 == 0 {
        return None;
    }
    let (mut offset, mut length) = (None, None);
    for e in 0..u16_at(ifd1)? {
        let entry = ifd1 + 2 + e * 12;
        match u16_at(entry)? {
            0x0201 => offset = Some(u32_at(entry + 8)?),
            0x0202 => length = Some(u32_at(entry + 8)?),
            _ => {}
        }
    }
    let (offset, length) = (offset?, length?);
    let thumb = tiff.get(offset..offset.checked_add(length)?)?;
    thumb.starts_with(&[0xFF, 0xD8]).then_some(thumb)
}

// ------------------------------------------------- a guest's folder, opened

/// Where files opened from a guest's folder are unsealed: under the runtime
/// directory, which is memory-backed on a systemd desktop, so they never
/// reach the disk, and removed when the folder is locked.
fn opened_dir(person: &qurb_sync::DeviceId) -> std::path::PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("qurb-open").join(person.short())
}

// ---------------------------------------------- a folder for each person

/// One person this computer keeps files for (decision 0067): its owner, with
/// the owner's phones, and each guest.
#[derive(Serialize)]
pub struct Person {
    /// `owner`, or a guest's person in hex: what the folder's commands take.
    id: String,
    name: String,
    /// "owner" or "guest".
    relation: &'static str,
    /// "locked", "asking" or "open".
    state: &'static str,
    /// A guest's folder is sealed: nothing here can read it without them.
    /// The owner's is not, and the window says so.
    sealed: bool,
    /// Files kept for them, where that can be counted without opening.
    files: Option<u64>,
    size: String,
    /// Their phones, by the names the folder shows them under.
    devices: Vec<String>,
    /// How many of those are phones in use: one set up again is one phone,
    /// though it keeps a folder for what was kept before.
    phones: usize,
}

/// A person's folder, at one place in it, while it is open.
#[derive(Serialize)]
pub struct PersonFolder {
    state: &'static str,
    folders: Vec<String>,
    files: Vec<PersonFile>,
}

#[derive(Serialize)]
pub struct PersonFile {
    /// Its name, as the folder shows it.
    name: String,
    /// Where it is kept: what `open_person_file` takes, with `device`.
    path: String,
    /// The phone it is kept for, as its fingerprint; none for a guest's.
    device: Option<String>,
    size: String,
    modified_at: i64,
}

/// The owner's folder is asked for as this, rather than by an identifier.
const OWNER: &str = "owner";

/// What a person's folder is filed under: the owner's under this computer's
/// own device, a guest's under their person (decision 0060).
fn person_of(hosted: &Hosted, id: &str) -> Result<qurb_sync::DeviceId, String> {
    if id == OWNER {
        return hosted.with_store(|store| Ok(store.device_id()?)).map_err(failed);
    }
    qurb_sync::DeviceId::from_hex(id).ok_or_else(|| "no such person".to_string())
}

/// The owner's name for their folder: the first word of the login's full
/// name, else the login, capitalised.
fn owner_name() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    let full = std::fs::read_to_string("/etc/passwd").ok().and_then(|passwd| {
        passwd
            .lines()
            .find(|line| line.split(':').next() == Some(user.as_str()))
            .and_then(|line| line.split(':').nth(4))
            .and_then(|gecos| gecos.split(',').next())
            .and_then(|name| name.split_whitespace().next())
            .map(str::to_string)
    });
    let name = full.unwrap_or(user);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "You".to_string(),
    }
}

/// The owner's phones whose files are kept here, each under the name its
/// folder shows: one set up again keeps its own folder, as *earlier*.
fn owner_phones(hosted: &Hosted) -> Result<Vec<(String, qurb_cli::Device)>, String> {
    let devices = hosted.with_store(|store| Ok(View::new(store, 0).devices()?)).map_err(failed)?;
    // The owner's phones, and any of their computers this one keeps files
    // for: a computer backing up its own folders here (the owner's list of
    // 2026-10-28) is shown as a phone's backups are.
    let phones: Vec<qurb_cli::Device> = devices
        .into_iter()
        .filter(|d| d.relation == qurb_storage::db::Relation::Own)
        .filter(|d| d.kind.as_deref() == Some("phone") || d.held.is_some())
        .filter(|d| d.held.is_some() || d.replaced_by.is_none())
        .collect();
    // Phones in use first, then what was kept for them before, as the phone
    // lists its own (`qurb_storage::person::own_phones`).
    let mut phones = phones;
    phones.sort_by_key(|d| d.replaced_by.is_some());
    let mut named = Vec::new();
    for phone in phones {
        let mut name = if phone.replaced_by.is_some() { format!("{}, earlier", phone.name) } else { phone.name.clone() };
        let mut n = 2;
        while named.iter().any(|(taken, _): &(String, qurb_cli::Device)| *taken == name) {
            name = format!("{} ({n})", phone.name);
            n += 1;
        }
        named.push((name, phone));
    }
    Ok(named)
}

/// How a person's folder stands, without using it: a look does not keep it
/// open. Locked, what was opened from it goes.
fn folder_state(person: &qurb_sync::DeviceId) -> &'static str {
    if qurb_peer::openings::is_open(person) {
        return "open";
    }
    let _ = std::fs::remove_dir_all(opened_dir(person));
    if qurb_peer::openings::asking(person).is_some() { "asking" } else { "locked" }
}

/// Everyone this computer keeps files for, the owner first.
#[tauri::command(async)]
pub fn people(hosted: Host<'_>) -> Answer<Vec<Person>> {
    let people = everyone(&hosted)?;
    lay_out(&hosted, &people);
    Ok(people)
}

/// A placeholder in `~/qurb` for each person this computer keeps files for,
/// when the folder is laid out by person (decision 0067). Best effort: a
/// placeholder that cannot be written changes nothing that is kept.
pub fn place_people(hosted: &Hosted) {
    if let Ok(people) = everyone(hosted) {
        lay_out(hosted, &people);
    }
}

fn lay_out(hosted: &Hosted, people: &[Person]) {
    let Some(dir) = qurb_cli::layout::people_dir(&hosted.root()) else { return };
    let notes: Vec<(String, String)> = people
        .iter()
        .map(|p| {
            let note = match p.relation {
                "owner" => format!(
                    "{name}'s folder\n\nWhat this computer keeps for {name}'s phones -- their backups, their \
                     WhatsApp and their Private Vault -- is kept by Qurb, encrypted, and opens only inside Qurb, \
                     with a yes on {name}'s phone.\n\nOpen Qurb, choose Files, then {name}.\n",
                    name = p.name
                ),
                _ => format!(
                    "{name}'s folder\n\nKept sealed by Qurb: this computer cannot read it. It opens only inside \
                     Qurb, when {name} approves on their phone.\n\nOpen Qurb, choose Files, then {name}.\n",
                    name = p.name
                ),
            };
            (p.name.clone(), note)
        })
        .collect();
    if let Err(e) = qurb_cli::layout::place_people(&dir, &notes) {
        tracing::warn!(error = %format!("{e:#}"), "could not lay out people's folders");
    }
}

fn everyone(hosted: &Hosted) -> Answer<Vec<Person>> {
    let owner = person_of(hosted, OWNER)?;
    let phones = owner_phones(hosted)?;
    let (files, bytes) = phones.iter().fold((0, 0), |(f, b), (_, p)| {
        let (pf, pb) = p.held.unwrap_or((0, 0));
        (f + pf, b + pb)
    });
    let mut people = vec![Person {
        id: OWNER.to_string(),
        name: owner_name(),
        relation: "owner",
        state: folder_state(&owner),
        sealed: false,
        files: Some(files),
        size: big(bytes),
        phones: phones.iter().filter(|(_, p)| p.replaced_by.is_none()).count(),
        devices: phones.into_iter().map(|(name, _)| name).collect(),
    }];

    // A guest's devices, gathered under their person.
    let devices = hosted.with_store(|store| Ok(View::new(store, 0).devices()?)).map_err(failed)?;
    let mut guests: Vec<(qurb_sync::DeviceId, Vec<qurb_cli::Device>)> = Vec::new();
    for device in devices.into_iter().filter(|d| d.relation == qurb_storage::db::Relation::Guest) {
        let person = hosted
            .with_store(|store| Ok(store.db().person_of(&device.id)?.unwrap_or(device.id)))
            .map_err(failed)?;
        match guests.iter_mut().find(|(p, _)| *p == person) {
            Some((_, theirs)) => theirs.push(device),
            None => guests.push((person, vec![device])),
        }
    }
    for (person, mut theirs) in guests {
        theirs.sort_by_key(|d| d.paired_at);
        let (count, kept) = hosted
            .with_store(|store| Ok((store.db().kept_names(&person)?.len() as u64, store.db().kept_for_person(&person)?)))
            .map_err(failed)?;
        people.push(Person {
            id: person.to_hex(),
            name: theirs[0].name.clone(),
            relation: "guest",
            state: folder_state(&person),
            sealed: true,
            files: Some(count),
            size: big(kept),
            phones: theirs.len(),
            devices: theirs.into_iter().map(|d| d.name).collect(),
        });
    }
    Ok(people)
}

/// A person's folder at `dir`: its folders and files while it is open. The
/// owner's lists each phone, then what is kept for it; a guest's, sealed, is
/// one folder of theirs. Listing counts as using the folder.
#[tauri::command(async)]
pub fn person_folder(hosted: Host<'_>, person: String, dir: String) -> Answer<PersonFolder> {
    let id = person_of(&hosted, &person)?;
    let state = folder_state(&id);
    if state != "open" {
        return Ok(PersonFolder { state, folders: Vec::new(), files: Vec::new() });
    }
    let key = qurb_peer::openings::key(&id).ok_or("the folder has just locked itself")?;
    let dir = dir.trim_matches('/');

    if person == OWNER {
        // One folder per phone, each phone's files as the phone sees them
        // too (`qurb_storage::person`).
        let phones = owner_phones(&hosted)?;
        let named: Vec<(String, qurb_sync::DeviceId)> = phones.iter().map(|(name, d)| (name.clone(), d.id)).collect();
        let (folders, shown) = hosted
            .with_store(|store| Ok(qurb_storage::person::listing(store.db(), &named, dir)?))
            .map_err(failed)?;
        let files = shown
            .into_iter()
            .map(|f| PersonFile {
                name: f.name,
                path: f.path,
                device: phones.iter().find(|(_, d)| d.id == f.owner).map(|(_, d)| d.fingerprint.clone()),
                size: big(f.size),
                modified_at: f.modified_at,
            })
            .collect();
        return Ok(PersonFolder { state, folders, files });
    }

    // A guest's, sealed: one folder of theirs, opened with their key.
    let key = qurb_storage::sealed::FolderKey::from_bytes(key);
    let kept = hosted.with_store(|store| Ok(store.open_kept(&id, &key)?)).map_err(failed)?;
    let mut folders = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    for f in kept {
        let rest = match dir {
            "" => f.path.as_str(),
            _ => match f.path.strip_prefix(dir).and_then(|r| r.strip_prefix('/')) {
                Some(rest) => rest,
                None => continue,
            },
        };
        match rest.split_once('/') {
            Some((folder, _)) => {
                folders.insert(folder.to_string());
            }
            None => files.push(PersonFile {
                name: rest.to_string(),
                path: f.path.clone(),
                device: None,
                size: big(f.size),
                modified_at: f.modified_at,
            }),
        }
    }
    Ok(PersonFolder { state, folders: folders.into_iter().collect(), files })
}

/// A file found in a person's folder: whose, and where in it.
#[derive(Serialize)]
pub struct FoundKept {
    /// The person, as `person_folder` takes them.
    person: String,
    #[serde(flatten)]
    file: PersonFile,
}

/// The most files a search of people's folders answers with.
const FOUND_MOST: usize = 500;

/// Search every person's folder that is open now -- the owner's phones'
/// backups, a guest's sealed folder while they have opened it -- for files
/// whose place holds `text`, ignoring case. A locked folder is not searched:
/// its names are as much its own as its files.
#[tauri::command(async)]
pub fn find_people(hosted: Host<'_>, text: String) -> Answer<Vec<FoundKept>> {
    let needle = text.trim().to_lowercase();
    let mut found = Vec::new();
    if needle.is_empty() {
        return Ok(found);
    }
    for person in everyone(&hosted)? {
        let id = person_of(&hosted, &person.id)?;
        let Some(key) = qurb_peer::openings::key(&id) else { continue };
        if person.id == OWNER {
            let phones = owner_phones(&hosted)?;
            let named: Vec<(String, qurb_sync::DeviceId)> = phones.iter().map(|(name, d)| (name.clone(), d.id)).collect();
            let shown = hosted
                .with_store(|store| Ok(qurb_storage::person::search(store.db(), &named, &needle, FOUND_MOST)?))
                .map_err(failed)?;
            found.extend(shown.into_iter().map(|f| FoundKept {
                person: OWNER.to_string(),
                file: PersonFile {
                    name: f.name,
                    path: f.path,
                    device: phones.iter().find(|(_, d)| d.id == f.owner).map(|(_, d)| d.fingerprint.clone()),
                    size: big(f.size),
                    modified_at: f.modified_at,
                },
            }));
        } else {
            let key = qurb_storage::sealed::FolderKey::from_bytes(key);
            let kept = hosted.with_store(|store| Ok(store.open_kept(&id, &key)?)).map_err(failed)?;
            found.extend(kept.into_iter().filter(|f| f.path.to_lowercase().contains(&needle)).map(|f| FoundKept {
                person: person.id.clone(),
                file: PersonFile {
                    name: f.path.clone(),
                    path: f.path,
                    device: None,
                    size: big(f.size),
                    modified_at: f.modified_at,
                },
            }));
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.file.modified_at));
    found.truncate(FOUND_MOST);
    Ok(found)
}

/// Ask the person's phone to open their folder here. They approve on it,
/// behind its screen lock, at its next sync with this computer.
#[tauri::command(async)]
pub fn ask_person_folder(hosted: Host<'_>, person: String) -> Answer<()> {
    let id = person_of(&hosted, &person)?;
    qurb_peer::openings::ask(&id);
    hosted.sync_now();
    Ok(())
}

/// Lock a person's folder: forgotten until asked again, and what was opened
/// from it removed.
#[tauri::command(async)]
pub fn lock_person_folder(hosted: Host<'_>, person: String) -> Answer<()> {
    let id = person_of(&hosted, &person)?;
    qurb_peer::openings::lock(&id);
    crate::stream::forget(&id);
    let _ = std::fs::remove_dir_all(opened_dir(&id));
    Ok(())
}

/// Open one file of an open folder: written out, checked, into memory-backed
/// space, and handed to the desktop's program for it.
#[tauri::command(async)]
pub fn open_person_file(hosted: Host<'_>, person: String, device: Option<String>, path: String) -> Answer<()> {
    let id = person_of(&hosted, &person)?;
    let key = qurb_peer::openings::key(&id).ok_or("the folder is locked")?;
    if person != OWNER {
        return open_sealed(&hosted, &id, key, &path);
    }
    let phone = device.ok_or("which phone's file?")?;
    let phone = own_phone(&hosted, &phone)?;
    if !qurb_sync::is_safe_path(&path) {
        return Err("that is not a file of the folder".to_string());
    }
    let row = hosted
        .with_store(|store| Ok(store.db().live_row_in(&path, Some(&phone.id))?))
        .map_err(failed)?
        .ok_or("that file is not in the folder")?;
    let name = std::path::Path::new(&path).file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();

    // A large video or song is played from the store, a range at a time,
    // with no copy anywhere (stream.rs): written out, a 1.6 GB episode filled
    // the memory-backed space it was written to (2026-10-15).
    if let Some(kind) = crate::stream::media_type(&name) {
        if row.size > STREAM_FROM {
            let (dir, key) = hosted.with_store(|store| Ok((store.root().to_path_buf(), store.chunk_key()))).map_err(failed)?;
            let url = crate::stream::serve(&dir, key, id, &row.content_hash, &name, kind).map_err(failed)?;
            return crate::stream::play(&url, kind).map_err(failed);
        }
    }

    // Anything else is written into memory-backed space -- only if it fits
    // there with room to spare, since the desktop's own sessions keep their
    // sockets in it -- under its own path, which `is_safe_path` keeps
    // inside: two files of one name in different folders are two files.
    let free = free_space(&opened_dir(&id));
    if row.size.saturating_add(KEEP_FREE) > free {
        return Err(format!(
            "{name} is {}, more than this computer can open in memory ({} free). Open it on the phone it came from.",
            big(row.size),
            big(free.saturating_sub(KEEP_FREE.min(free))),
        ));
    }
    let to = opened_dir(&id).join(phone.id.short()).join(&path);
    std::fs::create_dir_all(to.parent().ok_or("that file has no folder")?).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(opened_dir(&id), std::fs::Permissions::from_mode(0o700));
    }
    let mut out = std::fs::File::create(&to).map_err(|e| e.to_string())?;
    let written = hosted.with_store(|store| Ok(store.read_held_into(&phone.id, &path, &mut out)?));
    if let Err(e) = written {
        // Not left half written: it holds memory until the folder locks.
        drop(out);
        let _ = std::fs::remove_file(&to);
        return Err(failed(e));
    }
    reveal(&to)
}

/// Media larger than this is streamed rather than written out.
const STREAM_FROM: u64 = 64 << 20;

/// What opening a file must leave free in the memory-backed space.
const KEEP_FREE: u64 = 256 << 20;

/// The bytes free on the filesystem holding `dir`, or what holds it: the
/// runtime directory's, a slice of memory.
fn free_space(dir: &std::path::Path) -> u64 {
    qurb_cli::disk::available(dir)
}

/// A guest's file, unsealed into memory-backed space, and opened.
fn open_sealed(hosted: &Host<'_>, person: &qurb_sync::DeviceId, key: [u8; 32], path: &str) -> Answer<()> {
    let key = qurb_storage::sealed::FolderKey::from_bytes(key);
    let sealed = hosted
        .with_store(|store| Ok(store.kept_file(person, &key, path)?))
        .map_err(failed)?
        .ok_or("that file is not in the folder")?;
    let name = std::path::Path::new(path).file_name().ok_or("that file has no name")?;
    let dir = opened_dir(person).join(&sealed.to_hex().as_str()[..12]);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(opened_dir(person), std::fs::Permissions::from_mode(0o700));
    }
    let to = dir.join(name);
    if let Err(e) = hosted.with_store(|store| Ok(store.unseal_kept(&key, &sealed, path, &to)?)) {
        // Not left half written in memory-backed space.
        let _ = std::fs::remove_file(&to);
        return Err(failed(e));
    }
    reveal(&to)
}

/// Open the folder a file is in, in the file manager.
#[tauri::command(async)]
pub fn show_file(hosted: Host<'_>, path: String) -> Answer<()> {
    let file = on_disk(&hosted, &path)?;
    reveal(file.parent().ok_or("that file has no folder")?)
}

/// Open the Qurb folder in the file manager: `~/qurb`, with *Shared* and
/// each person's placeholder in it, once laid out by person (decision 0067);
/// else the synced folder itself.
#[tauri::command(async)]
pub fn show_root(hosted: Host<'_>) -> Answer<()> {
    let root = hosted.root();
    reveal(&qurb_cli::layout::people_dir(&root).unwrap_or(root))
}

/// Whether files added here go to this device's Private Vault rather than
/// the shared area. The running daemon picks it up at its next pass.
#[tauri::command(async)]
pub fn set_privacy(hosted: Host<'_>, private: bool) -> Answer<()> {
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.own_files_private = private;
    config.save(&dir).map_err(failed)
}

/// Whether this desktop raises its three notifications.
#[tauri::command(async)]
pub fn set_notifications(hosted: Host<'_>, on: bool) -> Answer<()> {
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.notifications = on;
    config.save(&dir).map_err(failed)
}

#[tauri::command(async)]
pub fn devices(hosted: Host<'_>) -> Answer<Vec<Device>> {
    // Live, from the daemon: which devices it holds a connection to, and how.
    // The index knows who is paired and when each was last reached; only the
    // running daemon knows what is connected now.
    let links = hosted.status().map(|s| s.links).unwrap_or_default();
    let (devices, pictures) = hosted
        .with_store(|store| {
            let devices = View::new(store, 0).devices()?;
            let pictures: Vec<_> = devices
                .iter()
                .map(|d| store.db().peer_profile(&d.id).ok().flatten().and_then(|p| p.picture))
                .collect();
            Ok((devices, pictures))
        })
        .map_err(failed)?;
    Ok(devices
        .into_iter()
        .zip(pictures)
        .map(|(d, picture)| {
            let link = links.iter().find(|l| l.fingerprint == d.fingerprint);
            Device {
                picture: picture.as_deref().map(picture_address),
                id: d.id.short(),
                route: link.map(|l| if l.relayed { "relay" } else { "direct" }),
                address: link.map(|l| l.address.clone()),
                name: d.name,
                fingerprint: d.fingerprint,
                paired_at: d.paired_at,
                last_seen: d.last_seen,
                relation: d.relation.as_str(),
                kept: d.kept.map(big),
                kind: d.kind,
                whatsapp: d.whatsapp.map(|(files, bytes)| WhatsAppKept { files, bytes: big(bytes) }),
                held: d.held.map(|(files, bytes)| WhatsAppKept { files, bytes: big(bytes) }),
                replaced_by: d.replaced_by.map(|id| id.short()),
                earlier: d.earlier.map(|id| id.short()),
                said_same: d.said_same_as.is_some(),
            }
        })
        .collect())
}

#[tauri::command(async)]
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
                None => view.activity_told(limit.min(200), before)?,
            };
            Ok((rows, store.db().device_names()?))
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
            // A device since removed is named as it was when it was here. One
            // never paired here -- where a file began, reaching this one
            // through another -- has no name, and is left unnamed rather than
            // shown as a code: the window says "another device".
            device: r.device.and_then(|id| named.get(&id).cloned()),
            detail: r.detail,
        })
        .collect())
}

/// Files this computer made that no other device has yet, for Home's
/// *Waiting* (the owner's item 8 of 2026-10-14): the first few, and how many
/// in all.
#[derive(Serialize)]
pub struct NotElsewhere {
    first: Vec<Pending>,
    files: usize,
    size: String,
}

#[derive(Serialize)]
pub struct Pending {
    path: String,
    size: String,
}

/// How many of them Home names; the rest are counted.
const NOT_ELSEWHERE_NAMED: usize = 3;

#[tauri::command(async)]
pub fn not_elsewhere(hosted: Host<'_>) -> Answer<NotElsewhere> {
    // Counted, and the first few named, rather than the whole list built
    // to count it: Home asks every few seconds (2026-10-28).
    let ((files, size), first) = hosted
        .with_store(|store| Ok((store.db().undelivered_count()?, store.db().undelivered_first(NOT_ELSEWHERE_NAMED)?)))
        .map_err(failed)?;
    Ok(NotElsewhere {
        files: files as usize,
        size: big(size),
        first: first.into_iter().map(|(path, size)| Pending { path, size: big(size) }).collect(),
    })
}

/// *Sync now*: every device tried at once, and told there is news.
#[tauri::command(async)]
pub fn sync_now(hosted: Host<'_>) -> Answer<()> {
    hosted.sync_now();
    Ok(())
}

#[tauri::command(async)]
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
            to_id: o.to.short(),
        })
        .collect())
}

/// Take back a send the other device has not collected yet.
///
/// Refused once it has been: the file is theirs by then. See
/// [`qurb_storage::Store::cancel_send`].
#[tauri::command(async)]
pub fn cancel_send(hosted: Host<'_>, path: String, to: String) -> Answer<()> {
    let device = paired_device(&hosted, &to)?;
    hosted
        .with_store_mut(|store| Ok(store.cancel_send(&path, &device.id)?))
        .map_err(failed)
}

/// Folders and who they are shared with (decision 0044), and the devices a
/// folder could be shared with.
#[derive(Serialize)]
pub struct Sharing {
    folders: Vec<SharedFolder>,
    devices: Vec<ShareTarget>,
}

#[derive(Serialize)]
pub struct SharedFolder {
    folder: String,
    /// Shared with every device: no rule.
    everyone: bool,
    /// Device ids, in hex, when not everyone.
    members: Vec<String>,
    /// Kept on this computer only remotely (decision 0045).
    remote: bool,
}

#[derive(Serialize)]
pub struct ShareTarget {
    /// The device id, in hex: what a rule names.
    id: String,
    name: String,
    /// This computer.
    here: bool,
}

#[tauri::command(async)]
pub fn sharing(hosted: Host<'_>) -> Answer<Sharing> {
    hosted
        .with_store(|store| {
            let me = store.device_id()?;
            let mut devices = vec![ShareTarget { id: me.to_hex(), name: "This computer".into(), here: true }];
            for peer in store.db().trusted_peers()? {
                devices.push(ShareTarget { id: peer.device_id.to_hex(), name: peer.name, here: false });
            }
            let remote = store.db().remote_folders()?;
            let folders = store
                .folder_sharing()?
                .into_iter()
                .map(|(folder, members)| SharedFolder {
                    remote: remote.contains(&folder),
                    folder,
                    everyone: members.is_none(),
                    members: members.unwrap_or_default().iter().map(|d| d.to_hex()).collect(),
                })
                .collect();
            Ok(Sharing { folders, devices })
        })
        .map_err(failed)
}

/// Share a folder with exactly these devices (ids in hex), or with every
/// device when `members` is empty.
#[tauri::command(async)]
pub fn set_sharing(hosted: Host<'_>, folder: String, members: Vec<String>) -> Answer<()> {
    let members: std::collections::BTreeSet<qurb_sync::DeviceId> = members
        .iter()
        .map(|hex| qurb_sync::DeviceId::from_hex(hex).ok_or_else(|| format!("not a device: {hex}")))
        .collect::<Answer<_>>()?;
    hosted
        .with_store_mut(|store| {
            match members.is_empty() {
                true => store.clear_sharing(&folder)?,
                false => store.set_sharing(&folder, &members)?,
            }
            Ok(())
        })
        .map_err(failed)?;
    hosted.nudge();
    Ok(())
}

/// What keeping a folder remotely did.
#[derive(Serialize)]
pub struct KeptRemotely {
    freed: usize,
    bytes: String,
    /// Files kept because this computer has the only copy.
    kept: Vec<String>,
}

/// Keep a folder on this computer only remotely (decision 0045).
#[tauri::command(async)]
pub fn keep_remotely(hosted: Host<'_>, folder: String) -> Answer<KeptRemotely> {
    let (freed, bytes, kept) =
        hosted.with_store_mut(|store| Ok(store.keep_remotely(&folder)?)).map_err(failed)?;
    Ok(KeptRemotely { freed, bytes: big(bytes), kept })
}

/// Keep a folder on this computer again. Returns how many files are asked for.
#[tauri::command(async)]
pub fn keep_locally(hosted: Host<'_>, folder: String) -> Answer<usize> {
    let asked = hosted.with_store_mut(|store| Ok(store.keep_locally(&folder)?)).map_err(failed)?;
    hosted.nudge();
    Ok(asked)
}

/// One side of a conflict, as the window shows it.
#[derive(Serialize)]
pub struct Side {
    path: String,
    size: String,
    here: bool,
    /// The name of the device that made this version.
    by: String,
    /// What that device is, for its icon: "computer" for this one, a peer's
    /// kind once it has said (decision 0053).
    kind: Option<String>,
    at: i64,
}

/// Two versions of one file (brief §24).
#[derive(Serialize)]
pub struct Conflicted {
    path: String,
    /// The version under the file's own name, unless that has since gone.
    this: Option<Side>,
    other: Side,
}

#[tauri::command(async)]
pub fn conflicts(hosted: Host<'_>) -> Answer<Vec<Conflicted>> {
    hosted
        .with_store(|store| {
            let mut names = store.db().device_names()?;
            let own = store.device_id()?;
            names.insert(own, "this computer".into());
            let kind = |id: &qurb_sync::DeviceId| match *id == own {
                true => Some("computer".to_string()),
                false => store.db().peer_kind(id).ok().flatten(),
            };
            let side = |v: qurb_storage::ConflictVersion| Side {
                // A device never paired here has no name: "another device",
                // as for a version nobody recorded, rather than a code.
                by: v
                    .modified_by
                    .and_then(|id| names.get(&id).cloned())
                    .unwrap_or_else(|| "another device".into()),
                kind: v.modified_by.as_ref().and_then(kind),
                size: big(v.size),
                here: v.here,
                at: v.updated_at,
                path: v.path,
            };
            Ok(store
                .conflicts()?
                .into_iter()
                .map(|c| Conflicted { path: c.original_path, this: c.original.map(side), other: side(c.copy) })
                .collect())
        })
        .map_err(failed)
}

/// Settle a conflict: keep "this" version, the "other", or "both". Returns
/// the path of what was kept.
#[tauri::command(async)]
pub fn settle_conflict(hosted: Host<'_>, other: String, keep: String) -> Answer<String> {
    let keep = match keep.as_str() {
        "this" => qurb_storage::Keep::Original,
        "other" => qurb_storage::Keep::Copy,
        "both" => qurb_storage::Keep::Both,
        _ => return Err(format!("keep this, other or both, not {keep}")),
    };
    let own_name = qurb_cli::Config::load(&qurb_cli::store_dir(&hosted.root()))
        .map(|c| c.name)
        .unwrap_or_default();
    let kept = hosted
        .with_store_mut(|store| {
            let mut names = store.db().device_names()?;
            if !own_name.is_empty() {
                names.insert(store.device_id()?, own_name.clone());
            }
            let label = store
                .db()
                .folder_row(&other)?
                .and_then(|(row, _)| row.modified_by)
                .and_then(|id| names.get(&id).cloned())
                .unwrap_or_else(|| "other version".into());
            Ok(store.settle_conflict(&other, keep, &label)?)
        })
        .map_err(failed)?;
    hosted.nudge();
    Ok(kept)
}

/// A file in Recently deleted (decision 0042).
#[derive(Serialize)]
pub struct Deleted {
    id: i64,
    path: String,
    size: String,
    /// Unix seconds.
    at: i64,
    /// "this computer", or the name of the device whose deletion it was.
    by: Option<String>,
    why: Option<String>,
    /// From this computer's Private Vault, where restoring puts it back.
    private: bool,
    /// The device whose Private Vault it was in, when not this computer's:
    /// one of this person's phones, which takes it back once it is restored
    /// here (decision 0073).
    of: Option<String>,
}

#[tauri::command(async)]
pub fn recently_deleted(hosted: Host<'_>) -> Answer<Vec<Deleted>> {
    hosted
        .with_store(|store| {
            let names = store.db().device_names()?;
            let me = store.device_id()?;
            Ok(store
                .recently_deleted()?
                .into_iter()
                .map(|entry| Deleted {
                    id: entry.id,
                    path: entry.path,
                    size: big(entry.size),
                    at: entry.deleted_at,
                    by: entry.deleted_by.map(|id| match id == me {
                        true => "this computer".to_string(),
                        false => names.get(&id).cloned().unwrap_or_else(|| id.short()),
                    }),
                    why: entry.why,
                    private: entry.scope == Some(me),
                    of: entry
                        .scope
                        .filter(|owner| *owner != me)
                        .map(|owner| names.get(&owner).cloned().unwrap_or_else(|| owner.short())),
                })
                .collect())
        })
        .map_err(failed)
}

/// Put a file back from Recently deleted. Returns where it went, which is
/// beside its old path when something is there now.
#[tauri::command(async)]
pub fn restore_deleted(hosted: Host<'_>, id: i64) -> Answer<String> {
    let at = hosted.with_store_mut(|store| Ok(store.restore_from_trash(id)?)).map_err(failed)?;
    hosted.nudge();
    Ok(at)
}

/// Delete a file in Recently deleted for good.
#[tauri::command(async)]
pub fn forget_deleted(hosted: Host<'_>, id: i64) -> Answer<()> {
    hosted.with_store_mut(|store| Ok(store.forget_deleted(id)?)).map_err(failed)
}

/// What removing a device would do, for the question asked before doing it.
#[derive(Serialize)]
pub struct Removal {
    name: String,
    /// Sends it has not collected, which removal cancels.
    waiting: usize,
    /// Files this device keeps for it, and their total size.
    kept: usize,
    kept_bytes: String,
    /// Files freed here that only it keeps: removing it leaves them nowhere to
    /// come back from.
    only_there: Vec<String>,
    /// Whether it keeps this device's own files.
    holds_ours: bool,
}

fn paired_device(hosted: &Host<'_>, text: &str) -> Answer<qurb_cli::Device> {
    match hosted
        .with_store(|store| Ok(View::new(store, 0).device_named(text)?))
        .map_err(failed)?
    {
        qurb_cli::Recipient::One(device) => Ok(*device),
        _ => Err(format!("no paired device {text}")),
    }
}

#[tauri::command(async)]
pub fn removal_plan(hosted: Host<'_>, device: String) -> Answer<Removal> {
    let device = paired_device(&hosted, &device)?;
    let plan = hosted.with_store(|store| Ok(store.removal_plan(&device.id)?)).map_err(failed)?;
    Ok(Removal {
        name: device.name,
        waiting: plan.waiting.len(),
        kept: plan.kept_for_it.len(),
        kept_bytes: big(plan.kept_for_it.iter().map(|(_, size)| size).sum()),
        only_there: plan.only_there,
        holds_ours: plan.holds_ours,
    })
}

/// Stop trusting a device. See [`qurb_storage::Store::remove_device`] for
/// what that does and does not do.
///
/// The daemon is told at once, so its connection to the device closes now
/// rather than at the next check of the trust store.
#[tauri::command(async)]
pub fn remove_device(hosted: Host<'_>, device: String, delete_kept: bool) -> Answer<()> {
    let device = paired_device(&hosted, &device)?;
    hosted
        .with_store_mut(|store| Ok(store.remove_device(&device.id, &device.name, delete_kept)?))
        .map_err(failed)?;
    hosted.nudge();
    Ok(())
}

/// Ask for a file whose local copy was dropped.
///
/// Records the request rather than performing it, exactly as `qurb fetch` does:
/// no peer may be reachable right now, and a request written to the index is
/// acted on whenever one next is. That is also what makes asking for a file
/// while offline work.
#[tauri::command(async)]
pub fn fetch(hosted: Host<'_>, path: String) -> Answer<bool> {
    hosted.with_store(|store| Ok(store.db().want(&path)?)).map_err(failed)
}

/// Change how much disk this folder may use.
///
/// Written to the config file, which the daemon re-reads every maintenance
/// pass. Not sent to the daemon directly: the file is the setting, and a
/// window that told the daemon without writing it down would lose the change on
/// the next restart.
#[tauri::command(async)]
pub fn set_limit(hosted: Host<'_>, bytes: String) -> Answer<()> {
    let bytes: u64 = bytes.parse().map_err(|_| "that is not a number of bytes".to_string())?;
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.limit = bytes;
    config.save(&dir).map_err(failed)
}

/// This computer's own folders, backed up to its other computers (the
/// owner's list of 2026-10-28; `qurb_cli::backup`): each folder, what of it
/// is kept in this computer's vault so far, and the computers that may keep
/// that vault, each on or off.
#[derive(Serialize)]
pub struct Backups {
    folders: Vec<BackedUpFolder>,
    keepers: Vec<Keeper>,
}

#[derive(Serialize)]
pub struct BackedUpFolder {
    /// The name it is kept under.
    name: String,
    /// Where it is on this computer.
    path: String,
    /// Files of it kept so far, and their size.
    files: u64,
    size: String,
    /// Why it is not backed up, if it is not: gone, or inside the synced
    /// folder.
    refused: Option<String>,
}

#[derive(Serialize)]
pub struct Keeper {
    fingerprint: String,
    name: String,
    /// It keeps this computer's own files now.
    on: bool,
    online: bool,
}

#[tauri::command(async)]
pub fn backups(hosted: Host<'_>) -> Answer<Backups> {
    let root = hosted.root();
    let config = qurb_cli::Config::load(&qurb_cli::store_dir(&root)).map_err(failed)?;
    let mut folders = Vec::new();
    for (name, path) in qurb_cli::backup::named(&config.backup_folders) {
        let prefix = format!("{}/{name}", qurb_cli::backup::prefix());
        let kept = hosted.with_store(|store| Ok(store.db().outside_under(&prefix)?)).map_err(failed)?;
        folders.push(BackedUpFolder {
            name,
            refused: qurb_cli::backup::refused(&path, &root),
            path: path.display().to_string(),
            files: kept.len() as u64,
            size: big(kept.iter().map(|r| r.size).sum()),
        });
    }
    let holders = hosted.with_store(|store| Ok(store.db().holders()?)).map_err(failed)?;
    let devices = hosted.with_store(|store| Ok(View::new(store, 0).devices()?)).map_err(failed)?;
    let links = hosted.status().map(|s| s.links).unwrap_or_default();
    let keepers = devices
        .into_iter()
        .filter(|d| d.relation == qurb_storage::db::Relation::Own && d.replaced_by.is_none() && d.kind.as_deref() != Some("phone"))
        .map(|d| Keeper {
            on: holders.contains(&d.id),
            online: links.iter().any(|l| l.fingerprint == d.fingerprint),
            fingerprint: d.fingerprint,
            name: d.name,
        })
        .collect();
    Ok(Backups { folders, keepers })
}

/// The folders to back up, as chosen: refused if one cannot be.
#[tauri::command(async)]
pub fn set_backup_folders(hosted: Host<'_>, folders: Vec<String>) -> Answer<()> {
    let root = hosted.root();
    let folders: Vec<std::path::PathBuf> = folders.iter().map(std::path::PathBuf::from).collect();
    for folder in &folders {
        if let Some(why) = qurb_cli::backup::refused(folder, &root) {
            return Err(why);
        }
    }
    let dir = qurb_cli::store_dir(&root);
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.backup_folders = folders;
    config.save(&dir).map_err(failed)?;
    hosted.sync_now();
    Ok(())
}

/// Whether one of this person's computers keeps this one's own files -- its
/// Vault and its backed-up folders (`qurb holders`).
#[tauri::command(async)]
pub fn set_keeper(hosted: Host<'_>, device: String, on: bool) -> Answer<()> {
    let device = recipient(&hosted, &device)?;
    hosted
        .with_store(|store| {
            match on {
                true => store.db().add_holder(&device.id)?,
                false => {
                    store.db().remove_holder(&device.id)?;
                }
            }
            Ok(())
        })
        .map_err(failed)?;
    hosted.sync_now();
    Ok(())
}

/// The person's word that `older` -- a device of theirs paired before -- is
/// `newer` set up again under another name: one device from then on, the
/// newer standing for both and reaching what is kept for the older. `newer`
/// absent takes the word back.
#[tauri::command(async)]
pub fn same_device(hosted: Host<'_>, older: String, newer: Option<String>) -> Answer<()> {
    let older = recipient(&hosted, &older)?;
    let newer = newer.map(|n| recipient(&hosted, &n)).transpose()?;
    if let Some(newer) = &newer {
        if older.relation != qurb_storage::db::Relation::Own || newer.relation != qurb_storage::db::Relation::Own {
            return Err("only two of your own devices can be the same one".to_string());
        }
        if older.id == newer.id || older.paired_at >= newer.paired_at {
            return Err(format!("{} was paired after {}, so it cannot be what it became", older.name, newer.name));
        }
    }
    hosted
        .with_store(|store| Ok(store.db().set_same_device(&older.id, newer.as_ref().map(|n| &n.id))?))
        .map_err(failed)
}

/// How fast files may go to and come from other devices, in bytes a second,
/// "0" for no limit: saved, and applied at once, since the daemon runs here.
#[tauri::command(async)]
pub fn set_bandwidth(hosted: Host<'_>, up: String, down: String) -> Answer<()> {
    let rate = |r: &str| r.parse::<u64>().map_err(|_| "that is not a number of bytes".to_string());
    let (up, down) = (rate(&up)?, rate(&down)?);
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.upload_limit = up;
    config.download_limit = down;
    config.save(&dir).map_err(failed)?;
    qurb_peer::limit::set(up, down);
    Ok(())
}

/// A `SystemTime` as unix seconds, or nothing if the clock says it is before
/// 1970 — which is not worth propagating an error over.
fn unix(at: std::time::SystemTime) -> Option<i64> {
    at.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// The RFC 4648 test vectors: every padding case.
    #[test]
    fn base64_matches_the_standard() {
        for (plain, encoded) in [
            ("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded, "{plain:?}");
        }
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    /// A JPEG as a camera writes one: an APP1 EXIF block whose IFD1 points at
    /// a thumbnail, in either byte order; and one with no thumbnail.
    #[test]
    fn a_cameras_thumbnail_is_found_in_its_exif() {
        let thumb = [0xFF, 0xD8, 0xFF, 0xD9];
        for little in [true, false] {
            let w16 = |v: u16| if little { v.to_le_bytes() } else { v.to_be_bytes() };
            let w32 = |v: u32| if little { v.to_le_bytes() } else { v.to_be_bytes() };
            let mut tiff = Vec::new();
            tiff.extend_from_slice(if little { b"II" } else { b"MM" });
            tiff.extend_from_slice(&w16(42));
            tiff.extend_from_slice(&w32(8)); // IFD0 at 8
            tiff.extend_from_slice(&w16(0)); // IFD0: no entries
            tiff.extend_from_slice(&w32(14)); // IFD1 at 14
            tiff.extend_from_slice(&w16(2)); // IFD1: two entries
            for (tag, value) in [(0x0201u16, 44u32), (0x0202, thumb.len() as u32)] {
                tiff.extend_from_slice(&w16(tag));
                tiff.extend_from_slice(&w16(4));
                tiff.extend_from_slice(&w32(1));
                tiff.extend_from_slice(&w32(value));
            }
            tiff.extend_from_slice(&w32(0)); // no IFD2
            assert_eq!(tiff.len(), 44);
            tiff.extend_from_slice(&thumb);

            let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
            jpeg.extend_from_slice(&((tiff.len() + 8) as u16).to_be_bytes());
            jpeg.extend_from_slice(b"Exif\0\0");
            jpeg.extend_from_slice(&tiff);
            jpeg.extend_from_slice(&[0xFF, 0xDA, 0, 2]);
            assert_eq!(exif_thumbnail(&jpeg), Some(&thumb[..]), "little endian: {little}");
        }
        assert_eq!(exif_thumbnail(&[0xFF, 0xD8, 0xFF, 0xDA, 0, 2]), None);
        assert_eq!(exif_thumbnail(b"not a jpeg"), None);
    }

    fn detail(path: &std::path::Path) -> String {
        format!("sent to this device; saved to {}", path.display())
    }

    /// A typed folder is always absolute, and always under the home folder
    /// unless it says otherwise.
    #[test]
    fn a_typed_folder_is_found_from_the_home_folder() {
        let home = std::path::Path::new("/home/someone");
        let at = |typed: &str| expand_from(typed, Some(home));
        assert_eq!(at("~/qurb"), home.join("qurb"));
        assert_eq!(at("~"), home);
        assert_eq!(at("home/project/qurb"), home.join("home/project/qurb"));
        assert_eq!(at("Documents/qurb "), home.join("Documents/qurb"));
        assert_eq!(at("/srv/qurb"), std::path::PathBuf::from("/srv/qurb"));
        assert_eq!(at("   "), std::path::PathBuf::new(), "nothing typed is not the home folder");
    }

    #[test]
    fn a_file_in_downloads_opens_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("Downloads/qurb");
        fs::create_dir_all(downloads.join("Trip/day1")).unwrap();
        let saved = downloads.join("Trip/day1/a.txt");

        let shown = folder_to_show(&detail(&saved), &downloads).unwrap();
        assert_eq!(shown, downloads.join("Trip/day1").canonicalize().unwrap());
    }

    /// Nothing in the history can make it open anywhere else.
    #[test]
    fn nowhere_outside_downloads_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let downloads = dir.path().join("Downloads/qurb");
        fs::create_dir_all(&downloads).unwrap();
        fs::create_dir_all(dir.path().join("private")).unwrap();

        let outside: [std::path::PathBuf; 3] = [
            dir.path().join("private/x.txt"),
            downloads.join("../../private/x.txt"),
            "/etc/passwd".into(),
        ];
        for saved in outside {
            assert!(folder_to_show(&detail(&saved), &downloads).is_err(), "{}", saved.display());
        }

        // Nor through a link inside Downloads pointing out of it.
        std::os::unix::fs::symlink(dir.path().join("private"), downloads.join("sneaky")).unwrap();
        let through_link = downloads.join("sneaky/x.txt");
        assert!(folder_to_show(&detail(&through_link), &downloads).is_err());
    }

    #[test]
    fn an_entry_that_is_not_a_download_opens_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(folder_to_show("sent to this device", dir.path()).is_err());
        assert!(folder_to_show("", dir.path()).is_err());
    }
}

/// Stop qurb: the window and the syncing both, until it is opened again.
/// Closing the window only hides it.
#[tauri::command]
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

/// Whether qurb starts, hidden, when this person logs in.
#[tauri::command]
pub fn starts_at_login() -> bool {
    crate::autostart::config_dir().is_some_and(|dir| crate::autostart::is_enabled(&dir))
}

#[tauri::command(async)]
pub fn set_starts_at_login(on: bool) -> Answer<()> {
    let dir = crate::autostart::config_dir().ok_or("there is no config directory to start from")?;
    if on {
        let program = std::env::current_exe().map_err(failed)?;
        crate::autostart::enable(&dir, &program).map_err(failed)
    } else {
        crate::autostart::disable(&dir).map_err(failed)
    }
}

