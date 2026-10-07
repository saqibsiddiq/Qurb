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
#[tauri::command]
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

#[tauri::command]
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
#[tauri::command]
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

/// Make a new device and start it.
///
/// Nothing to write down first (decision 0052): another device is added with
/// a code, which carries the key. The 24 words can still be shown, in
/// Settings, by [`reveal_phrase`].
///
/// `allowance` is the disk this device may use, in bytes, from the question
/// asked before the key is made (decision 0038). Written with the rest of the
/// device's settings, before anything syncs.
#[tauri::command]
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
#[tauri::command]
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
#[tauri::command]
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
    // Every paired device holds this key: pairing checks it (decision 0053).
    let (key_also_on, phone_holds_key) = hosted
        .with_store(|store| {
            let peers = store.db().trusted_peers()?;
            let phone = peers.iter().any(|p| {
                store.db().peer_kind(&p.device_id).ok().flatten().as_deref() == Some("phone")
            });
            Ok((peers.into_iter().map(|p| p.name).collect::<Vec<_>>(), phone))
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
#[tauri::command]
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
#[tauri::command]
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

/// Hand a folder to the desktop's file manager.
///
/// `xdg-open`, which every Linux desktop answers with its own file manager.
/// Waited for on a thread of its own, so it neither blocks the window nor
/// leaves a finished process behind.
fn reveal(dir: &std::path::Path) -> Answer<()> {
    let mut child = std::process::Command::new("xdg-open")
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("could not open the file manager: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
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
}

#[derive(Serialize)]
pub struct PairingState {
    /// "none", "waiting", "asking", "paired", "expired" or "failed".
    state: &'static str,
    name: Option<String>,
    fingerprint: Option<String>,
    message: Option<String>,
    /// While "asking": the number the device asking should be showing, and
    /// what it is (decision 0053).
    number: Option<String>,
    kind: Option<String>,
    wants_key: bool,
}

impl PairingState {
    fn of(state: &'static str) -> Self {
        Self { state, name: None, fingerprint: None, message: None, number: None, kind: None, wants_key: false }
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
#[tauri::command]
pub fn send_files(hosted: Host<'_>, paths: Vec<String>, to: String) -> Answer<SendReport> {
    // The same lookup `qurb send` uses, so the two cannot disagree about which
    // device a name refers to.
    let device = match hosted
        .with_store(|store| Ok(View::new(store, 0).device_named(&to)?))
        .map_err(failed)?
    {
        qurb_cli::Recipient::One(device) => device,
        qurb_cli::Recipient::Unknown { .. } => return Err(format!("no paired device {to}")),
        qurb_cli::Recipient::Several(_) => {
            return Err(format!("more than one device is called {to}"))
        }
    };

    let picked: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    let plan = qurb_cli::send::plan(&picked);

    let mut skipped: Vec<Skipped> = plan
        .skipped
        .into_iter()
        .map(|(path, why)| Skipped { path: path.display().to_string(), why })
        .collect();
    let (mut sent, mut bytes) = (Vec::new(), 0u64);
    for (name, source) in plan.files {
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
#[tauri::command]
pub async fn start_pairing(hosted: Host<'_>) -> Answer<Invitation> {
    let (store, identity, name) = hosted.for_pairing().map_err(failed)?;
    let master = hosted.master().map_err(failed)?;
    let now = now();

    // Port zero, not the configured one. The daemon in this process is already
    // listening on that, and a pairing host that failed to bind would be a
    // pairing screen that could not open while syncing was working — which is
    // every time somebody would use it. The invite carries whatever port this
    // gets, so the far end dials the right one either way.
    let host = qurb_peer::PairingHost::open("0.0.0.0:0".parse().unwrap(), &identity, now)
        .map_err(failed)?;

    let code = host.invite().encode();
    let spoken = host.invite().for_humans();
    let expires_at = host.invite().expires_at;
    // A code that cannot be drawn is still a code. It can be typed, and it can
    // be read aloud, so failing to render is worth noting and not worth
    // refusing over.
    let qr = qurb_cli::qr::svg(&code).ok();

    let attempt = hosted.begin_pairing(code.clone(), spoken.clone(), expires_at);
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
        let outcome = match host.wait(store, &ours, now, approve).await {
            Ok(peer) => {
                // The daemon syncs with it now, not at its next check.
                nudge.notify_one();
                crate::session::Pairing::Paired {
                    name: peer.name,
                    fingerprint: peer.fingerprint.short(),
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

    Ok(Invitation { code, spoken, expires_at, qr })
}

/// How the attempt on screen is going. Polled while the code is up.
#[tauri::command]
pub fn pairing_state(hosted: Host<'_>) -> Answer<PairingState> {
    use crate::session::Pairing;
    let Some(attempt) = hosted.attempt() else {
        return Ok(PairingState::of("none"));
    };

    Ok(match attempt.state() {
        Pairing::Waiting => PairingState::of("waiting"),
        Pairing::Asking { name, kind, number, wants_key } => PairingState {
            name: Some(name),
            number: Some(number),
            kind,
            wants_key,
            ..PairingState::of("asking")
        },
        Pairing::Paired { name, fingerprint } => PairingState {
            name: Some(name),
            fingerprint: Some(fingerprint),
            ..PairingState::of("paired")
        },
        Pairing::Expired => PairingState::of("expired"),
        Pairing::Failed(message) => PairingState { message: Some(message), ..PairingState::of("failed") },
    })
}

/// The person's answer to the device asking to pair (decision 0053).
#[tauri::command]
pub fn answer_pairing(hosted: Host<'_>, approve: bool) -> Answer<()> {
    match hosted.attempt() {
        Some(attempt) if attempt.answer(approve) => Ok(()),
        _ => Err("no device is asking any more".to_string()),
    }
}

/// The number this computer will show while the device showing `code`
/// approves it, before joining it (decision 0053).
#[tauri::command]
pub fn pairing_number(hosted: Host<'_>, code: String) -> Answer<String> {
    let invite = qurb_peer::Invite::parse(code.trim())
        .map_err(|_| "that is not a pairing code — check it was copied whole".to_string())?;
    let (_, identity, _) = hosted.for_pairing().map_err(failed)?;
    Ok(invite.number_for(&identity.fingerprint()))
}

/// The same, for a folder being set up by joining: its certificate is made
/// now if it has none, and is the one joining then presents.
#[tauri::command]
pub fn setup_pairing_number(path: String, code: String) -> Answer<String> {
    qurb_cli::setup::number_for(&expand(&path), &code).map_err(failed)
}

/// Stop showing a code, and stop answering it.
///
/// Both halves matter. A cancelled invite that still worked would be worse
/// than having no cancel button at all: the code would be off the screen and
/// live, which is precisely the state somebody pressing cancel is trying to
/// avoid.
#[tauri::command]
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
    let invite = qurb_peer::Invite::parse(code.trim())
        .map_err(|_| "that is not a pairing code — check it was copied whole".to_string())?;
    let (store, identity, name) = hosted.for_pairing().map_err(failed)?;
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
        incoming: status.incoming.into_iter().map(in_flight).collect(),
        outgoing: status.outgoing.into_iter().map(in_flight).collect(),
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
        free_disk: big(qurb_cli::setup::inspect(&hosted.root()).free),
        over: numbers.over(),
        file_count: numbers.file_count,
        evicted: numbers.evicted,
        only_here: numbers.only_here,
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
#[tauri::command]
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
#[tauri::command]
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

#[tauri::command]
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
                    device: r.device.as_ref().map(name),
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
#[tauri::command]
pub fn free_file(hosted: Host<'_>, path: String) -> Answer<String> {
    hosted
        .with_store_mut(|store| Ok(store.free_local(&path)?))
        .map(big)
        .map_err(failed)
}

/// Move a file into this computer's Private Vault, or out of it to every
/// device (decision 0057). Returns whether it moved. Refused, by the store,
/// for a file whose bytes are not here.
#[tauri::command]
pub fn move_file_area(hosted: Host<'_>, path: String, private: bool) -> Answer<bool> {
    hosted.with_store_mut(|store| Ok(store.move_area(&path, private)?)).map_err(failed)
}

/// The devices keeping this computer's Private Vault, by name: what moving a
/// file into it says keeps a copy.
#[tauri::command]
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

#[tauri::command]
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
#[tauri::command]
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

/// Open a file with the program the desktop uses for it.
#[tauri::command]
pub fn open_file(hosted: Host<'_>, path: String) -> Answer<()> {
    reveal(&on_disk(&hosted, &path)?)
}

/// Open the folder a file is in, in the file manager.
#[tauri::command]
pub fn show_file(hosted: Host<'_>, path: String) -> Answer<()> {
    let file = on_disk(&hosted, &path)?;
    reveal(file.parent().ok_or("that file has no folder")?)
}

/// Open the synced folder itself in the file manager.
#[tauri::command]
pub fn show_root(hosted: Host<'_>) -> Answer<()> {
    reveal(&hosted.root())
}

/// Whether files added here go to this device's Private Vault rather than
/// the shared area. The running daemon picks it up at its next pass.
#[tauri::command]
pub fn set_privacy(hosted: Host<'_>, private: bool) -> Answer<()> {
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.own_files_private = private;
    config.save(&dir).map_err(failed)
}

/// Whether this desktop raises its three notifications.
#[tauri::command]
pub fn set_notifications(hosted: Host<'_>, on: bool) -> Answer<()> {
    let dir = qurb_cli::store_dir(&hosted.root());
    let mut config = qurb_cli::Config::load(&dir).map_err(failed)?;
    config.notifications = on;
    config.save(&dir).map_err(failed)
}

#[tauri::command]
pub fn devices(hosted: Host<'_>) -> Answer<Vec<Device>> {
    // Live, from the daemon: which devices it holds a connection to, and how.
    // The index knows who is paired and when each was last reached; only the
    // running daemon knows what is connected now.
    let links = hosted.status().map(|s| s.links).unwrap_or_default();
    Ok(hosted
        .with_store(|store| Ok(View::new(store, 0).devices()?))
        .map_err(failed)?
        .into_iter()
        .map(|d| {
            let link = links.iter().find(|l| l.fingerprint == d.fingerprint);
            Device {
                id: d.id.short(),
                route: link.map(|l| if l.relayed { "relay" } else { "direct" }),
                address: link.map(|l| l.address.clone()),
                name: d.name,
                fingerprint: d.fingerprint,
                paired_at: d.paired_at,
                last_seen: d.last_seen,
            }
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
            // A device since removed is named as it was when it was here.
            device: r.device.map(|id| named.get(&id).cloned().unwrap_or_else(|| id.short())),
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
            to_id: o.to.short(),
        })
        .collect())
}

/// Take back a send the other device has not collected yet.
///
/// Refused once it has been: the file is theirs by then. See
/// [`qurb_storage::Store::cancel_send`].
#[tauri::command]
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

#[tauri::command]
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
#[tauri::command]
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
#[tauri::command]
pub fn keep_remotely(hosted: Host<'_>, folder: String) -> Answer<KeptRemotely> {
    let (freed, bytes, kept) =
        hosted.with_store_mut(|store| Ok(store.keep_remotely(&folder)?)).map_err(failed)?;
    Ok(KeptRemotely { freed, bytes: big(bytes), kept })
}

/// Keep a folder on this computer again. Returns how many files are asked for.
#[tauri::command]
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

#[tauri::command]
pub fn conflicts(hosted: Host<'_>) -> Answer<Vec<Conflicted>> {
    hosted
        .with_store(|store| {
            let mut names = store.db().device_names()?;
            names.insert(store.device_id()?, "this computer".into());
            let side = |v: qurb_storage::ConflictVersion| Side {
                by: v
                    .modified_by
                    .map(|id| names.get(&id).cloned().unwrap_or_else(|| id.short()))
                    .unwrap_or_else(|| "another device".into()),
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
#[tauri::command]
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
}

#[tauri::command]
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
                })
                .collect())
        })
        .map_err(failed)
}

/// Put a file back from Recently deleted. Returns where it went, which is
/// beside its old path when something is there now.
#[tauri::command]
pub fn restore_deleted(hosted: Host<'_>, id: i64) -> Answer<String> {
    let at = hosted.with_store_mut(|store| Ok(store.restore_from_trash(id)?)).map_err(failed)?;
    hosted.nudge();
    Ok(at)
}

/// Delete a file in Recently deleted for good.
#[tauri::command]
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
        qurb_cli::Recipient::One(device) => Ok(device),
        _ => Err(format!("no paired device {text}")),
    }
}

#[tauri::command]
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
#[tauri::command]
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

/// A `SystemTime` as unix seconds, or nothing if the clock says it is before
/// 1970 — which is not worth propagating an error over.
fn unix(at: std::time::SystemTime) -> Option<i64> {
    at.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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

#[tauri::command]
pub fn set_starts_at_login(on: bool) -> Answer<()> {
    let dir = crate::autostart::config_dir().ok_or("there is no config directory to start from")?;
    if on {
        let program = std::env::current_exe().map_err(failed)?;
        crate::autostart::enable(&dir, &program).map_err(failed)
    } else {
        crate::autostart::disable(&dir).map_err(failed)
    }
}

