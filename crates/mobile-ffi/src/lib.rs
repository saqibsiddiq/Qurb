//! The engine, as a phone can call it.
//!
//! Android and iOS cannot call Rust directly. This crate is the seam: a small,
//! deliberately boring surface that [UniFFI] turns into Kotlin and Swift. It
//! contains no sync logic of its own — everything here delegates to
//! [`qurb_engine`] — because logic that lives behind an FFI boundary is logic
//! that cannot be tested from the rest of the workspace.
//!
//! Four constraints shape it, and all four come from the platforms rather than
//! from taste.
//!
//! **Memory.** An iOS FileProvider extension is killed at a ceiling in the tens
//! of megabytes. So nothing here returns a file's contents. [`Qurb::export`]
//! writes to a path the caller supplies and [`Qurb::import`] reads from one.
//! Bytes never cross the boundary, which also avoids copying every file through
//! the FFI's own serialisation.
//!
//! **Threading.** The engine takes `&mut self`, so one lock guards it. Calls
//! block; the platform side is expected to make them off the main thread, which
//! both Kotlin coroutines and Swift's async do naturally.
//!
//! **Time.** Both platforms grant background work a window and kill anything
//! that outstays one, so the entry point that matters is not "sync" but
//! [`Qurb::sync_within`] — sync for at most this long, and stop cleanly.
//!
//! **Errors.** A Rust error chain does not survive the crossing. Everything
//! becomes [`QurbError`], flat and matchable, with the detail kept as text.
//!
//! [UniFFI]: https://mozilla.github.io/uniffi-rs/

use qurb_engine::{Engine, SyncStats};
use qurb_keys::{MasterKey, Purpose, RecoveryPhrase, Vault};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

uniffi::setup_scaffolding!();

/// What can go wrong, flattened for the crossing.
///
/// Deliberately few. A Kotlin or Swift caller can only usefully distinguish
/// cases it can *do* something about — ask for the passphrase again, tell the
/// user the file is gone, retry later — so the enum is that list and nothing
/// more. `detail` carries the original message for logs and bug reports.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum QurbError {
    /// No vault at this location. Call `create` or `restore` first.
    #[error("not set up: {detail}")]
    NotSetUp { detail: String },

    /// The vault is here but the passphrase was wrong or missing.
    #[error("locked: {detail}")]
    Locked { detail: String },

    /// The recovery phrase was not 24 valid words in a valid order.
    #[error("bad recovery phrase: {detail}")]
    BadPhrase { detail: String },

    /// No such file in the synced tree.
    #[error("no such file: {detail}")]
    NotFound { detail: String },

    /// The disk said no: out of space, permission denied, a bad path.
    #[error("storage failed: {detail}")]
    Storage { detail: String },

    /// A pairing code that was not a pairing code, or had expired.
    #[error("bad pairing code: {detail}")]
    BadCode { detail: String },

    /// The network refused, or nothing answered. Usually worth retrying later
    /// rather than showing as a failure: a phone syncs against devices that
    /// are asleep most of the time.
    #[error("network: {detail}")]
    Network { detail: String },

    /// Freeing a file's local copy was refused, because no other device is
    /// known to hold it: this is the only copy. Not a fault to report as one --
    /// the answer is to have a device keep it, or to send it somewhere.
    #[error("only copy: {detail}")]
    OnlyCopy { detail: String },

    /// A change asked of a computer's shared folder that it would not make,
    /// and why, in words for the person: a name taken, a folder that is not
    /// theirs (decision 0073).
    #[error("{detail}")]
    Refused { detail: String },

    /// Anything else, including bugs.
    #[error("{detail}")]
    Other { detail: String },
}

impl From<qurb_engine::Error> for QurbError {
    fn from(e: qurb_engine::Error) -> Self {
        // Matched on the storage error underneath rather than on the text,
        // which would break the first time a message is reworded.
        match &e {
            qurb_engine::Error::Storage(qurb_storage::Error::NotFound { .. }) => {
                QurbError::NotFound { detail: e.to_string() }
            }
            qurb_engine::Error::Io { .. } | qurb_engine::Error::Storage(_) => {
                QurbError::Storage { detail: e.to_string() }
            }
            _ => QurbError::Other { detail: e.to_string() },
        }
    }
}

impl From<qurb_storage::Error> for QurbError {
    fn from(e: qurb_storage::Error) -> Self {
        match &e {
            qurb_storage::Error::NotFound { .. } => QurbError::NotFound { detail: e.to_string() },
            qurb_storage::Error::CannotEvict { .. } => QurbError::OnlyCopy { detail: e.to_string() },
            _ => QurbError::Storage { detail: e.to_string() },
        }
    }
}

impl From<qurb_keys::Error> for QurbError {
    fn from(e: qurb_keys::Error) -> Self {
        QurbError::Locked { detail: e.to_string() }
    }
}

/// One file in the synced tree.
///
/// A flat record rather than the engine's own type: this crosses the boundary
/// on every directory listing, and a FileProvider asks for listings constantly.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FileEntry {
    /// Path relative to the root, with forward slashes, in NFC.
    pub path: String,
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch.
    pub modified_at: i64,
    /// Where the bytes are.
    pub available: Available,
    /// In this phone's own vault rather than the shared area.
    pub private: bool,
}

/// Where a file's bytes are, as the phone should say it.
///
/// Four answers, decided in the storage crate so the phone and the desktop
/// cannot disagree. The third is the one that matters: a file only on this
/// phone is lost with the phone, and must never be offered as space to free.
/// The fourth, since decision 0055, says plainly that a listed file is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Available {
    /// On this phone, and another device has it too.
    Here,
    /// Freed on this phone; another device has it, and `fetch` brings it back.
    Elsewhere,
    /// On this phone and nowhere else anybody knows of.
    OnlyHere,
    /// Not on this phone, and no device it can ask is known to have it
    /// (decision 0055): listed, and not something to fetch.
    Nowhere,
}

impl From<qurb_storage::db::Availability> for Available {
    fn from(a: qurb_storage::db::Availability) -> Self {
        match a {
            qurb_storage::db::Availability::Here => Available::Here,
            qurb_storage::db::Availability::Elsewhere => Available::Elsewhere,
            qurb_storage::db::Availability::OnlyHere => Available::OnlyHere,
            qurb_storage::db::Availability::Nowhere => Available::Nowhere,
        }
    }
}

impl From<qurb_storage::db::FolderEntry> for FileEntry {
    fn from(f: qurb_storage::db::FolderEntry) -> Self {
        FileEntry {
            path: f.path,
            size: f.size,
            modified_at: f.mtime_ns,
            available: f.availability.into(),
            private: f.private,
        }
    }
}

/// One directory of the folder, as a file browser shows it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Directory {
    /// The folders directly inside it, by name, sorted.
    pub folders: Vec<String>,
    /// The files directly inside it, in path order.
    pub files: Vec<FileEntry>,
}

/// Something sent to another device that it has not collected yet.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Waiting {
    pub path: String,
    pub size: u64,
    /// The device's name, as it was paired.
    pub to: String,
    /// Its fingerprint, to name it back in `cancel_send`.
    pub to_fingerprint: String,
}

/// A file about to be sent that went to that device before (decision 0059).
#[derive(Debug, Clone, uniffi::Record)]
pub struct EarlierSend {
    /// The file on this phone, as it was handed to `sent_before`.
    pub source: String,
    /// The name it went under then.
    pub sent_as: String,
    /// When, in unix seconds.
    pub at: i64,
}

/// One file of WhatsApp's folder, as the app listed it through the grant the
/// person gave (decision 0062).
#[derive(Debug, Clone, uniffi::Record)]
pub struct OutsideFile {
    /// Its path within WhatsApp's folder: `Media/WhatsApp Images/IMG-….jpg`.
    pub path: String,
    /// The document to read it from.
    pub uri: String,
    pub size: u64,
    /// When it last changed, in milliseconds since the epoch, as the listing
    /// said: how a change is noticed without reading the file.
    pub modified_ms: i64,
}

/// One of this phone's folders, at any depth, for choosing on the phone as a
/// tree (item 10 of 2026-10-14; item 1 of the owner's list of 2026-10-24).
#[derive(Debug, Clone, uniffi::Record)]
pub struct OwnFolder {
    /// From the root of shared storage: `DCIM`, `DCIM/Camera`.
    pub path: String,
    /// Beneath it, at any depth.
    pub files: u64,
    pub bytes: u64,
    /// All of it backed up, some of it, or none ([`qurb_storage::chosen`]).
    pub tick: FolderTick,
    /// Of `bytes`, how many are backed up.
    pub chosen_bytes: u64,
}

/// A folder's mark in the tree of folders to back up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FolderTick {
    All,
    Some,
    None,
}

impl From<qurb_storage::chosen::Tick> for FolderTick {
    fn from(tick: qurb_storage::chosen::Tick) -> Self {
        match tick {
            qurb_storage::chosen::Tick::All => FolderTick::All,
            qurb_storage::chosen::Tick::Some => FolderTick::Some,
            qurb_storage::chosen::Tick::None => FolderTick::None,
        }
    }
}

/// What one run of the WhatsApp backup did (decision 0062).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BackupRun {
    pub added: u32,
    pub changed: u32,
    pub unchanged: u32,
    /// Gone from WhatsApp's folder, and kept by the computer.
    pub kept: u32,
    /// Gone from WhatsApp's folder, and deleted from the backup.
    pub deleted: u32,
    /// New or changed, left for the next run when time ran out.
    pub remaining: u32,
    /// Read this run, to describe new and changed files.
    pub bytes_read: u64,
    pub failed: u32,
    /// The first file that could not be read, and why.
    pub first_failure: Option<String>,
}

/// The WhatsApp backup as it stands, for Settings (decision 0062).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BackupState {
    pub files: u64,
    pub bytes: u64,
    /// On a computer this phone may count on.
    pub held_files: u64,
    pub held_bytes: u64,
    /// Gone from WhatsApp's folder, and kept on that computer only.
    pub only_kept: u64,
}

/// Where WhatsApp's folder is kept in this phone's vault (decision 0062).
fn whatsapp_prefix() -> String {
    format!("{}/whatsapp", qurb_storage::db::OUTSIDE)
}

/// Whether a folder a computer chose can be backed up: a plain folder of the
/// phone's shared storage, named from its root. Nothing absolute, nothing
/// climbing out with `..`, nothing hidden, and not the two folders Android
/// keeps from every app (decision 0063). A computer, even a compromised one,
/// can only point at the phone's own ordinary folders.
fn choosable(folder: &str) -> bool {
    !folder.is_empty()
        && !folder.starts_with('/')
        && folder.split('/').all(|part| !part.is_empty() && part != ".." && !part.starts_with('.'))
        && !folder.starts_with("Android/data")
        && !folder.starts_with("Android/obb")
}

/// The choices made for this phone that can be backed up from
/// ([`choosable`]): what [`qurb_storage::chosen`] reads the tree from.
fn usable_choices(db: &qurb_storage::db::Db) -> Result<Vec<qurb_storage::db::Choice>, QurbError> {
    Ok(db.choices_here()?.into_iter().filter(|c| choosable(&c.folder)).collect())
}

/// Every file beneath `dir`, as entries of a folder kept where it lies: named
/// from the folder, read from where it is. Hidden names are skipped, as
/// WhatsApp's caches are, and links are not followed.
fn walk_files(dir: &Path, under: &str, into: &mut Vec<qurb_storage::OutsideEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
        let path = if under.is_empty() { name } else { format!("{under}/{name}") };
        if meta.is_dir() {
            walk_files(&entry.path(), &path, into);
        } else if meta.is_file() {
            let mtime_ns = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos() as i64);
            into.push(qurb_storage::OutsideEntry {
                path,
                source: entry.path().to_string_lossy().into_owned(),
                size: meta.len(),
                mtime_ns,
            });
        }
    }
}

/// The folders of `root` at any depth, with the files and bytes beneath
/// each, from one walk (decision 0063; at every depth since the owner's list
/// of 2026-10-24, for choosing as a tree). Hidden folders, and the two Android
/// keeps from every app, are left out, and so is a folder with no file
/// beneath it.
fn folder_summaries(root: &Path) -> Vec<qurb_storage::db::FolderSummary> {
    let mut files = Vec::new();
    walk_files(root, "", &mut files);
    let mut folders: std::collections::BTreeMap<String, (u64, u64)> = std::collections::BTreeMap::new();
    for file in &files {
        let parts: Vec<&str> = file.path.split('/').collect();
        // A file at the top belongs to no folder; one in a folder counts for
        // it and for each folder it is in.
        for depth in 1..parts.len() {
            let folder = parts[..depth].join("/");
            if !choosable(&folder) {
                break;
            }
            let entry = folders.entry(folder).or_default();
            entry.0 += 1;
            entry.1 += file.size;
        }
    }
    folders
        .into_iter()
        .map(|(path, (files, bytes))| qurb_storage::db::FolderSummary { path, files, bytes })
        .collect()
}

/// One word of the recovery phrase, as somebody typed it back from paper.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PhraseAnswer {
    /// Its position as shown, counting from one.
    pub position: u32,
    pub word: String,
}

/// One thing that happened, for a history screen.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Happening {
    /// For paging: pass the last one's id as `before` to get older ones.
    pub id: i64,
    /// Unix seconds.
    pub at: i64,
    /// `stored`, `deleted`, `received`, `sent`, `collected`, `evicted`,
    /// `restored`, `conflicted`, `paired`, `failed`, `cancelled`, or a word a
    /// newer build knows and this one does not.
    pub kind: String,
    pub path: Option<String>,
    pub size: Option<u64>,
    /// The other device's name, where there is one.
    pub device: Option<String>,
    /// Why it failed, what a conflict was filed as, where a file went.
    pub detail: Option<String>,
}

/// What a scan did.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ScanSummary {
    pub stored: u32,
    pub unchanged: u32,
    pub deleted: u32,
    /// Files skipped because another file's name normalised to the same path.
    pub collided: u32,
    /// Files that failed individually. The scan continued past them.
    pub failed: u32,
}

impl From<SyncStats> for ScanSummary {
    fn from(s: SyncStats) -> Self {
        Self {
            stored: s.stored as u32,
            unchanged: s.unchanged as u32,
            deleted: s.deleted as u32,
            collided: s.collided as u32,
            failed: s.failures.len() as u32,
        }
    }
}

/// A file no other device is known to hold.
#[derive(Debug, Clone, uniffi::Record)]
pub struct OnlyHere {
    pub path: String,
    pub size: u64,
    /// In this phone's Private Vault, or sent to a device that has not
    /// collected it yet.
    pub private: bool,
}

/// What the store costs on this device.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Usage {
    /// What the user's files add up to, counted the way they would count them:
    /// three copies of one photo are three photos.
    pub logical: u64,
    /// What qurb actually occupies on this phone: the files in the folder, plus
    /// the chunk store's copies of what the folder cannot supply. Both,
    /// because under single-copy storage (decision 0024) neither half is the
    /// whole; this used to report the chunk store alone, which read as a
    /// saving it was not.
    pub on_disk: u64,
}

/// What [`Qurb::housekeep`] freed.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Tidied {
    /// Bytes the chunk store gave back.
    pub freed: u64,
    /// Deleted files past the retention window, now gone for good.
    pub tombstones_expired: u32,
}

/// What this device is still the only holder of.
///
/// The answer to "did my photo get there yet". While this is non-empty, losing
/// the phone loses work, which is worth being able to say plainly rather than
/// leaving someone to guess from a sync that reported no error.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Outstanding {
    /// Files this device made that no other device is known to hold.
    pub files: Vec<FileEntry>,
    pub bytes: u64,
}

// ---------------------------------------------------------------------------
// Key protection
// ---------------------------------------------------------------------------

/// Somewhere the platform can keep the master key.
///
/// Implemented in Kotlin or Swift, because neither platform's keystore is
/// reachable from Rust: Android's is a Java API needing a `Context`, and iOS's
/// needs entitlements belonging to an app bundle. Both are a few lines on their
/// own side and impossible on this one.
///
/// What it keeps is 32 bytes — the master key itself, not something wrapping
/// it. Both platforms handle small secrets well, and a wrapping layer would
/// mean running a key-derivation function at every launch for nothing, since
/// the stored value is already full entropy.
///
/// Implementations are called from whatever thread opens the vault, which on a
/// phone is during app launch. They must be safe to call from any thread.
///
/// # What this is worth
///
/// On Android, a key in the Keystore is held by hardware the app cannot read
/// directly, and on a device with a secure element it never enters the app's
/// memory in exportable form. On iOS, Keychain items marked
/// `WhenUnlockedThisDeviceOnly` are unreadable while the phone is locked and do
/// not travel to a backup.
///
/// Neither helps while the app is running and holding the key. That is what it
/// means to be a program that can decrypt your files.
#[uniffi::export(with_foreign)]
pub trait KeyStore: Send + Sync {
    /// Keep `secret` under `label`, replacing anything already there.
    fn put(&self, label: String, secret: Vec<u8>) -> Result<(), QurbError>;

    /// Return what was kept, or `None` if nothing was.
    ///
    /// `None` rather than an error: a first launch asks before anything has
    /// been stored, and that is not a failure.
    fn get(&self, label: String) -> Result<Option<Vec<u8>>, QurbError>;

    /// Forget it. Removing something already absent must succeed.
    fn remove(&self, label: String) -> Result<(), QurbError>;
}

/// Adapts a platform [`KeyStore`] to what `qurb-keys` expects.
///
/// Two traits rather than one because `qurb-keys` must not depend on UniFFI:
/// the key layer is used by the daemon, the tests and the CLI, none of which
/// have any business knowing that a phone exists.
struct PlatformStore(Arc<dyn KeyStore>);

impl qurb_keys::SecretStore for PlatformStore {
    fn put(&self, label: &str, secret: &[u8]) -> qurb_keys::Result<()> {
        self.0
            .put(label.to_string(), secret.to_vec())
            .map_err(|e| qurb_keys::Error::Keystore { detail: e.to_string() })
    }

    fn get(&self, label: &str) -> qurb_keys::Result<Option<Vec<u8>>> {
        self.0
            .get(label.to_string())
            .map_err(|e| qurb_keys::Error::Keystore { detail: e.to_string() })
    }

    fn remove(&self, label: &str) -> qurb_keys::Result<()> {
        self.0
            .remove(label.to_string())
            .map_err(|e| qurb_keys::Error::Keystore { detail: e.to_string() })
    }
}

/// Where the services are, and what this device calls itself.
///
/// Every field has a working default, so an app that does not care can pass
/// [`Settings::default`] — which is what [`Qurb::open`] does.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Settings {
    /// Shown to other devices when pairing. Display only: nothing is ever
    /// decided from it, because the peer chooses it.
    #[uniffi(default = "phone")]
    pub device_name: String,
    /// The rendezvous service that introduces two devices.
    #[uniffi(default = "ws://localhost:9000")]
    pub signal_url: String,
    /// A relay to fall back to when no direct path exists, as `host:port` --
    /// a name or an address; a name is looked up on every pass. `None` means
    /// direct connections only, which on a cellular network often means none
    /// at all.
    #[uniffi(default = None)]
    pub relay: Option<String>,
    /// The port to listen on. Zero means any, which is right on a phone: it is
    /// always behind a router that forwards nothing, so a fixed port buys
    /// nothing and collides with whatever else wanted it.
    #[uniffi(default = 0)]
    pub port: u16,
    /// Whether to ask a public STUN server what this device's address looks
    /// like from outside.
    ///
    /// On by default, and necessary: a phone is behind carrier-grade NAT and
    /// has no idea what address a peer should dial. Turning it off restricts
    /// the device to peers on the same network, and is worth doing only when
    /// contacting a third party is itself the objection — it reveals this
    /// device's public IP to that server, as any VPN or video call does.
    #[uniffi(default = true)]
    pub discover: bool,
    /// How this device can be woken while it is not connected.
    ///
    /// A push token, from the platform's own service. A phone cannot hold a
    /// socket open in the background, so without one it learns about a change
    /// at its next scheduled look — a quarter of an hour, or longer while
    /// dozing. With one, the rendezvous service can poke it the moment another
    /// device has something, and the poke carries nothing but "go and sync".
    ///
    /// `None` on a desktop, which is already connected and needs no waking,
    /// and on a phone whose owner has not set up push.
    #[uniffi(default = None)]
    pub wake_token: Option<String>,
    /// Whether a file added on this phone goes into its own vault rather than
    /// the shared area (decision 0036). Off by default, so that turning it on
    /// is a choice the app makes once it can name a device to keep them.
    #[uniffi(default = false)]
    pub own_files_private: bool,
    /// Whether this phone takes part in the shared area. Off since decision
    /// 0073: the phone reaches a computer's shared folder live instead, and on
    /// opening lets go of the shared files it held -- each one a computer
    /// keeps dropped here, the rest moved into its own vault -- and files
    /// everything added into its vault, whatever `own_files_private` says. On
    /// only where this engine stands in for a desktop, in its own tests.
    #[uniffi(default = false)]
    pub shared_area: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device_name: "phone".to_string(),
            signal_url: "ws://localhost:9000".to_string(),
            relay: None,
            port: 0,
            discover: true,
            wake_token: None,
            own_files_private: false,
            shared_area: false,
        }
    }
}

/// A newly created identity, shown to the user once.
#[derive(Debug, uniffi::Record)]
pub struct Setup {
    /// The 24 words. The only copy of the key that exists outside this device;
    /// if the user does not write them down, a lost phone is lost files.
    pub recovery_phrase: String,
}

/// An open qurb store on this device.
///
/// One per app. Creating a second against the same directory is not prevented
/// here but will fail at the SQLite lock, which is the right place for it.
#[derive(uniffi::Object)]
pub struct Qurb {
    inner: Mutex<Engine>,
    root: PathBuf,
    store_dir: PathBuf,
    master: MasterKey,
    /// What this device calls itself when pairing. Display only.
    device_name: String,
    /// Where the rendezvous service is.
    signal_url: String,
    /// The relay to fall back to, if one is configured.
    relay: Option<String>,
    /// The port to listen on. Zero means any, which is right behind a router
    /// that forwards nothing — and a phone is always behind one of those.
    port: u16,
    /// Whether to ask a public STUN server for this device's public address.
    discover: bool,
    /// How this device can be woken while it is not connected. See
    /// [`Settings::wake_token`]. Changed by [`Qurb::set_wake_token`] when the
    /// push service issues a new one.
    wake_token: Mutex<Option<String>>,
    /// Built on first use and kept.
    ///
    /// Lazy because a phone that only browses its files should not pay for a
    /// thread pool, and kept because building one per call would be worse.
    runtime: Mutex<Option<Arc<tokio::runtime::Runtime>>>,
    /// What this device has handed to devices collecting from it. Read while
    /// a pass runs, from another thread, so it takes no lock.
    serving: Arc<ServingStats>,
    /// The app's way to open again a document it sent from where it is
    /// (decision 0060), given to every store handle opened here.
    documents: Mutex<Option<Arc<dyn qurb_storage::Documents>>>,
    /// Computers that asked, at the last sync with each, to open this phone's
    /// folder there: by fingerprint (decisions 0060 and 0067).
    open_asks: Mutex<std::collections::HashMap<String, Asked>>,
    /// Asks the person approved, answered at the next sync with that computer.
    approved: Mutex<std::collections::HashMap<String, [u8; 16]>>,
    /// What each computer answered when this phone sent the key, for the app
    /// to say: the person approved, and should know whether it opened.
    open_answers: Mutex<Vec<OpenAnswer>>,
    /// The last ask from each computer the person answered, either way. A
    /// computer goes on asking for five minutes; the same ask is not put to
    /// the person twice.
    settled: Mutex<std::collections::HashMap<String, [u8; 16]>>,
    /// Summaries that read every row, kept while the index is unchanged.
    remembered: Remembered,
    /// What the screens read the index through: a connection of its own,
    /// opened on first use, so that a list or a count is not kept waiting
    /// while a sync holds the engine to bring a large file in (`look`).
    reader: Mutex<Option<Store>>,
    chunk_key: ChunkKey,
}

/// A computer's answer to the key this phone sent it, once approved
/// (decision 0060, step 5).
#[derive(Debug, Clone, uniffi::Record)]
pub struct OpenAnswer {
    /// The computer's name.
    pub name: String,
    /// Whether the folder opened there. Not when the computer's ask had
    /// lapsed by the time the key arrived, or it keeps nothing the key opens.
    pub opened: bool,
}

/// An ask to open this phone's folder, as heard: the computer's name, the
/// ask's nonce, and whether it is one of this person's own computers.
type Asked = (String, [u8; 16], bool);

/// A computer this phone visits, asking to open the phone's folder there
/// (decision 0060, step 5).
#[derive(Debug, Clone, uniffi::Record)]
pub struct OpenAsk {
    pub fingerprint: String,
    /// The computer's name.
    pub name: String,
    /// One of this person's own computers, asking for the owner's folder
    /// (decision 0067), which it keeps unsealed; else a computer this phone
    /// visits, which keeps its folder sealed.
    pub own: bool,
}

/// The app's way to open a document again later (decision 0060). A file sent
/// from the system's file picker keeps no copy: it is read from where it is
/// when the other device collects it, through this.
#[uniffi::export(with_foreign)]
pub trait DocumentOpener: Send + Sync {
    /// A file descriptor open for reading, which the engine then owns and
    /// closes; -1 when the document cannot be opened any more.
    fn open(&self, uri: String) -> i32;
    /// Nothing more will be read from it: release the permission to read it.
    fn release(&self, uri: String);
}

/// A [`DocumentOpener`] as the store asks for one.
struct Lent(Arc<dyn DocumentOpener>);

impl qurb_storage::Documents for Lent {
    fn open(&self, source: &str) -> Option<std::fs::File> {
        let fd = self.0.open(source.to_string());
        if fd < 0 {
            return None;
        }
        // SAFETY: the app detached this descriptor for the engine to own; it
        // is not used, or closed, anywhere else.
        Some(unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) })
    }

    fn release(&self, source: &str) {
        self.0.release(source.to_string());
    }
}

/// Bytes served to devices collecting from this one, and when the last went.
#[derive(Default)]
struct ServingStats {
    bytes: std::sync::atomic::AtomicU64,
    /// Milliseconds since the Unix epoch; zero before anything has gone.
    last: std::sync::atomic::AtomicU64,
    /// Devices that reached this one and asked it something since a pass
    /// last looked: marked seen when it ends, as if the pass had reached them.
    asked_by: std::sync::Mutex<std::collections::HashSet<[u8; 32]>>,
}

impl ServingStats {
    fn now_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    /// Whether a chunk went to somebody within `window`: someone is collecting.
    fn recently(&self, window: std::time::Duration) -> bool {
        let last = self.last.load(std::sync::atomic::Ordering::Relaxed);
        last != 0 && Self::now_millis().saturating_sub(last) <= window.as_millis() as u64
    }
}

impl qurb_peer::Served for ServingStats {
    fn served(&self, _to: &qurb_peer::Fingerprint, _chunk: &blake3::Hash, bytes: u64) {
        self.bytes.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
        self.last.store(Self::now_millis(), std::sync::atomic::Ordering::Relaxed);
    }

    fn asked_by(&self, from: &qurb_peer::Fingerprint) {
        if let Ok(mut heard) = self.asked_by.lock() {
            heard.insert(*from.as_bytes());
        }
    }
}

/// What this device is handing to devices collecting from it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Serving {
    /// Bytes sent to collecting devices since this handle was opened. A screen
    /// showing progress takes the difference from when it started looking.
    pub bytes: u64,
    /// Whether any went in the last few seconds.
    pub collecting: bool,
}

/// Set up a new device, generating a key and its recovery phrase.
///
/// Fails if a vault already exists — overwriting one would destroy the only
/// copy of a key that may be protecting files this device cannot re-fetch.
///
/// `root` is the directory to sync. On iOS this is inside the app group
/// container so the FileProvider extension can reach it too; on Android it is
/// app-private storage.
#[uniffi::export]
pub fn create(root: String) -> Result<Setup, QurbError> {
    create_protected(root, None)
}

/// Set up a new device, keeping the key in the platform's keystore.
///
/// The arrangement to prefer on a phone. Without a `keystore` the key sits in
/// an owner-only file inside the app's private directory — which the kernel
/// enforces, and which is worth nothing on a device with no passcode.
///
/// Whatever is chosen here is recorded in the vault, so [`Qurb::open`] must be
/// given the same keystore afterwards. Opening without it fails saying so
/// rather than silently falling back, because a silent fallback would mean
/// reading a key that is not there.
/// Send the engine's logs somewhere a person can read them.
///
/// Called at the top of every entry point rather than by the platform, because
/// an initialisation step the caller has to remember is one that will be
/// forgotten — and was. Without a subscriber, every `tracing::` call in the
/// engine is discarded, so on Android the entire engine was silent: a local
/// discovery failure had to be diagnosed from the *other* device's logs,
/// because the phone had no account of what it had done.
///
/// Once per process. A second call is a no-op rather than an error, which is
/// what makes it safe to put at the top of everything.
///
/// Nothing but Android. Every other platform this runs on has a terminal, and
/// the desktop binaries install their own subscriber with a filter the user
/// controls.
#[cfg(target_os = "android")]
fn logging() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let Ok(layer) = tracing_android::layer("qurb") else { return };
        let _ = tracing_subscriber::registry()
            // Debug from the engine, warnings from everything else. Enough to
            // follow a sync without the noise of every library it uses, and
            // overridable by the same variable the desktop uses.
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "qurb=debug,warn".into()),
            )
            .with(layer)
            .try_init();
    });
}

#[cfg(not(target_os = "android"))]
fn logging() {}

#[uniffi::export]
pub fn create_protected(
    root: String,
    keystore: Option<Arc<dyn KeyStore>>,
) -> Result<Setup, QurbError> {
    let root = PathBuf::from(root);
    let store_dir = store_dir(&root);
    let vault = vault_at(&store_dir, keystore.as_ref());

    if vault.exists() {
        return Err(QurbError::Other {
            detail: format!("{} is already set up", root.display()),
        });
    }

    std::fs::create_dir_all(&store_dir)
        .map_err(|e| QurbError::Storage { detail: e.to_string() })?;

    let protection = match keystore {
        Some(_) => qurb_keys::Protection::Platform,
        None => qurb_keys::Protection::File,
    };

    match vault.open_or_create_with(protection, None)? {
        qurb_keys::Opened::Created { phrase, .. } => {
            Ok(Setup { recovery_phrase: phrase.to_string() })
        }
        // Unreachable: `exists` was false a moment ago, and there is one caller
        // per process. Reported rather than unwrapped, because a panic across
        // an FFI boundary is undefined behaviour on some platforms.
        qurb_keys::Opened::Existing(_) => Err(QurbError::Other {
            detail: "a vault appeared while we were creating one".into(),
        }),
    }
}

/// Set up a device from an existing recovery phrase.
///
/// This is the second phone, or the replacement for a lost one. It produces the
/// same master key, which is what lets this device read content the others
/// encrypted.
#[uniffi::export]
pub fn restore(root: String, phrase: String) -> Result<(), QurbError> {
    restore_protected(root, phrase, None)
}

/// Restore from a phrase, keeping the key in the platform's keystore.
#[uniffi::export]
pub fn restore_protected(
    root: String,
    phrase: String,
    keystore: Option<Arc<dyn KeyStore>>,
) -> Result<(), QurbError> {
    let root = PathBuf::from(root);
    let store_dir = store_dir(&root);

    let phrase = RecoveryPhrase::parse(&phrase)
        .map_err(|e| QurbError::BadPhrase { detail: e.to_string() })?;

    std::fs::create_dir_all(&store_dir)
        .map_err(|e| QurbError::Storage { detail: e.to_string() })?;

    let protection = match keystore {
        Some(_) => qurb_keys::Protection::Platform,
        None => qurb_keys::Protection::File,
    };

    vault_at(&store_dir, keystore.as_ref()).restore_with(&phrase, protection, None)?;
    Ok(())
}

/// Set up a device by joining one the person already has, from the code it
/// shows: its key comes over the pairing connection, so nothing is typed but
/// the code -- usually scanned (decision 0052). The two are paired as well.
///
/// The key is kept as [`restore_protected`] keeps it. Nothing is installed
/// unless it arrives from the device whose fingerprint the code carries; a
/// code that is wrong, expired or already used leaves only this device's new
/// certificate, which the next attempt reuses.
#[uniffi::export]
pub fn join_new(
    root: String,
    code: String,
    device_name: String,
    keystore: Option<Arc<dyn KeyStore>>,
) -> Result<PeerInfo, QurbError> {
    logging();
    let store_dir = store_dir(Path::new(&root));
    let vault = vault_at(&store_dir, keystore.as_ref());
    if vault.exists() {
        return Err(QurbError::Other { detail: format!("{root} is already set up") });
    }
    std::fs::create_dir_all(&store_dir).map_err(|e| QurbError::Storage { detail: e.to_string() })?;
    let identity = qurb_peer::Identity::load_or_create(&store_dir)
        .map_err(|e| QurbError::Storage { detail: e.to_string() })?;
    let protection = match keystore {
        Some(_) => qurb_keys::Protection::Platform,
        None => qurb_keys::Protection::File,
    };

    // A runtime of its own: nothing else is running yet, and this one lasts
    // only as long as the pairing does.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| QurbError::Other { detail: format!("no runtime: {e}") })?;
    let invite = typed_code(&runtime, &code, &identity)?;
    let paired = runtime
        .block_on(qurb_peer::join(&invite, &identity, &device_name, "phone", now(), |key| {
            let master = vault
                .restore_with(&key.to_phrase(), protection, None)
                .map_err(|e| e.to_string())?;
            let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
            let store = Store::open(&store_dir, chunk_key).map_err(|e| e.to_string())?;
            Ok(Arc::new(std::sync::Mutex::new(store)))
        }))
        // A refusal as what it was (pairing_failed): setting up a phone, these
        // still read "Network problem." until 2026-10-24.
        .map_err(|e| match e {
            qurb_peer::Error::InviteExpired => QurbError::Other {
                detail: "That code has expired: ask the other device for a new one.".into(),
            },
            other => pairing_failed(other),
        })?;

    Ok(PeerInfo {
        fingerprint: hex(paired.fingerprint.as_bytes()),
        short: paired.fingerprint.short(),
        name: paired.name,
        paired_at: now(),
        last_seen: None,
        relation: "own".into(),
        kind: None,
    })
}

/// What this phone shows while the device showing `code` approves it
/// (decision 0053): the number, for the person to compare the two screens,
/// and that device's name and kind, when it says them, for where to approve --
/// *Approve on saqib* (decision 0074). The certificate the number is derived
/// from is made here if this phone has none yet, and is the one joining then
/// presents.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingWith {
    /// Six digits, as "482 913".
    pub number: String,
    /// What the device showing the code calls itself, or None from one too
    /// old to say, or that did not answer in time.
    pub name: Option<String>,
    /// "phone", "computer" or "replica", as that device says.
    pub kind: Option<String>,
}

/// The number this phone shows while the device showing `code` approves it,
/// and who that device is ([`PairingWith`]). Shown before joining.
///
/// Six digits are found on this network here (decision 0066), and what they
/// fetched is remembered for the joining that follows. Asking the device its
/// name waits a few seconds at most, and a device that does not answer is
/// left nameless rather than failing here: the join says what went wrong.
#[uniffi::export]
pub fn pairing_with(root: String, code: String) -> Result<PairingWith, QurbError> {
    logging();
    let store_dir = store_dir(Path::new(&root));
    std::fs::create_dir_all(&store_dir).map_err(|e| QurbError::Storage { detail: e.to_string() })?;
    let identity = qurb_peer::Identity::load_or_create(&store_dir)
        .map_err(|e| QurbError::Storage { detail: e.to_string() })?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(|e| QurbError::Other { detail: format!("no runtime: {e}") })?;
    let invite = typed_code(&runtime, &code, &identity)?;
    let named = runtime.block_on(qurb_peer::who(&invite, &identity, now()));
    Ok(PairingWith {
        number: invite.number_for(&identity.fingerprint()),
        name: named.as_ref().map(|(name, _)| name.clone()),
        kind: named.and_then(|(_, kind)| kind),
    })
}

/// A typed code, the long one or six digits found on this network
/// (decision 0066), as an invite.
fn typed_code(
    runtime: &tokio::runtime::Runtime,
    code: &str,
    identity: &qurb_peer::Identity,
) -> Result<qurb_peer::Invite, QurbError> {
    runtime.block_on(qurb_peer::short::resolve(code.trim(), identity)).map_err(|e| match e {
        qurb_peer::Error::BadInvite { detail } => QurbError::BadCode { detail },
        e @ (qurb_peer::Error::NoneOffering
        | qurb_peer::Error::WrongDigits
        | qurb_peer::Error::DigitsWithdrawn) => QurbError::Other { detail: sentence(&e.to_string()) },
        other => QurbError::Network { detail: other.to_string() },
    })
}

/// Who wants to pair with this phone, for the person to approve.
#[derive(Debug, Clone, uniffi::Record)]
pub struct PairingRequest {
    /// What it calls itself: a claim, which the number is there to check.
    pub name: String,
    /// `phone`, `computer` or `replica`, if it said.
    pub kind: Option<String>,
    /// The six digits it should be showing.
    pub number: String,
    /// Whether it asked for this phone's key, as a device with none does.
    pub wants_key: bool,
    /// Whether it is another person's device, asking to visit as a guest
    /// (decision 0060).
    pub guest: bool,
}

/// Asked, while this phone shows a code, whether to let a device in
/// (decision 0053). Implemented by the app: show who is asking and the number,
/// and answer. Called on a background thread, and may block until the person
/// answers.
#[uniffi::export(with_foreign)]
pub trait PairingApprover: Send + Sync {
    fn approve(&self, request: PairingRequest) -> bool;
}

/// How a vault's key is kept, for a device already set up.
#[uniffi::export]
pub fn protection_of(root: String) -> Result<String, QurbError> {
    let vault = Vault::at(&store_dir(Path::new(&root)));
    if !vault.exists() {
        return Err(QurbError::NotSetUp { detail: format!("{root} has no vault") });
    }
    Ok(vault.protection()?.as_str().to_string())
}

/// A vault, with the platform keystore attached if there is one.
fn vault_at(store_dir: &Path, keystore: Option<&Arc<dyn KeyStore>>) -> Vault {
    let vault = Vault::at(store_dir);
    match keystore {
        Some(store) => vault.using(Arc::new(PlatformStore(Arc::clone(store)))),
        None => vault,
    }
}

/// Why `text` is not a relay's address, or nothing if it has the shape of
/// one: `host:port`, a name or an address. For a settings screen to refuse a
/// mistyped setting as it is saved, by the rule the engine itself applies,
/// rather than have every sync find out later.
#[uniffi::export]
pub fn relay_address_problem(text: String) -> Option<String> {
    qurb_peer::relay_address_ok(&text).err()
}

/// Whether this directory has been set up.
#[uniffi::export]
pub fn is_set_up(root: String) -> bool {
    Vault::at(&store_dir(Path::new(&root))).exists()
}

/// Every exported method on `Qurb` lives in this one block.
///
/// Not a style preference. UniFFI keeps only the **last** `#[uniffi::export]`
/// impl block for an object and silently discards the others: the Rust
/// compiles, the bindings generate without a warning, and the methods from
/// every earlier block are simply absent from the Kotlin and Swift. Splitting
/// this block once cost eight methods, and nothing noticed until an app tried
/// to call them — the Rust tests reach these functions directly rather than
/// through the generated bindings, so they all kept passing.
#[uniffi::export]
impl Qurb {
    /// Open a store that has already been set up.
    ///
    /// `passphrase` is required only when the vault is passphrase-protected;
    /// pass `None` otherwise. On a phone the usual arrangement is no passphrase,
    /// because the app sandbox and the device's own lock screen already stand
    /// between the file and anyone else — see the crate README.
    #[uniffi::constructor]
    pub fn open(root: String, passphrase: Option<String>) -> Result<Self, QurbError> {
        Self::open_with(root, passphrase, Settings::default())
    }

    /// Open a vault whose key is in the platform's keystore.
    ///
    /// The `keystore` must be the same one the device was set up with. There is
    /// no fallback: a vault recorded as platform-protected and opened without
    /// one fails saying so, because the alternative is reading a key that is
    /// not there and reporting something less clear.
    #[uniffi::constructor]
    pub fn open_protected(
        root: String,
        keystore: Arc<dyn KeyStore>,
        settings: Settings,
    ) -> Result<Self, QurbError> {
        Self::open_inner(root, None, settings, Some(keystore))
    }

    /// Open, choosing where the services are and what this device is called.
    ///
    /// Separate from [`open`](Self::open) because most callers want the
    /// defaults and an app that lets the user point at their own rendezvous
    /// service needs this one.
    #[uniffi::constructor]
    pub fn open_with(
        root: String,
        passphrase: Option<String>,
        settings: Settings,
    ) -> Result<Self, QurbError> {
        Self::open_inner(root, passphrase, settings, None)
    }


    /// The directory being synced.
    pub fn root(&self) -> String {
        self.root.display().to_string()
    }

    /// Bring the index up to date with what is on disk.
    ///
    /// Needed at launch, and after the app has been suspended — neither
    /// platform delivers filesystem events to a process that was not running,
    /// so anything that changed in between produced no event at all.
    pub fn scan(&self) -> Result<ScanSummary, QurbError> {
        Ok(self.engine()?.reconcile()?.into())
    }

    /// Every live file in the folder, sorted by path: the shared area, and
    /// whatever was sent to this phone. The second kind is private, and it is
    /// still the phone's own file -- a list without it would hide exactly the
    /// thing somebody just sent.
    pub fn list(&self) -> Result<Vec<FileEntry>, QurbError> {
        self.page(0, u32::MAX)
    }

    /// A page of the same, in path order.
    ///
    /// For a screen that shows a large library a screenful at a time: every
    /// record crosses the boundary, and ten thousand of them at once is time a
    /// person spends looking at an empty list.
    pub fn page(&self, offset: u32, limit: u32) -> Result<Vec<FileEntry>, QurbError> {
        let engine = self.look()?;
        let rows = engine.store().db().folder_listing(limit as usize, offset as usize)?;
        Ok(rows.into_iter().map(FileEntry::from).collect())
    }

    /// Whether a path exists in the index.
    pub fn contains(&self, path: String) -> Result<bool, QurbError> {
        Ok(self.look()?.store().db().in_folder(&path)?.is_some())
    }

    /// One directory of the folder: the folders directly inside it and the
    /// files directly inside it, with where each file's bytes are. `dir` is
    /// relative to the root; empty is the root.
    ///
    /// From the index, not the disk. A file freed from this phone has no bytes
    /// in the folder and is still one of its files; a file browser that walked
    /// the directory simply lost it. The index is flat, so a folder is what the
    /// paths beneath it say it is.
    pub fn browse(&self, dir: String) -> Result<Directory, QurbError> {
        self.directory(dir, None)
    }

    /// The same, in one area only: the shared area for `private == false`,
    /// this phone's Private Vault for `true`. What the Files screen and the
    /// Private Vault each show -- a folder holding only private files is not
    /// a folder of the shared area.
    pub fn browse_in(&self, dir: String, private: bool) -> Result<Directory, QurbError> {
        self.directory(dir, Some(private))
    }

    /// One file, with where its bytes are; nothing if the path is not a file
    /// in the folder.
    pub fn entry(&self, path: String) -> Result<Option<FileEntry>, QurbError> {
        let path = qurb_watcher::normalize(path.trim_matches('/'));
        Ok(self.look()?.store().db().folder_entry(&path)?.map(FileEntry::from))
    }

    /// Files whose path contains `text`, ignoring case, at most `limit`.
    pub fn search(&self, text: String, limit: u32) -> Result<Vec<FileEntry>, QurbError> {
        let found = self.look()?.store().db().folder_search(&text, limit as usize)?;
        Ok(found.into_iter().map(FileEntry::from).collect())
    }

    /// The same, in one area only, as [`browse_in`](Self::browse_in).
    pub fn search_in(&self, text: String, limit: u32, private: bool) -> Result<Vec<FileEntry>, QurbError> {
        let found = self.look()?.store().db().folder_search(&text, 4 * limit as usize)?;
        Ok(found
            .into_iter()
            .filter(|e| e.private == private)
            .take(limit as usize)
            .map(FileEntry::from)
            .collect())
    }

    /// Take a file from `source` into the synced tree at `path`, in the area
    /// given: the shared area, or this phone's Private Vault. What *Add files*
    /// does in each, whatever *Keep new files private* says -- that setting is
    /// for files that arrive in the folder by other ways. A path already in
    /// qurb keeps its own area.
    pub fn import_into(&self, source: String, path: String, private: bool) -> Result<(), QurbError> {
        self.import(source, path, Some(private))
    }

    /// Write a stored file's contents to `destination`, a chunk at a time.
    ///
    /// The reason this takes a path instead of returning bytes: peak memory is
    /// one chunk, at most 2 MiB, however large the file is. A FileProvider
    /// extension asked for a 4 GB video survives this and would not survive
    /// being handed the bytes.
    ///
    /// Returns the number of bytes written. The content is verified against its
    /// hash, but only once the last byte has been written — so treat
    /// `destination` as incomplete until this returns, and do not move it into
    /// place before then.
    pub fn export(&self, path: String, destination: String) -> Result<u64, QurbError> {
        let engine = self.engine()?;
        let mut out = std::fs::File::create(&destination)
            .map_err(|e| QurbError::Storage { detail: format!("{destination}: {e}") })?;
        let written = engine.store().read_file_into(&path, &mut out)?;
        // The kernel is free to hold these pages until it feels like writing
        // them. On a phone the process may be suspended before it does.
        out.sync_all().map_err(|e| QurbError::Storage { detail: e.to_string() })?;
        Ok(written)
    }

    /// Take a file from `source` into the synced tree at `path`.
    ///
    /// Named `import_file` rather than `import` because `import` is a keyword
    /// in both Swift and Kotlin, and the generated binding would need escaping
    /// at every call site.
    ///
    /// The file is copied into the tree and indexed. `source` is left alone, so
    /// the caller can hand over a temporary file the system gave it and clean up
    /// afterwards as usual.
    ///
    /// A `source` that is already at `path` inside the tree is indexed where it
    /// lies rather than copied. Copying it would be `std::fs::copy` from a file
    /// onto itself, which truncates it to nothing — an easy call for an app to
    /// make, and a silent way to destroy the file it was trying to add.
    pub fn import_file(&self, source: String, path: String) -> Result<(), QurbError> {
        self.import(source, path, None)
    }

    /// Rename or move a file, keeping it in the area it is in. See
    /// [`qurb_storage::Store::rename_file`].
    pub fn rename(&self, from: String, to: String) -> Result<(), QurbError> {
        let (from, to) = (qurb_watcher::normalize(&from), qurb_watcher::normalize(to.trim_matches('/')));
        Ok(self.engine()?.store_mut().rename_file(&from, &to)?)
    }

    /// Make an empty folder, for files to be added or moved into. Nothing to
    /// sync until something is in it: the index knows files, not folders.
    pub fn make_folder(&self, path: String) -> Result<(), QurbError> {
        let path = qurb_watcher::normalize(path.trim_matches('/'));
        if path.is_empty() || !qurb_sync::is_safe_path(&path) || qurb_sync::sharing::is_rule_path(&path) {
            return Err(QurbError::NotFound { detail: format!("{path:?} is not a folder name that can be used") });
        }
        std::fs::create_dir_all(self.root.join(&path))
            .map_err(|e| QurbError::Storage { detail: e.to_string() })
    }

    /// Remove a file from the synced tree.
    ///
    /// Tombstoned rather than erased, so the deletion reaches other devices
    /// instead of looking to them like a file they should send back -- and
    /// kept in Recently deleted here, so it can be put back (decision 0042).
    pub fn remove(&self, path: String) -> Result<(), QurbError> {
        self.engine()?.store_mut().delete_to_trash(&qurb_watcher::normalize(&path), None)?;
        Ok(())
    }

    /// Delete a file of this phone's folder on a computer (decision 0073,
    /// item 3): this phone's own vault changes, and the computer follows at
    /// the next sync. It goes into this phone's Recently deleted, its bytes
    /// here or not: one freed to the computer is put back from the chunks
    /// the computer keeps as long (`Store::delete_to_trash_by`). A Private
    /// Vault file goes into the computer's as well, and restored there comes
    /// back here at the next sync (`Store::take_back_restored`). A backup -- of
    /// WhatsApp, or of a folder -- only once its original is gone from this
    /// phone: while it is here, the next backup would put the copy back, so
    /// that is refused with why. Never a file kept for this phone before it
    /// was set up again, which is another device's.
    pub fn delete_kept(&self, path: String) -> Result<(), QurbError> {
        let path = qurb_watcher::normalize(&path);
        let mut engine = self.engine()?;
        let me = engine.store().device_id()?;
        match engine.store().db().folder_row(&path)? {
            Some((_, Some(owner))) if owner == me => {}
            _ => return Err(QurbError::Refused { detail: "only this phone's own files can be deleted from here".into() }),
        }
        if engine.store().db().read_from_outside(&path)? {
            return Err(QurbError::Refused {
                detail: "it is still on this phone, and the next backup would bring it back".into(),
            });
        }
        engine.store_mut().delete_to_trash(&path, None)?;
        Ok(())
    }

    /// This device's own identity, in the short form a device's page shows
    /// for each of the others: what Settings shows, to compare with what
    /// another device says this phone is.
    pub fn identity_short(&self) -> Result<String, QurbError> {
        Ok(self.identity()?.fingerprint().short())
    }

    /// Devices this one trusts.
    pub fn peers(&self) -> Result<Vec<PeerInfo>, QurbError> {
        let engine = self.look()?;
        Ok(engine
            .store()
            .db()
            .trusted_peers()?
            .into_iter()
            .map(|p| {
                let fingerprint = qurb_peer::Fingerprint::from_bytes(p.fingerprint);
                PeerInfo {
                    fingerprint: hex(fingerprint.as_bytes()),
                    short: fingerprint.short(),
                    name: p.name,
                    paired_at: p.paired_at,
                    last_seen: p.last_seen,
                    relation: p.relation.as_str().into(),
                    kind: engine.store().db().peer_kind(&p.device_id).ok().flatten(),
                }
            })
            .collect())
    }

    /// Offer an invitation, for another device to scan or be read.
    ///
    /// The returned code carries this device's *full* fingerprint and must
    /// travel out of band — a QR code on the screen, or a code spoken aloud.
    /// Sending it over the network being paired would defeat the point: someone
    /// who can change what you see has already won.
    pub fn offer_pairing(&self) -> Result<Arc<Pairing>, QurbError> {
        let identity = self.identity()?;
        let runtime = self.runtime()?;

        // Inside the runtime even though `open` is not async: it binds a QUIC
        // endpoint, and quinn registers the socket with whatever reactor is
        // current. Without this it fails with "no async runtime found" — which
        // names the cause but not the fix, since nothing in the call is awaited.
        let host = {
            let _guard = runtime.enter();
            // Port zero, not the sync port: a background sync starting while a
            // code is on the screen would find that one taken. The invite
            // carries whatever port this gets.
            qurb_peer::PairingHost::open(
                "0.0.0.0:0".parse().expect("a literal address"),
                &identity,
                now(),
            )
            .map_err(|e| QurbError::Network { detail: e.to_string() })?
        };

        let invite = host.invite();
        Ok(Arc::new(Pairing {
            code: invite.encode(),
            human: invite.for_humans(),
            expires_at: invite.expires_at,
            digits: host.digits().map(|d| qurb_peer::short::spaced(&d)),
            digits_work: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            store: self.shared_store()?,
            name: self.device_name.clone(),
            runtime,
            inner: Mutex::new(Some(host)),
            cancelled: Arc::new(tokio::sync::Notify::new()),
            master: self.master.clone(),
        }))
    }

    /// Visit another person's computer as a guest, with the guest code it
    /// shows (decision 0060): this phone keeps its own key, shows the number
    /// from [`pairing_with`] while the person there approves, and from then
    /// on can send that computer files and be sent them.
    pub fn visit_computer(&self, code: String) -> Result<PeerInfo, QurbError> {
        let identity = self.identity()?;
        let runtime = self.runtime()?;
        let invite = typed_code(&runtime, &code, &identity)?;
        if !invite.guest {
            return Err(QurbError::BadCode {
                detail: "that code adds one of your own devices, not a visit as a guest".into(),
            });
        }
        self.visit_with(&invite, &identity, &runtime)
    }

    /// Accept an invitation offered by another device.
    ///
    /// The usual direction for a phone: the desktop shows a QR code and the
    /// phone's camera reads it, or the person types the six digits beside it
    /// (decision 0066). A guest code -- another person's computer -- is a
    /// visit, and the device it gives back is a host.
    pub fn join_pairing(&self, code: String) -> Result<PeerInfo, QurbError> {
        let identity = self.identity()?;
        let runtime = self.runtime()?;
        let invite = typed_code(&runtime, &code, &identity)?;
        if invite.guest {
            return self.visit_with(&invite, &identity, &runtime);
        }
        let store = self.shared_store()?;

        // The other device asks its person to approve this one, comparing the
        // number `pairing_with` gave this screen (decision 0053).
        let ours = qurb_peer::Ours { name: &self.device_name, kind: "phone", key: &self.master };
        let paired = runtime
            .block_on(qurb_peer::accept(&invite, &identity, store, &ours, now()))
            .map_err(pairing_failed)?;

        Ok(PeerInfo {
            fingerprint: hex(paired.fingerprint.as_bytes()),
            short: paired.fingerprint.short(),
            name: paired.name,
            paired_at: now(),
            last_seen: None,
            relation: "own".into(),
            kind: paired.kind,
        })
    }

    /// Sync with every trusted device, giving up after `seconds`.
    ///
    /// The deadline is the point. Both platforms hand a background task a
    /// window and kill it for outstaying one, so a sync that runs until it is
    /// finished is a sync that eventually gets the app's background privileges
    /// revoked. Running out of time is reported in
    /// [`SyncOutcome::timed_out`] and is not an error — work already applied
    /// stays applied, because each file is committed as it lands rather than at
    /// the end.
    ///
    /// Pass a generous value when the app is in the foreground and the user is
    /// watching; pass what the platform granted when it is not.
    pub fn sync_within(&self, seconds: u32) -> Result<SyncOutcome, QurbError> {
        let deadline = std::time::Duration::from_secs(seconds.max(1) as u64);
        self.sync_inner(deadline, deadline)
    }

    /// The same, then keep answering a device that is collecting from this
    /// one, up to `serving_seconds` from the start.
    ///
    /// A pass ends when its window does, and a device pulling a large file
    /// from the phone lost it there: an 800 MB video failed part-way, every
    /// time. For a caller the platform will let run longer -- a foreground
    /// worker -- this keeps the pass open while chunks are being asked for,
    /// and ends it [`COLLECTING`] after the last one went: [`PAUSED`] after,
    /// if what was being collected is not collected yet.
    pub fn sync_serving(&self, seconds: u32, serving_seconds: u32) -> Result<SyncOutcome, QurbError> {
        let deadline = std::time::Duration::from_secs(seconds.max(1) as u64);
        let serving = std::time::Duration::from_secs(serving_seconds as u64).max(deadline);
        self.sync_inner(deadline, serving)
    }

    /// What this device is handing to devices collecting from it. Takes no
    /// lock, so a screen can ask while a pass is running.
    pub fn serving(&self) -> Serving {
        Serving {
            bytes: self.serving.bytes.load(std::sync::atomic::Ordering::Relaxed),
            collecting: self.serving.recently(COLLECTING),
        }
    }

    /// Bytes a device reached would come and take from this one: sends not
    /// yet collected, shared files only this device has, and -- when a device
    /// keeps this one's files -- private ones not yet kept. What decides
    /// whether a sync is worth running as a long one.
    pub fn waiting_for_others_bytes(&self) -> Result<u64, QurbError> {
        Ok(self.for_others()?.1)
    }

    /// What this device made and nothing else has taken yet.
    ///
    /// Not "what failed to sync" — there is no queue of failed transfers,
    /// because there is no transfer to fail until the other device is
    /// reachable. The file is simply here, indexed and waiting, and this says
    /// which files those are. A share made with every other device switched
    /// off looks exactly like a share made with them on, until one answers.
    pub fn outstanding(&self) -> Result<Outstanding, QurbError> {
        let engine = self.look()?;
        let waiting = engine.store().undelivered()?;

        let bytes = waiting.iter().map(|(_, size)| size).sum();
        let db = engine.store().db();
        let files = waiting
            .into_iter()
            .map(|(path, size)| {
                let private = matches!(db.folder_row(&path), Ok(Some((_, Some(_)))));
                FileEntry { path, size, modified_at: 0, available: Available::OnlyHere, private }
            })
            .collect();
        Ok(Outstanding { files, bytes })
    }

    /// How much space the store occupies on this device.
    ///
    /// Both numbers, because on a phone the difference is the selling point:
    /// `logical` is what the files add up to, `on_disk` is what they cost after
    /// deduplication and compression.
    pub fn usage(&self) -> Result<Usage, QurbError> {
        let engine = self.look()?;
        let db = engine.store().db();
        // Live *files*, not chunks. Summing chunks would count shared content
        // once and report that three copies of a photo take up one photo's
        // worth of space, which is true of the disk and not of the library.
        Ok(Usage { logical: db.live_bytes()?, on_disk: engine.store().usage()?.total() })
    }

    /// What would be gone if this phone's data were cleared: files no other
    /// device is known to hold, largest first (decision 0053). What the screen
    /// Android opens in place of *Clear data* lists before anything goes.
    pub fn only_here(&self) -> Result<Vec<OnlyHere>, QurbError> {
        let engine = self.look()?;
        Ok(engine
            .store()
            .only_here()?
            .into_iter()
            .map(|(path, size, private)| OnlyHere { path, size, private })
            .collect())
    }

    /// Free what nothing needs: garbage past the retention window, and
    /// chunk-store copies of bytes the folder already holds.
    ///
    /// The same routine the desktop daemon runs every few minutes. Nothing on a
    /// phone ran it before, and a Galaxy S23 was measured holding 100.7 MB of
    /// chunks for 30.9 MB of files. Slow enough on a large store to belong off
    /// the main thread, like everything here.
    pub fn housekeep(&self) -> Result<Tidied, QurbError> {
        let done = self.engine()?.housekeep(qurb_engine::RETENTION)?;
        Ok(Tidied {
            freed: done.bytes_freed(),
            tombstones_expired: done.collected.tombstones_expired as u32,
        })
    }

    /// The bytes this phone keeps of files it sent that have arrived.
    ///
    /// A send's bytes are kept after delivery and are the first thing to go
    /// when space runs short (decision 0030). A desktop's storage cap says
    /// when that is. A phone has no cap, so nothing ever said it, and the
    /// phone kept every send it had made: 1.6 GiB for one video on the S23,
    /// on 2026-10-07, two days after the laptop took it. So the app shows
    /// this as its own line, and lets go of it only when asked by name
    /// ([`release_sent_copies`](Self::release_sent_copies)), never as part of
    /// freeing space in general. The laptop had since lost its copy of that
    /// video, and the phone's was probably the last.
    pub fn sent_copies(&self) -> Result<u64, QurbError> {
        // Which chunks, from the index, kept while it is unchanged: the query
        // walks every file's chunks, 0.7 to 0.9 s on the S23 each time
        // Settings was drawn (2026-10-15). Whether each is still stored is
        // asked of the chunk store every time, since freeing them is not a
        // change to the index alone.
        let engine = self.look()?;
        let store = engine.store();
        let chunks = self.remembered.answer("releasable", store.db().change_mark()?, || {
            Ok(store.db().releasable_held_chunks()?)
        })?;
        Ok(chunks.iter().filter(|h| store.cas().contains(h)).map(|h| store.cas().stored_size(h).unwrap_or(0)).sum())
    }

    /// Let go of [`sent_copies`](Self::sent_copies). Only sends the recipient
    /// is recorded as having, as under a desktop's cap. Returns the bytes
    /// freed.
    pub fn release_sent_copies(&self) -> Result<u64, QurbError> {
        Ok(self.engine()?.store_mut().release_held_payloads()?.bytes_reclaimed)
    }

    /// Free this phone's copy of a file another device keeps: *Free local
    /// space*. The file stays known, and `fetch` brings it back.
    ///
    /// Refused with `OnlyCopy` when no other device is known to hold it --
    /// the engine's check, not the app's, so no screen can skip it. Returns the
    /// bytes freed.
    pub fn free_local(&self, path: String) -> Result<u64, QurbError> {
        Ok(self.engine()?.store_mut().free_local(&qurb_watcher::normalize(&path))?)
    }

    /// Move a file into this phone's Private Vault, or out of it to every
    /// device (decision 0057). Into the vault, the person's other devices
    /// remove their copies at their next sync, and only a device keeping this
    /// phone's vault holds it from then on. Refused for a file whose bytes
    /// are not on this phone. Returns whether it moved.
    /// Out of it is refused on a phone out of the shared area (decision 0073).
    pub fn move_to_private(&self, path: String, private: bool) -> Result<bool, QurbError> {
        let mut engine = self.engine()?;
        if !private && !engine.shared_area() {
            return Err(QurbError::Refused { detail: "this phone keeps no shared files".into() });
        }
        Ok(engine.store_mut().move_area(&qurb_watcher::normalize(&path), private)?)
    }

    /// Ask for a freed file back. Acted on at the next sync with a device that
    /// has it, so asking while offline works. Returns whether it was freed at
    /// all -- asking for a file already here is not an error, and changes
    /// nothing.
    pub fn fetch(&self, path: String) -> Result<bool, QurbError> {
        Ok(self.engine()?.store().db().want(&qurb_watcher::normalize(&path))?)
    }

    /// Whether files added from now on go into this phone's own vault. Files
    /// already here stay where they are: an edit never moves a file between
    /// areas, and neither does this.
    pub fn set_own_files_private(&self, private: bool) -> Result<(), QurbError> {
        let mut engine = self.engine()?;
        // A phone out of the shared area files everything privately.
        let private = private || !engine.shared_area();
        engine.store_mut().set_new_files_private(private);
        Ok(())
    }

    /// Whether these are the recovery phrase's words at those positions: the
    /// check behind "confirm three of your words" when a phone is set up.
    ///
    /// Answered here, against the phrase derived from the key this handle
    /// already holds, so the app can let go of its copy of the words the moment
    /// it has drawn them rather than keep all 24 to compare against
    /// (decision 0033). The rule itself is `RecoveryPhrase::matches`, which the
    /// desktop uses too.
    pub fn phrase_matches(&self, answers: Vec<PhraseAnswer>) -> bool {
        let answers: Vec<(usize, &str)> =
            answers.iter().map(|a| (a.position as usize, a.word.as_str())).collect();
        self.master.to_phrase().matches(&answers)
    }

    /// The 24 words again, for writing out a new copy before the old one is
    /// lost. Derived from the key rather than kept anywhere: anyone who can
    /// open this handle can already read every file, so showing them the words
    /// gives away nothing they did not have (decision 0033).
    pub fn recovery_phrase(&self) -> String {
        self.master.to_phrase().to_string()
    }

    /// The devices that keep this phone's own files for it (decision 0036).
    /// Reach the computer `fingerprint`, which keeps this phone's folder, to
    /// browse it, within `seconds` (decision 0067, step 4). The computer has
    /// to be on and running Qurb.
    pub fn open_kept_folder(&self, fingerprint: String, seconds: u32) -> Result<Arc<KeptFolder>, QurbError> {
        let budget = std::time::Duration::from_secs(seconds.max(1) as u64);
        let (peer, name, trusted, meetings, me) = {
            let engine = self.engine()?;
            let db = engine.store().db();
            let peers = db.trusted_peers()?;
            let found = peers
                .iter()
                .find(|p| hex(&p.fingerprint) == fingerprint)
                .ok_or_else(|| QurbError::NotFound { detail: "no such device".into() })?;
            (
                qurb_peer::Fingerprint::from_bytes(found.fingerprint),
                found.name.clone(),
                qurb_peer::trusted_fingerprints(engine.store()).map_err(|e| QurbError::Storage { detail: e.to_string() })?,
                db.meetings().unwrap_or_default(),
                *engine.store().device_id()?.as_bytes(),
            )
        };
        let identity = self.identity()?;
        let runtime = self.runtime()?;
        let relay = runtime.block_on(qurb_peer::relay_for(self.relay.as_deref(), &self.signal_url));
        let trust = qurb_peer::tls::TrustList::new(trusted);
        // A port of its own: a sync pass may be using the usual one.
        let connector = runtime
            .block_on(async {
                tokio::time::timeout(
                    budget,
                    qurb_peer::Connector::start(
                        "0.0.0.0:0".parse().expect("a literal address"),
                        identity,
                        self.master.clone(),
                        &trust,
                        self.signal_url.clone(),
                        qurb_peer::Finding::everything(relay),
                    ),
                )
                .await
            })
            .map_err(|_| QurbError::Network { detail: format!("{name} did not answer in time") })?
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;
        {
            let _guard = runtime.enter();
            for (peer, secret) in meetings {
                connector.meet(qurb_peer::Fingerprint::from_bytes(peer.fingerprint), secret);
            }
        }
        let served = self.shared_store()?;
        let generation = qurb_peer::Generation::new();
        let mut accepting = Vec::new();
        for endpoint in [Some(connector.endpoint().clone()), connector.relay_endpoint().cloned()].into_iter().flatten() {
            let store = Arc::clone(&served);
            let generation = Arc::clone(&generation);
            let trust = trust.clone();
            let watcher = Arc::clone(&self.serving);
            accepting.push(runtime.spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let store = Arc::clone(&store);
                    let generation = Arc::clone(&generation);
                    let trust = trust.clone();
                    let watcher: Arc<dyn qurb_peer::Served> = watcher.clone();
                    tokio::spawn(async move {
                        if let Ok(connection) = incoming.await {
                            qurb_peer::server::serve_connection_watched(connection, store, generation, watcher, &trust)
                                .await;
                        }
                    });
                }
            }));
        }
        let connector = Arc::new(connector);
        // The timeout made inside the runtime: made outside, its timer has no
        // runtime to belong to and panics. It did, on the S23, every time
        // until 2026-10-15; the emulator showed the panic as "isn't reachable".
        let client = runtime
            .block_on(async { tokio::time::timeout(budget, connector.reach(peer)).await })
            .map_err(|_| QurbError::Network { detail: format!("{name} isn't reachable: it has to be on, with Qurb running") })?
            .map_err(|e| QurbError::Network { detail: format!("{name} isn't reachable: {e}") });
        let client = match client {
            Ok(client) => client,
            Err(e) => {
                for task in &accepting {
                    task.abort();
                }
                return Err(e);
            }
        };
        Ok(Arc::new(KeptFolder {
            runtime,
            client,
            _connector: connector,
            accepting,
            reader: std::sync::Mutex::new(self.open_store()?),
            name,
            me,
        }))
    }

    pub fn holders(&self) -> Result<Vec<PeerInfo>, QurbError> {
        let holders = self.look()?.store().db().holders()?;
        Ok(self
            .peers()?
            .into_iter()
            .filter(|p| self.device_of(&p.fingerprint).is_ok_and(|d| holders.contains(&d)))
            .collect())
    }

    /// Let a paired device keep this phone's own files. It is shown them from
    /// the next sync, keeps them where nobody using it sees them, and gives
    /// them back when asked.
    pub fn add_holder(&self, fingerprint: String) -> Result<(), QurbError> {
        let device = self.device_of(&fingerprint)?;
        Ok(self.engine()?.store().db().add_holder(&device)?)
    }

    /// Stop showing a device this phone's own files. What it already keeps it
    /// keeps; nothing here reaches into another device.
    pub fn remove_holder(&self, fingerprint: String) -> Result<(), QurbError> {
        let device = self.device_of(&fingerprint)?;
        self.engine()?.store().db().remove_holder(&device)?;
        Ok(())
    }

    /// Folders, and which devices each is shared with (decision 0044).
    pub fn sharing(&self) -> Result<Vec<SharedFolder>, QurbError> {
        let engine = self.look()?;
        let remote = engine.store().db().remote_folders()?;
        Ok(engine
            .store()
            .folder_sharing()?
            .into_iter()
            .map(|(folder, members)| SharedFolder {
                remote: remote.contains(&folder),
                folder,
                everyone: members.is_none(),
                members: members.unwrap_or_default().iter().map(|d| d.to_hex()).collect(),
            })
            .collect())
    }

    /// The devices a folder can be shared with: this phone first, then every
    /// paired device, each by the device id a rule names.
    pub fn share_targets(&self) -> Result<Vec<ShareTarget>, QurbError> {
        let engine = self.look()?;
        let store = engine.store();
        let mut out = vec![ShareTarget {
            id: store.device_id()?.to_hex(),
            name: "This phone".into(),
            here: true,
        }];
        for peer in store.db().trusted_peers()? {
            out.push(ShareTarget { id: peer.device_id.to_hex(), name: peer.name, here: false });
        }
        Ok(out)
    }

    /// Keep a folder on this phone only remotely (decision 0045): its files
    /// stay listed, local copies another device keeps are freed, and what
    /// changes elsewhere is not downloaded until opened.
    pub fn keep_remotely(&self, folder: String) -> Result<KeptRemotely, QurbError> {
        let (freed, bytes, kept) = self.engine()?.store_mut().keep_remotely(&folder)?;
        Ok(KeptRemotely { freed: freed as u32, bytes, kept })
    }

    /// Keep a folder on this phone again; everything in it is asked for.
    pub fn keep_locally(&self, folder: String) -> Result<u32, QurbError> {
        Ok(self.engine()?.store_mut().keep_locally(&folder)? as u32)
    }

    /// Share a folder with exactly these devices, or with every device when
    /// `members` is empty.
    pub fn set_sharing(&self, folder: String, members: Vec<String>) -> Result<(), QurbError> {
        let members: std::collections::BTreeSet<qurb_sync::DeviceId> = members
            .iter()
            .map(|hex| {
                qurb_sync::DeviceId::from_hex(hex)
                    .ok_or_else(|| QurbError::NotFound { detail: format!("not a device: {hex}") })
            })
            .collect::<Result<_, _>>()?;
        let mut engine = self.engine()?;
        match members.is_empty() {
            true => engine.store_mut().clear_sharing(&folder)?,
            false => engine.store_mut().set_sharing(&folder, &members)?,
        }
        Ok(())
    }

    /// Files two devices changed without either seeing the other (brief §24).
    pub fn conflicts(&self) -> Result<Vec<ConflictInfo>, QurbError> {
        let engine = self.look()?;
        let store = engine.store();
        let mut names = store.db().device_names()?;
        // Its own versions too: a conflict is as often with itself as with
        // the other device, and its id is not what anybody calls it.
        names.insert(store.device_id()?, "this phone".into());
        // A device never paired here has no name: "another device", as for a
        // version nobody recorded, rather than a code.
        let side = |v: qurb_storage::ConflictVersion| ConflictSide {
            by: v
                .modified_by
                .and_then(|id| names.get(&id).cloned())
                .unwrap_or_else(|| "another device".into()),
            size: v.size,
            here: v.here,
            changed_at: v.updated_at,
            path: v.path,
        };
        Ok(store
            .conflicts()?
            .into_iter()
            .map(|c| ConflictInfo { path: c.original_path, this: c.original.map(side), other: side(c.copy) })
            .collect())
    }

    /// Settle a conflict. `keep` is "this", "other" or "both". Returns the path
    /// of what was kept; the version not kept goes to Recently deleted.
    pub fn settle_conflict(&self, other: String, keep: String) -> Result<String, QurbError> {
        let keep = match keep.as_str() {
            "this" => qurb_storage::Keep::Original,
            "other" => qurb_storage::Keep::Copy,
            "both" => qurb_storage::Keep::Both,
            _ => return Err(QurbError::NotFound { detail: format!("keep this, other or both, not {keep}") }),
        };
        let mut engine = self.engine()?;
        let mut names = engine.store().db().device_names()?;
        // In a file name, this phone by the name other devices know it by.
        names.insert(engine.store().device_id()?, self.device_name.clone());
        let label = engine
            .store()
            .db()
            .folder_row(&other)?
            .and_then(|(row, _)| row.modified_by)
            .and_then(|id| names.get(&id).cloned())
            .unwrap_or_else(|| "other version".into());
        Ok(engine.store_mut().settle_conflict(&other, keep, &label)?)
    }

    /// What is in Recently deleted on this phone, most recent first
    /// (decision 0042).
    pub fn recently_deleted(&self) -> Result<Vec<DeletedFile>, QurbError> {
        let engine = self.look()?;
        let store = engine.store();
        let names = store.db().device_names()?;
        let me = store.device_id()?;
        Ok(store
            .recently_deleted()?
            .into_iter()
            .map(|entry| DeletedFile {
                id: entry.id,
                path: entry.path,
                size: entry.size,
                deleted_at: entry.deleted_at,
                deleted_by: entry.deleted_by.map(|id| match id == me {
                    true => "this phone".to_string(),
                    false => names.get(&id).cloned().unwrap_or_else(|| id.short()),
                }),
                why: entry.why,
                private: entry.scope == Some(me),
            })
            .collect())
    }

    /// Put a recently deleted file back, as a change made here, so it returns
    /// on every device. Returns where it went: its old path, or beside it when
    /// something is there now.
    pub fn restore_deleted(&self, id: i64) -> Result<String, QurbError> {
        Ok(self.engine()?.store_mut().restore_from_trash(id)?)
    }

    /// Delete a file in Recently deleted for good.
    pub fn forget_deleted(&self, id: i64) -> Result<(), QurbError> {
        Ok(self.engine()?.store_mut().forget_deleted(id)?)
    }

    /// What removing a paired device would do here, for the question asked
    /// before doing it. See [`qurb_storage::Store::removal_plan`].
    pub fn removal_plan(&self, fingerprint: String) -> Result<Removal, QurbError> {
        let device = self.device_of(&fingerprint)?;
        let plan = self.engine()?.store().removal_plan(&device)?;
        Ok(Removal {
            waiting: plan.waiting.len() as u32,
            kept: plan.kept_for_it.len() as u32,
            kept_bytes: plan.kept_for_it.iter().map(|(_, size)| size).sum(),
            only_there: plan.only_there,
            holds_ours: plan.holds_ours,
        })
    }

    /// Stop trusting a paired device: from the next sync it cannot connect to
    /// this phone or sync with it. It keeps its key and everything on it. See
    /// [`qurb_storage::Store::remove_device`].
    pub fn remove_device(&self, fingerprint: String, delete_kept: bool) -> Result<(), QurbError> {
        let device = self.device_of(&fingerprint)?;
        let name = self
            .peers()?
            .into_iter()
            .find(|p| p.fingerprint == fingerprint)
            .map(|p| p.name)
            .unwrap_or_default();
        self.engine()?.store_mut().remove_device(&device, &name, delete_kept)?;
        Ok(())
    }

    /// Send a file on this phone to one device and to nobody else, under
    /// `name`: a file in the folder, read from there when that device
    /// collects it (decision 0060). No copy is kept; a file changed or
    /// deleted before then is not sent, and the history says why. Returns the
    /// file's size.
    pub fn send_file(&self, source: String, name: String, to: String) -> Result<u64, QurbError> {
        self.send_from(&source, name, to, false)
    }

    /// Send a copy the app made of something lent only briefly -- a file from
    /// Android's share sheet -- deleting the copy once the other device has
    /// it (decision 0060).
    pub fn send_copy(&self, copy: String, name: String, to: String) -> Result<u64, QurbError> {
        self.send_from(&copy, name, to, true)
    }

    /// Send a document picked with the system's file picker, read from where
    /// it is through the [`DocumentOpener`] when the other device collects it
    /// (decision 0060). The app keeps its permission to read the document
    /// until then.
    pub fn send_document(&self, uri: String, name: String, to: String) -> Result<u64, QurbError> {
        self.send_from(&uri, name, to, false)
    }

    /// Computers this phone visits that asked, at the last sync, to open its
    /// folder there (decision 0060, step 5). The app shows each, behind the
    /// phone's screen lock.
    pub fn open_asks(&self) -> Vec<OpenAsk> {
        self.open_asks
            .lock()
            .map(|asks| {
                asks.iter()
                    .map(|(fingerprint, (name, _, own))| OpenAsk {
                        fingerprint: fingerprint.clone(),
                        name: name.clone(),
                        own: *own,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The person approved, behind the phone's screen lock: the folder's key
    /// goes to that computer at the next sync with it, where it is held in
    /// memory while the folder is open.
    pub fn approve_open(&self, fingerprint: String) -> Result<(), QurbError> {
        let ask = self.open_asks.lock().ok().and_then(|mut asks| asks.remove(&fingerprint));
        let Some((_, nonce, _)) = ask else {
            return Err(QurbError::NotFound { detail: "that computer is not asking any more".into() });
        };
        if let Ok(mut settled) = self.settled.lock() {
            settled.insert(fingerprint.clone(), nonce);
        }
        if let Ok(mut approved) = self.approved.lock() {
            approved.insert(fingerprint, nonce);
        }
        Ok(())
    }

    /// What computers answered to keys this phone sent since the last call:
    /// whether each opened the folder (decision 0060, step 5).
    pub fn open_answers(&self) -> Vec<OpenAnswer> {
        self.open_answers.lock().map(|mut answers| std::mem::take(&mut *answers)).unwrap_or_default()
    }

    /// The person said no: nothing is sent, and the computer's ask lapses.
    pub fn decline_open(&self, fingerprint: String) {
        let ask = self.open_asks.lock().ok().and_then(|mut asks| asks.remove(&fingerprint));
        if let (Some((_, nonce, _)), Ok(mut settled)) = (ask, self.settled.lock()) {
            settled.insert(fingerprint, nonce);
        }
    }

    /// How the engine opens documents it sends from where they are (decision
    /// 0060). Set by the app each time it opens the engine.
    pub fn set_document_opener(&self, opener: Arc<dyn DocumentOpener>) -> Result<(), QurbError> {
        let documents: Arc<dyn qurb_storage::Documents> = Arc::new(Lent(opener));
        if let Ok(mut held) = self.documents.lock() {
            *held = Some(documents.clone());
        }
        self.engine()?.store_mut().set_documents(documents);
        Ok(())
    }

    /// Bring the backup of WhatsApp's folder into line with a listing of it,
    /// for up to `seconds` (decision 0062). New and changed files are read
    /// once each, to describe them, and kept in this phone's vault where they
    /// lie; nothing is copied. A photo, video or document gone from WhatsApp
    /// stays, kept by the computer, when the computer has it; a chat backup
    /// WhatsApp replaced goes. WhatsApp's caches -- its folders whose names
    /// start with a dot -- are left out. A run that runs out of time says how
    /// many it left, and the next carries on.
    pub fn back_up_whatsapp(&self, files: Vec<OutsideFile>, seconds: u32) -> Result<BackupRun, QurbError> {
        let prefix = whatsapp_prefix();
        let listing: Vec<qurb_storage::OutsideEntry> = files
            .into_iter()
            .filter(|f| !f.path.split('/').any(|part| part.starts_with('.') || part.is_empty()))
            .map(|f| qurb_storage::OutsideEntry {
                path: f.path,
                source: f.uri,
                size: f.size,
                mtime_ns: f.modified_ms.saturating_mul(1_000_000),
            })
            .collect();
        let media = |path: &str| path.starts_with("Media/");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds.into());
        let mut run = BackupRun::default();
        self.keep_folder(&prefix, &listing, &media, deadline, &mut run)?;
        Ok(run)
    }


    /// This phone's folders, at any depth from `root`, its shared storage,
    /// with what is beneath each: what a computer keeping its vault is shown
    /// to choose from (decision 0063). Needs "All files access"; without it
    /// nothing can be listed, and the computer is told there is nothing.
    pub fn list_folders(&self, root: String) -> Result<u32, QurbError> {
        let folders = folder_summaries(Path::new(&root));
        self.engine()?.store().db().set_own_folders(&folders)?;
        Ok(folders.len() as u32)
    }

    /// Back up the folders chosen for this phone on a computer keeping its
    /// vault (decision 0063), each kept where it lies under `root`, as
    /// WhatsApp's is. A file deleted here stays on the computer. A run that
    /// runs out of time says how many it left, and the next carries on.
    ///
    /// Each ticked folder not inside another is walked once, taking the files
    /// whose folder is backed up: a folder unticked inside it is left out,
    /// and one ticked again deeper still is taken ([`qurb_storage::chosen`]).
    /// What was kept of a folder since unticked stays on the computer.
    pub fn back_up_chosen(&self, root: String, seconds: u32) -> Result<BackupRun, QurbError> {
        let choices = usable_choices(self.engine()?.store().db())?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds.into());
        let mut run = BackupRun::default();
        for folder in qurb_storage::chosen::roots(&choices) {
            if std::time::Instant::now() >= deadline {
                break;
            }
            let base = Path::new(&root).join(&folder);
            let mut listing = Vec::new();
            walk_files(&base, "", &mut listing);
            listing.retain(|file| {
                let inside = match file.path.rsplit_once('/') {
                    Some((dir, _)) => format!("{folder}/{dir}"),
                    None => folder.clone(),
                };
                qurb_storage::chosen::backed_up(&choices, &inside)
            });
            let prefix = format!("{}/storage/{folder}", qurb_storage::db::OUTSIDE);
            self.keep_folder(&prefix, &listing, &|_| true, deadline, &mut run)?;
        }
        Ok(run)
    }

    /// Every file beneath `dir`, listed directly, hidden names left out: for a
    /// folder kept where it lies, on a phone with "All files access"
    /// (decision 0063). One look at each entry; listed by the app instead,
    /// WhatsApp's 36,055 files took 24 s on the S23, against 4.7 s for all of
    /// its storage this way.
    pub fn list_directly(&self, dir: String) -> Vec<OutsideFile> {
        let mut found = Vec::new();
        walk_files(Path::new(&dir), "", &mut found);
        found
            .into_iter()
            .map(|f| OutsideFile {
                path: f.path,
                uri: f.source,
                size: f.size,
                modified_ms: f.mtime_ns / 1_000_000,
            })
            .collect()
    }

    /// The folders backed up from, chosen here or on a computer keeping this
    /// phone's vault (decision 0063): each ticked one not inside another.
    pub fn chosen_folders(&self) -> Result<Vec<String>, QurbError> {
        Ok(qurb_storage::chosen::roots(&usable_choices(self.look()?.store().db())?))
    }

    /// This phone's folders as last listed, at any depth, each with its mark
    /// in the tree and how much of it is backed up: for choosing on the phone
    /// as a tree (the owner's list of 2026-10-24, item 1).
    pub fn own_folders(&self) -> Result<Vec<OwnFolder>, QurbError> {
        let engine = self.look()?;
        let db = engine.store().db();
        let folders = db.own_folders()?;
        let listed: std::collections::HashSet<&str> = folders.iter().map(|f| f.path.as_str()).collect();
        // A choice of a folder no longer listed -- gone, or emptied -- would
        // mark the folder around it as partly chosen for good.
        let choices: Vec<_> = usable_choices(db)?.into_iter().filter(|c| listed.contains(c.folder.as_str())).collect();
        // What is directly in each folder, and of that what is backed up;
        // then each folder's share added to the folders around it, deepest
        // first, so that the sum is one pass however deep the tree.
        let mut direct: std::collections::HashMap<&str, u64> = folders.iter().map(|f| (f.path.as_str(), f.bytes)).collect();
        for folder in &folders {
            if let Some((parent, _)) = folder.path.rsplit_once('/') {
                if let Some(bytes) = direct.get_mut(parent) {
                    *bytes = bytes.saturating_sub(folder.bytes);
                }
            }
        }
        let mut chosen: std::collections::HashMap<&str, u64> = folders
            .iter()
            .map(|f| {
                let here = match qurb_storage::chosen::backed_up(&choices, &f.path) {
                    true => direct[f.path.as_str()],
                    false => 0,
                };
                (f.path.as_str(), here)
            })
            .collect();
        let mut deepest: Vec<&str> = folders.iter().map(|f| f.path.as_str()).collect();
        deepest.sort_by_key(|p| std::cmp::Reverse(p.matches('/').count()));
        for path in deepest {
            if let Some((parent, _)) = path.rsplit_once('/') {
                let share = chosen[path];
                if let Some(total) = chosen.get_mut(parent) {
                    *total += share;
                }
            }
        }
        Ok(folders
            .iter()
            .map(|f| OwnFolder {
                tick: qurb_storage::chosen::tick(&choices, &f.path).into(),
                chosen_bytes: chosen[f.path.as_str()],
                path: f.path.clone(),
                files: f.files,
                bytes: f.bytes,
            })
            .collect())
    }

    /// Choose, or stop choosing, one of this phone's folders to back up, here
    /// (item 10). The computer hears it at the next sync; the later choice
    /// wins wherever it was made.
    pub fn choose_folder(&self, folder: String, chosen: bool) -> Result<(), QurbError> {
        if !choosable(&folder) {
            return Err(QurbError::Other { detail: format!("{folder} is not one of this phone's folders") });
        }
        self.engine()?.store().db().choose_here(&folder, chosen, qurb_storage::db::now_ms())?;
        Ok(())
    }

    /// The chosen folders' backup as it stands: how much, and how much of it
    /// the computer has (decision 0063).
    pub fn folders_backup(&self) -> Result<BackupState, QurbError> {
        self.backup_state("folders", &format!("{}/storage", qurb_storage::db::OUTSIDE))
    }

    /// The WhatsApp backup as it stands: how much, and how much of it the
    /// computer has (decision 0062).
    pub fn whatsapp_backup(&self) -> Result<BackupState, QurbError> {
        self.backup_state("whatsapp", &whatsapp_prefix())
    }

    /// The push token this phone is woken by now, for the next pass to give
    /// the rendezvous service. Until 2026-10-18 the one read when the engine
    /// opened was given for the life of the app, so a token the push service
    /// reissued left the phone unwakeable until the app was next started:
    /// a rendezvous service now forgets a token the push service calls gone,
    /// and was handed the old one again on every pass.
    pub fn set_wake_token(&self, token: Option<String>) {
        if let Ok(mut held) = self.wake_token.lock() {
            *held = token.filter(|t| !t.is_empty());
        }
    }

    /// Whether this phone is on a network that does not count what it carries
    /// -- Wi-Fi, usually. Only then is the WhatsApp backup offered to the
    /// computer keeping it, which is 9.65 GB the first time on the S23
    /// (decision 0062). The app says so before each sync.
    pub fn set_unmetered(&self, unmetered: bool) -> Result<(), QurbError> {
        Ok(self.engine()?.store().db().set_outside_offered(unmetered)?)
    }

    /// Which of `sources` this phone has sent to `to` before, with the name
    /// each went under and when: asked before sending, so that the app can say
    /// so and the person choose (decision 0059). A send of the same file
    /// again is a new send, which the other device takes.
    pub fn sent_before(&self, sources: Vec<String>, to: String) -> Result<Vec<EarlierSend>, QurbError> {
        let device = self.device_of(&to)?;
        let found = self.look()?.store().sent_before(&sources, &device)?;
        Ok(found
            .into_iter()
            .map(|e| EarlierSend { source: e.source, sent_as: e.sent_as, at: e.at })
            .collect())
    }

    /// What this phone has sent that has not been collected yet.
    pub fn waiting(&self) -> Result<Vec<Waiting>, QurbError> {
        let peers = self.peers()?;
        let pending = self.look()?.store().pending_deliveries()?;
        Ok(pending
            .into_iter()
            .map(|(path, size, to)| {
                let peer = peers.iter().find(|p| self.device_of(&p.fingerprint).ok() == Some(to));
                Waiting {
                    path,
                    size,
                    to: peer.map(|p| p.name.clone()).unwrap_or_else(|| to.short()),
                    to_fingerprint: peer.map(|p| p.fingerprint.clone()).unwrap_or_default(),
                }
            })
            .collect())
    }

    /// Take back a send the other device has not collected yet. Refused once
    /// it has: the file is theirs then.
    pub fn cancel_send(&self, path: String, to: String) -> Result<(), QurbError> {
        let device = self.device_of(&to)?;
        Ok(self.engine()?.store_mut().cancel_send(&path, &device)?)
    }

    /// What happened, newest first. `before` pages backwards by id.
    pub fn history(&self, limit: u32, before: Option<i64>) -> Result<Vec<Happening>, QurbError> {
        let names = self.look()?.store().db().device_names()?;
        let rows = self.look()?.store().db().activity(limit as usize, before)?;
        Ok(rows
            .into_iter()
            .map(|r| Happening {
                id: r.id,
                at: r.at,
                kind: r.kind.as_str().to_string(),
                path: r.path,
                size: r.size,
                // A device since removed is named as it was when it was here.
                // One never paired here -- where a file began, reaching this
                // phone through another -- has no name, and is left unnamed
                // rather than shown as a code: the app says "another device".
                device: r.device.and_then(|id| names.get(&id).cloned()),
                detail: r.detail,
            })
            .collect())
    }
}

/// Not exported. `#[uniffi::export]` takes every function in the block it is
/// applied to, and neither a `MutexGuard` nor a four-argument constructor
/// taking an optional callback interface can cross an FFI boundary.
impl Qurb {
    /// Keep one folder where it lies, a few seconds at a time until
    /// `deadline`, adding what was done to `run` (decisions 0062 and 0063).
    fn keep_folder(
        &self,
        prefix: &str,
        listing: &[qurb_storage::OutsideEntry],
        keep_when_gone: &dyn Fn(&str) -> bool,
        deadline: std::time::Instant,
        run: &mut BackupRun,
    ) -> Result<(), QurbError> {
        let before = (run.added, run.changed, run.failed);
        let remaining_before = run.remaining;
        // Files that could not be read this run: skipped for the rest of it,
        // and tried again next time. Mostly ones WhatsApp moved or deleted in
        // the minute it took to list the folder -- ordinary, and on the S23
        // it stopped every run after the first at one such file.
        let mut unreadable = std::collections::HashSet::new();
        let mut remaining;
        loop {
            // A few seconds at a time, letting go of the engine in between, so
            // that the app is never kept waiting on it for a whole first run.
            let slice = (std::time::Instant::now() + std::time::Duration::from_secs(3)).min(deadline);
            let skip = |path: &str| unreadable.contains(path);
            let report =
                self.engine()?.store_mut().keep_outside_folder(prefix, listing, keep_when_gone, Some(slice), &skip)?;
            run.added += report.added as u32;
            run.changed += report.changed as u32;
            run.kept += report.kept as u32;
            run.deleted += report.deleted as u32;
            run.bytes_read += report.bytes_read;
            remaining = report.remaining as u32;
            run.failed += report.failed.len() as u32;
            if run.first_failure.is_none() {
                run.first_failure = report.failed.first().map(|(path, why)| format!("{path}: {why}"));
            }
            unreadable.extend(report.failed.into_iter().map(|(path, _)| path));
            if report.remaining == 0 || std::time::Instant::now() >= deadline {
                break;
            }
        }
        run.remaining = remaining_before + remaining;
        // Unchanged since before this run: each slice also counts the files
        // an earlier slice of it just read.
        let done = (run.added - before.0) + (run.changed - before.1) + remaining + (run.failed - before.2);
        run.unchanged += (listing.len() as u32).saturating_sub(done);
        Ok(())
    }

    /// One directory, from the index: every area, or one (see
    /// [`browse_in`](Self::browse_in)).
    fn directory(&self, dir: String, area: Option<bool>) -> Result<Directory, QurbError> {
        let dir = qurb_watcher::normalize(dir.trim_matches('/'));
        let entries = self.look()?.store().db().folder_entries_under(&dir)?;
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };

        let mut folders = std::collections::BTreeSet::new();
        let mut files = Vec::new();
        for entry in entries.into_iter().filter(|e| area.is_none_or(|private| e.private == private)) {
            let Some(rest) = entry.path.strip_prefix(&prefix) else { continue };
            match rest.split_once('/') {
                Some((folder, _)) => {
                    folders.insert(folder.to_string());
                }
                None => files.push(FileEntry::from(entry)),
            }
        }
        Ok(Directory { folders: folders.into_iter().collect(), files })
    }

    /// Copy `source` into the tree at `path` and index it: in the area given,
    /// or where *Keep new files private* says for `None`.
    ///
    /// A `source` that is already at `path` inside the tree is indexed where
    /// it lies rather than copied. Copying it would be `std::fs::copy` from a
    /// file onto itself, which truncates it to nothing — an easy call for an
    /// app to make, and a silent way to destroy the file it was trying to add.
    fn import(&self, source: String, path: String, private: Option<bool>) -> Result<(), QurbError> {
        // Nothing goes into a shared area this phone takes no part in.
        let private = match private {
            Some(false) if !self.engine()?.shared_area() => Some(true),
            other => other,
        };
        let logical = qurb_watcher::normalize(&path);
        let destination = self.root.join(&logical);

        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| QurbError::Storage { detail: e.to_string() })?;
        }

        // Copied in beside its destination, under a name the scan ignores,
        // and moved into place only with the engine held. Copied straight in,
        // a sync scanning the folder before the store below stored it first,
        // under *Keep new files private*: on 2026-10-08 a file added in a
        // shared folder on the S23 went into Private Vault.
        let staged = match same_file(Path::new(&source), &destination) {
            true => None,
            false => {
                let name = destination.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let staging = destination.with_file_name(format!(".{name}.adding.incoming"));
                if let Err(e) = std::fs::copy(&source, &staging) {
                    let _ = std::fs::remove_file(&staging);
                    return Err(QurbError::Storage { detail: format!("{source}: {e}") });
                }
                Some(staging)
            }
        };

        let mut engine = self.engine()?;
        if let Some(staging) = staged {
            if let Err(e) = std::fs::rename(&staging, &destination) {
                let _ = std::fs::remove_file(&staging);
                return Err(QurbError::Storage { detail: format!("{}: {e}", destination.display()) });
            }
        }
        let store = engine.store_mut();
        let was = store.new_files_private();
        if let Some(private) = private {
            store.set_new_files_private(private);
        }
        let stored = store.put_file(&logical, &destination);
        store.set_new_files_private(was);
        stored?;
        Ok(())
    }

    fn open_inner(
        root: String,
        passphrase: Option<String>,
        settings: Settings,
        keystore: Option<Arc<dyn KeyStore>>,
    ) -> Result<Self, QurbError> {
        // Every route into the engine passes through here, which is why it is
        // the one place that has to remember.
        logging();

        let store_dir = store_dir(Path::new(&root));
        let vault = vault_at(&store_dir, keystore.as_ref());

        if !vault.exists() {
            return Err(QurbError::NotSetUp { detail: format!("{root} has no vault") });
        }

        let master: MasterKey = vault.unlock(passphrase.as_deref())?;
        let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
        let master_chunk_key = chunk_key.clone();
        let root = PathBuf::from(&root);
        let mut store = Store::open(&store_dir, chunk_key)?;
        store.set_new_files_private(settings.own_files_private || !settings.shared_area);
        // What this phone tells devices that ask (decision 0053).
        let _ = store.db().set_local_kind("phone");
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);
        let mut engine = Engine::new(&root, store, ignore);
        engine.set_shared_area(settings.shared_area);
        if !settings.shared_area {
            // Out of the shared area (decision 0073, item 2). Each opening
            // finds less to do: the first lets go of what the computers keep
            // and moves the rest into the vault, leaving what is here and a
            // computer has not yet confirmed keeping, which the next sync
            // asks (`check_holders`); a later one finishes.
            match engine.store_mut().leave_shared_area() {
                Ok(left) if left != qurb_storage::LeftShared::default() => {
                    tracing::info!(dropped = left.dropped, moved = left.moved, kept = left.kept, "left the shared area");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "could not leave the shared area yet"),
            }
        }

        Ok(Self {
            inner: Mutex::new(engine),
            root,
            store_dir,
            master,
            device_name: settings.device_name,
            signal_url: settings.signal_url,
            relay: settings.relay,
            port: settings.port,
            discover: settings.discover,
            wake_token: Mutex::new(settings.wake_token),
            runtime: Mutex::new(None),
            serving: Arc::new(ServingStats::default()),
            documents: Mutex::new(None),
            open_asks: Mutex::new(std::collections::HashMap::new()),
            approved: Mutex::new(std::collections::HashMap::new()),
            open_answers: Mutex::new(Vec::new()),
            remembered: Remembered::default(),
            settled: Mutex::new(std::collections::HashMap::new()),
            reader: Mutex::new(None),
            chunk_key: master_chunk_key,
        })
    }
}


/// Not exported. `#[uniffi::export]` takes every method in the block it is
/// applied to, and a `MutexGuard` cannot cross an FFI boundary — nor should it.
impl Qurb {
    /// Visit another person's computer with a guest invite (decision 0060).
    fn visit_with(
        &self,
        invite: &qurb_peer::Invite,
        identity: &qurb_peer::Identity,
        runtime: &tokio::runtime::Runtime,
    ) -> Result<PeerInfo, QurbError> {
        let store = self.shared_store()?;
        let host = runtime
            .block_on(qurb_peer::visit(invite, identity, store, &self.device_name, "phone", now()))
            .map_err(pairing_failed)?;
        Ok(PeerInfo {
            fingerprint: hex(host.fingerprint.as_bytes()),
            short: host.fingerprint.short(),
            name: host.name,
            paired_at: now(),
            last_seen: None,
            relation: "host".into(),
            kind: Some("computer".into()),
        })
    }

    fn send_from(&self, source: &str, name: String, to: String, temporary: bool) -> Result<u64, QurbError> {
        let device = self.device_of(&to)?;
        let name = qurb_watcher::normalize(&name);
        if !qurb_sync::is_safe_path(&name) {
            return Err(QurbError::Other {
                detail: format!("{name:?} is not a name a file can be sent under"),
            });
        }
        let mut engine = self.engine()?;
        engine.store_mut().send_from(&name, source, temporary, &device)?;
        Ok(engine.store().db().live_row_in(&name, Some(&device))?.map(|r| r.size).unwrap_or(0))
    }

    /// The paired device with this fingerprint, as `peers` gives it: hex, in
    /// full. Only paired devices -- a fingerprint nobody paired with names
    /// nothing here.
    fn device_of(&self, fingerprint: &str) -> Result<qurb_sync::DeviceId, QurbError> {
        let unknown = || QurbError::NotFound { detail: format!("no paired device {fingerprint}") };
        let bytes: Vec<u8> = (0..fingerprint.len())
            .step_by(2)
            .map(|i| fingerprint.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
            .collect::<Option<_>>()
            .ok_or_else(unknown)?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| unknown())?;
        self.look()?
            .store()
            .db()
            .peer_by_fingerprint(&bytes)?
            .map(|p| p.device_id)
            .ok_or_else(unknown)
    }

    /// This device's network identity, loaded or created on first use.
    fn identity(&self) -> Result<qurb_peer::Identity, QurbError> {
        qurb_peer::Identity::load_or_create(&self.store_dir)
            .map_err(|e| QurbError::Storage { detail: e.to_string() })
    }

    /// A second handle on the store, for the parts of `qurb-peer` that serve
    /// requests from their own task.
    ///
    /// A separate connection rather than a share of the engine's: SQLite in WAL
    /// mode allows concurrent readers, and handing out the engine's own handle
    /// would mean a peer's read could block a local write behind one lock.
    fn shared_store(&self) -> Result<Arc<std::sync::Mutex<Store>>, QurbError> {
        Ok(Arc::new(std::sync::Mutex::new(self.open_store()?)))
    }

    /// A store that knows the folder its payloads live in.
    ///
    /// Every store this device opens has to, and there is more than one of
    /// them: the engine's, the one the peer server reads from to answer chunk
    /// requests, and the one a sync uses for content it already holds. A store
    /// opened without the folder finds no payload for any materialised file,
    /// so the peer server would answer "not found" for every chunk of every
    /// file this device is holding.
    fn open_store(&self) -> Result<Store, QurbError> {
        let key = self.engine()?.store().chunk_key();
        self.another_store(key)
    }

    /// A second handle on this phone's store, able to read what a send reads
    /// from where it is.
    fn another_store(&self, key: ChunkKey) -> Result<Store, QurbError> {
        let mut store = Store::open(&self.store_dir, key)?.in_tree(&self.root);
        if let Some(documents) = self.documents.lock().ok().and_then(|d| d.clone()) {
            store.set_documents(documents);
        }
        Ok(store)
    }

    /// The tokio runtime, built on first use.
    fn runtime(&self) -> Result<Arc<tokio::runtime::Runtime>, QurbError> {
        let mut slot = self
            .runtime
            .lock()
            .map_err(|_| QurbError::Other { detail: "the runtime is unusable".into() })?;

        if let Some(runtime) = slot.as_ref() {
            return Ok(Arc::clone(runtime));
        }

        // Two threads. Enough to overlap a transfer with the connection work
        // behind it, and few enough that a backgrounded app is not holding a
        // pool the platform would rather have back.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("qurb")
            .build()
            .map_err(|e| QurbError::Other { detail: format!("no runtime: {e}") })?;

        let runtime = Arc::new(runtime);
        *slot = Some(Arc::clone(&runtime));
        Ok(runtime)
    }

    /// One sync pass against every trusted peer, bounded by `budget`.
    fn sync_inner(
        &self,
        budget: std::time::Duration,
        serving: std::time::Duration,
    ) -> Result<SyncOutcome, QurbError> {
        let started = std::time::Instant::now();
        let mut outcome = SyncOutcome {
            reached: 0,
            unreachable: 0,
            adopted: 0,
            conflicts: 0,
            timed_out: false,
        };

        let peers: Vec<qurb_peer::Fingerprint> = {
            let engine = self.engine()?;
            qurb_peer::trusted_fingerprints(engine.store())
                .map_err(|e| QurbError::Storage { detail: e.to_string() })?
        };
        if peers.is_empty() {
            return Ok(outcome);
        }

        let identity = self.identity()?;
        let runtime = self.runtime()?;
        // Looked up on every pass: a phone moves between networks, and a
        // relay on a server of one's own is known by its name. A lookup that
        // fails costs this pass its fallback, not the pass -- a device on the
        // same network, or with a path that punches, is still reached. None
        // set is the relay beside the rendezvous service, at `/relay`: the
        // way round from a hotspot or a carrier, with nothing to configure.
        let relay = runtime.block_on(qurb_peer::relay_for(self.relay.as_deref(), &self.signal_url));

        // Built per pass rather than kept. A phone's address changes with every
        // move between Wi-Fi and cellular, and a connector holding a stale
        // public address announces somewhere nothing can reach.
        //
        // Inside the deadline like everything else. Starting means finding a
        // public address and connecting to the rendezvous service, each bounded
        // on its own, but together they can outlast a short window -- and a
        // window that closes before the phone could reach anybody is every
        // device unreachable, not a pass with work left over. See
        // `SyncOutcome::timed_out` for why the difference matters.
        // For the handshake and, again, for every request served this pass.
        let trust = qurb_peer::tls::TrustList::new(peers.clone());
        let started_connector = runtime.block_on(async {
            tokio::time::timeout(
                budget,
                qurb_peer::Connector::start(
                    format!("0.0.0.0:{}", self.port).parse().expect("a literal address"),
                    identity,
                    self.master.clone(),
                    &trust,
                    self.signal_url.clone(),
                    // Beacons only when this pass is allowed to look around at
                    // all. A phone syncing in a background window on a carrier
                    // network has nothing to discover locally and no time to
                    // spend finding that out.
                    match self.discover {
                        true => qurb_peer::Finding::everything(relay),
                        false => qurb_peer::Finding { stun: false, beacons: None, relay },
                    },
                ),
            )
            .await
        });
        let connector = match started_connector {
            Ok(started) => started.map_err(|e| QurbError::Network { detail: e.to_string() })?,
            Err(_) => {
                outcome.unreachable = peers.len() as u32;
                return Ok(outcome);
            }
        };
        // Computers this phone visits as a guest, met under a secret of their
        // own, since the rendezvous service matches this person's devices by
        // their key and another person's do not hold it (decision 0060).
        if let Ok(meetings) = self.engine()?.store().db().meetings() {
            let _guard = runtime.enter();
            for (peer, secret) in meetings {
                connector.meet(qurb_peer::Fingerprint::from_bytes(peer.fingerprint), secret);
            }
        }

        // Say how this device can be woken, so the service can poke it when
        // another device has something and this one is asleep. Registered on
        // every pass rather than once: push tokens are reissued, and the
        // service holds them in memory, so re-stating it costs one small
        // message and removes a whole class of "it stopped working".
        let wake = self.wake_token.lock().ok().and_then(|t| t.clone());
        if wake.is_some() {
            let _ = connector.reachable_via(wake);
        }

        // Answer as well as ask, for the length of this pass.
        //
        // A sync is two devices each dialling the other -- a QUIC handshake's
        // opening packets *are* the hole punch, so a device that only listens
        // has punched nothing and a device that only dials has nobody to reach.
        // Without this the two sides call each other simultaneously and both
        // hear silence.
        //
        // Unlike the desktop daemon, which serves continuously, this stops when
        // the pass does. That is what a phone wants: a background window is not
        // a licence to keep a socket open afterwards, and the platform will
        // suspend the process the moment the window closes regardless.
        let served = self.shared_store()?;
        let generation = qurb_peer::Generation::new();
        let mut accepting = Vec::new();
        for endpoint in [Some(connector.endpoint().clone()), connector.relay_endpoint().cloned()]
            .into_iter()
            .flatten()
        {
            let store = Arc::clone(&served);
            let generation = Arc::clone(&generation);
            let trust = trust.clone();
            let watcher = Arc::clone(&self.serving);
            accepting.push(runtime.spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let store = Arc::clone(&store);
                    let generation = Arc::clone(&generation);
                    let trust = trust.clone();
                    let watcher: Arc<dyn qurb_peer::Served> = watcher.clone();
                    tokio::spawn(async move {
                        if let Ok(connection) = incoming.await {
                            qurb_peer::server::serve_connection_watched(
                                connection, store, generation, watcher, &trust,
                            )
                            .await;
                        }
                    });
                }
            }));
        }

        // Every device at once, and each synced as soon as it answers.
        //
        // Reaching is mostly waiting -- for an introduction, for a hole punch,
        // for a switched-off device to fail to answer -- and waiting for them
        // one after another let the first absent device use the whole window:
        // with two paired and the first off, the second was never tried, pass
        // after pass, since the order never changes. Reaching is spawned, one
        // task per device, each bounded by what is left of the window; syncing
        // stays one at a time, because it holds the engine.
        let connector = Arc::new(connector);
        let mut reaching = tokio::task::JoinSet::new();
        for peer in peers.iter().copied() {
            let connector = Arc::clone(&connector);
            let left = budget.saturating_sub(started.elapsed());
            reaching.spawn_on(
                async move { (peer, tokio::time::timeout(left, connector.reach(peer)).await) },
                runtime.handle(),
            );
        }

        // Computers reached that may ask to open this phone's folder: asked
        // again while the pass stays to be collected from (`trade_asks`).
        let mut askers: Vec<(qurb_peer::Fingerprint, qurb_peer::PeerClient)> = Vec::new();
        while let Some(done) = runtime.block_on(reaching.join_next()) {
            let (peer, client) = match done {
                Ok((peer, Ok(Ok(client)))) => (peer, client),
                // A device that has not answered by the end of the window did
                // not answer: unreachable, not "ran out of time". The
                // difference is what the platform does next -- see
                // `SyncOutcome::timed_out` and decision 0020.
                Ok((_, Ok(Err(e)))) => {
                    tracing::debug!(error = %e, "peer unreachable");
                    outcome.unreachable += 1;
                    continue;
                }
                Ok((_, Err(_))) => {
                    tracing::debug!("peer did not answer before the time ran out");
                    outcome.unreachable += 1;
                    continue;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "reaching a peer failed");
                    outcome.unreachable += 1;
                    continue;
                }
            };

            let left = match budget.checked_sub(started.elapsed()) {
                Some(left) if !left.is_zero() => left,
                _ => {
                    outcome.timed_out = true;
                    continue;
                }
            };
            match self.sync_reached(&runtime, &client, peer, left) {
                Ok(Some(stats)) => {
                    outcome.reached += 1;
                    outcome.adopted += stats.adopted as u32;
                    outcome.conflicts += stats.conflicts as u32;
                    askers.push((peer, client));
                }
                // Ran out of time mid-peer. Whatever landed is already
                // committed; the rest is the next window's problem.
                Ok(None) => outcome.timed_out = true,
                Err(e) => {
                    tracing::debug!(error = %e, "peer went away while syncing");
                    outcome.unreachable += 1;
                }
            }
        }

        // Stay a moment for the others.
        //
        // Every device pulls what it wants, so what this phone has for another
        // device moves only when that device dials back and asks -- which it
        // does the moment it hears the phone, but not before the phone has
        // finished its own pulling. On a fast local network that finish comes
        // in under a second, and the pass used to end there: the phone had
        // closed its door by the time the desktop came for the photo. Found on
        // a Galaxy S23 whose desktop was chosen to keep its files and never
        // received one.
        //
        // So a pass that reached somebody and has something waiting for them
        // says so and keeps answering, until it has been collected or
        // `LINGER` has passed, and never beyond the window.
        //
        // And while a device is actually collecting -- a chunk went within
        // `COLLECTING` -- it keeps answering past the window, up to `serving`.
        // That is no longer than the window unless the caller asked through
        // `sync_serving`, which it does only where the platform will let it
        // run on.
        if stays_to_answer(
            outcome.reached > 0 && !outcome.timed_out,
            self.serving.recently(COLLECTING),
            || self.waiting_for_others().unwrap_or(false),
        ) {
            let come_by = come_by(std::time::Instant::now(), started, budget, serving);
            let at_most = started + serving;
            runtime.block_on(connector.announce_news());
            askers.retain(|(peer, _)| self.asker(peer).is_some());
            let mut looked = std::time::Instant::now();
            let mut excused = looked;
            let mut asked_at = looked;
            loop {
                let now = std::time::Instant::now();
                excused = excused_until(looked, now, excused);
                looked = now;
                if now.duration_since(asked_at) >= ASKS_EVERY {
                    asked_at = now;
                    for (peer, client) in askers.iter_mut() {
                        // Its connection may have closed in a long wait:
                        // reached again, briefly, or left to the next pass.
                        if client.is_closed() {
                            let connector = Arc::clone(&connector);
                            let peer = *peer;
                            match runtime.block_on(async { tokio::time::timeout(ASKS_WAIT, connector.reach(peer)).await }) {
                                Ok(Ok(again)) => *client = again,
                                _ => continue,
                            }
                        }
                        if let Some((known, key)) = self.asker(peer) {
                            self.trade_asks(&runtime, client, &known, key);
                        }
                    }
                }
                if !keep_answering(
                    now,
                    come_by,
                    at_most,
                    self.serving.recently(COLLECTING),
                    self.serving.recently(PAUSED) || now < excused,
                    || self.waiting_for_others().unwrap_or(false),
                ) {
                    break;
                }
                std::thread::sleep(LOOK_EVERY);
            }
        }

        for task in accepting {
            task.abort();
        }
        // A device that reached this one is on, as much as one this pass
        // reached: Home says a device is off by when it was last seen, and
        // a computer that collected through a relay while this phone could not
        // reach it was not off (`Served::asked_by`). Best effort, as is the
        // note for a device reached.
        let heard: Vec<[u8; 32]> =
            self.serving.asked_by.lock().map(|mut h| h.drain().collect()).unwrap_or_default();
        if !heard.is_empty() {
            if let Ok(engine) = self.engine() {
                for fingerprint in &heard {
                    let _ = engine.store().db().mark_peer_seen(fingerprint);
                }
            }
        }
        Ok(outcome)
    }

    /// Whether a device reached this pass would come and take something:
    /// a send it has not collected, a shared file only this device has, or --
    /// when a device keeps this phone's files -- one of those not yet kept.
    ///
    /// A private file with nobody chosen to keep it is waiting for no one, and
    /// staying open for it would spend every pass's spare seconds for nothing.
    fn waiting_for_others(&self) -> Result<bool, QurbError> {
        Ok(self.for_others()?.0 > 0)
    }

    /// How many files a reached device would come for, and their bytes.
    fn for_others(&self) -> Result<(usize, u64), QurbError> {
        let engine = self.look()?;
        let store = engine.store();
        let pending = store.pending_deliveries()?;
        let mut files = pending.len();
        let mut bytes: u64 = pending.iter().map(|(_, size, _)| *size).sum();

        let only_here = store.undelivered()?;
        let kept = !store.db().holders()?.is_empty();
        if !only_here.is_empty() {
            let db = store.db();
            for (path, size) in &only_here {
                let private = matches!(db.folder_row(path), Ok(Some((_, Some(_)))));
                if !private || kept {
                    files += 1;
                    bytes += size;
                }
            }
        }

        // The WhatsApp backup the computer does not have yet, while it is
        // offered: on Wi-Fi (decision 0062). What keeps this phone serving
        // until the computer has it all.
        if kept && store.db().outside_offered()? {
            let backup = store.db().outside_summary(&whatsapp_prefix())?;
            files += (backup.files - backup.held_files) as usize;
            bytes += backup.bytes - backup.held_bytes;
        }
        Ok((files, bytes))
    }

    /// The device behind `peer`, and what a yes would send it, if it may ask
    /// to open this phone's folder there. Takes the engine briefly.
    fn asker(&self, peer: &qurb_peer::Fingerprint) -> Option<(qurb_storage::db::TrustedPeer, [u8; 32])> {
        let engine = self.engine().ok()?;
        let known = engine.store().db().peer_by_fingerprint(peer.as_bytes()).ok().flatten()?;
        self.may_ask(engine.store(), &known).then(|| {
            let key = self.key_to_open(engine.store(), &known);
            (known, key)
        })
    }

    /// Whether `known` may ask to open this phone's folder there: a computer
    /// of another person's keeping it sealed (decision 0060), or one of this
    /// person's keeping its vault (decision 0067).
    fn may_ask(&self, store: &qurb_storage::Store, known: &qurb_storage::db::TrustedPeer) -> bool {
        match known.relation {
            qurb_storage::db::Relation::Host => true,
            qurb_storage::db::Relation::Own => store.db().is_holder(&known.device_id).unwrap_or(false),
            _ => false,
        }
    }

    /// What a yes sends `known`: the folder's key to a computer that keeps
    /// it sealed, and nothing to one of this person's, which holds the same
    /// key already, so the yes is all it needs.
    fn key_to_open(&self, store: &qurb_storage::Store, known: &qurb_storage::db::TrustedPeer) -> [u8; 32] {
        match known.relation {
            qurb_storage::db::Relation::Host => {
                qurb_storage::sealed::FolderKey::for_host(&store.chunk_key(), &known.device_id).to_bytes()
            }
            _ => [0; 32],
        }
    }

    /// With a computer that may ask (`may_ask`): send it the person's yes, if
    /// they gave one, and note whether it is asking, for the app to show.
    /// Done in each pass that reaches it, and again every `ASKS_EVERY` while
    /// a long pass stays to be collected from: somebody at that computer
    /// waits on the answer, and a long pass can last half an hour.
    fn trade_asks(
        &self,
        runtime: &tokio::runtime::Runtime,
        client: &qurb_peer::PeerClient,
        known: &qurb_storage::db::TrustedPeer,
        key: [u8; 32],
    ) {
        let own = known.relation == qurb_storage::db::Relation::Own;
        let fingerprint = hex(&known.fingerprint);
        let approved = self.approved.lock().ok().and_then(|mut a| a.remove(&fingerprint));
        if let Some(nonce) = approved {
            let sent = runtime.block_on(async { tokio::time::timeout(ASKS_WAIT, client.unlock(nonce, key)).await });
            match sent {
                Ok(Ok(opened)) => {
                    tracing::info!(opened, "answered an ask to open this phone's folder");
                    if let Ok(mut answers) = self.open_answers.lock() {
                        answers.push(OpenAnswer { name: known.name.clone(), opened });
                    }
                }
                // Not delivered: still approved, sent at the next chance.
                failed => {
                    let error = match failed {
                        Ok(Err(e)) => e.to_string(),
                        _ => "no answer in time".to_string(),
                    };
                    tracing::warn!(error, "could not send the yes to open this phone's folder");
                    if let Ok(mut held) = self.approved.lock() {
                        held.insert(fingerprint.clone(), nonce);
                    }
                }
            }
        }
        let asked = match runtime.block_on(async { tokio::time::timeout(ASKS_WAIT, client.asks()).await }) {
            Ok(Ok(asked)) => asked,
            failed => {
                let error = match failed {
                    Ok(Err(e)) => e.to_string(),
                    _ => "no answer in time".to_string(),
                };
                tracing::warn!(error, "could not ask whether a computer wants to open this phone's folder");
                // Not known either way: what was noted stands.
                return;
            }
        };
        let answered = self.settled.lock().ok().and_then(|s| s.get(&fingerprint).copied());
        tracing::debug!(asking = asked.is_some(), answered_before = asked.is_some() && asked == answered, "a visited computer's ask");
        if let Ok(mut asks) = self.open_asks.lock() {
            match asked {
                Some(nonce) if approved != Some(nonce) && answered != Some(nonce) => {
                    asks.insert(fingerprint, (known.name.clone(), nonce, own));
                }
                _ => {
                    asks.remove(&fingerprint);
                }
            }
        }
    }

    /// Sync against a peer already reached. `Ok(None)` means the time ran out.
    fn sync_reached(
        &self,
        runtime: &tokio::runtime::Runtime,
        client: &qurb_peer::PeerClient,
        peer: qurb_peer::Fingerprint,
        budget: std::time::Duration,
    ) -> Result<Option<qurb_engine::PlanStats>, QurbError> {
        // The timeouts are constructed *inside* the async blocks. `timeout`
        // needs a reactor when it is built, not when it is awaited, so building
        // one outside `block_on` panics with a message about being called from
        // outside a runtime -- which is true and reads like it is about the
        // future it wraps.
        let started = std::time::Instant::now();
        // The computer's list as kept from the last pass, if it says nothing
        // changed since (`qurb_peer::trees`): 9.8 MB on the S23 otherwise,
        // every pass, and more than a slow hotspot carried in one.
        let trees = qurb_peer::trees::Trees::at(self.store_dir.join("trees"));
        let tree = match runtime
            .block_on(async { tokio::time::timeout(budget, client.tree_kept(&peer, &trees)).await })
        {
            Ok(Ok(tree)) => tree,
            Ok(Err(e)) => return Err(QurbError::Network { detail: e.to_string() }),
            Err(_) => return Ok(None),
        };

        let mut engine = self.engine()?;
        // It answered, so say when: the Devices screen's "last reached" is
        // the first thing to look at when something has not arrived. Only the
        // desktop daemon used to write this down, so a phone said "not reached
        // yet" about a computer it had been syncing with for days. Best
        // effort: failing to note the time is no reason to abandon the sync.
        let _ = engine.store().db().mark_peer_seen(peer.as_bytes());
        let peer_device = engine
            .store()
            .db()
            .peer_by_fingerprint(peer.as_bytes())
            .ok()
            .flatten()
            .map(|p| p.device_id);
        // What kind of device it is, if it has not said: paired before
        // devices said so when pairing (decision 0053). Briefly, so a slow
        // answer does not spend the pass.
        if let Some(device) = &peer_device {
            if matches!(engine.store().db().peer_kind(device), Ok(None)) {
                let asked = runtime.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(2), client.about()).await
                });
                if let Ok(Ok(Some(kind))) = asked {
                    let _ = engine.store().learn_kind(device, &kind);
                }
            }
            // Which folders the person chose for this phone on a computer
            // keeping its vault (decision 0063): asked, never pushed. Kept, so
            // that a backup run with the computer away still knows. Briefly,
            // and a computer that cannot say leaves what was last said.
            if engine.store().db().is_holder(device).unwrap_or(false) {
                let asked = runtime.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(2), client.chosen()).await
                });
                // Taken where later than what was chosen here: the person
                // may have chosen on either (item 10).
                if let Ok(Ok(Some(theirs))) = asked {
                    let _ = engine.store().db().merge_choices_here(&theirs);
                }
            }
        }
        let plan = engine.plan_with(&tree, peer_device.as_ref())?;

        // Before the early return, not after: two devices that agree about
        // everything produce an empty plan every time, and those are exactly
        // the ones with holdings to report -- content that arrived before
        // there was any way to say so is content neither side will transfer
        // again.
        let reader = self.another_store(engine.store().chunk_key())?;
        if let Ok(Some(known)) = engine.store().db().peer_by_fingerprint(peer.as_bytes()) {
            runtime.block_on(qurb_peer::report_holdings(
                client,
                &reader,
                &known.device_id,
                &tree,
                64,
            ));
            // And the other way: what it is recorded as holding for files
            // freed here, asked a few at a time (decision 0055).
            runtime.block_on(qurb_peer::check_holders(client, &reader, &known.device_id, 16));
            // And what a computer keeping this phone's vault keeps that the
            // phone was not told of, a `Got` lost on the way: asked about, a
            // few each pass (`confirm_kept`).
            if engine.store().db().is_holder(&known.device_id).unwrap_or(false) {
                let learned = runtime.block_on(qurb_peer::confirm_kept(client, &reader, &known.device_id, 64));
                if learned > 0 {
                    tracing::info!(learned, "learned of files a computer keeps that this phone was not told of");
                }
            }
            // A computer of another person keeping this phone's vault, sealed:
            // what it keeps that this phone does not know of -- a phone set up
            // again -- and what was opened and is wanted back (decision 0060).
            let host = known.relation == qurb_storage::db::Relation::Host;
            if host {
                runtime.block_on(qurb_peer::learn_kept(client, engine.store_mut(), &known.device_id, &tree));
                runtime.block_on(qurb_peer::fetch_kept(client, engine.store_mut(), &known.device_id, &tree));
            }
            // Asked to open this phone's folder there (0060's step 5, 0067).
            if self.may_ask(engine.store(), &known) {
                let key = self.key_to_open(engine.store(), &known);
                self.trade_asks(runtime, client, &known, key);
            }
        }

        if plan.is_empty() {
            return Ok(Some(qurb_engine::PlanStats::default()));
        }

        if budget.checked_sub(started.elapsed()).is_none() {
            return Ok(None);
        }

        // The transfer itself is not interrupted once started. A partly applied
        // plan is a valid state -- every file is committed as it lands -- but
        // abandoning one mid-file would leave a staging file behind, and the
        // platform's patience is measured in seconds while a chunk is measured
        // in milliseconds.
        //
        // Inside `block_on` and then `block_in_place`: `NetworkSource` captures
        // the current runtime handle when it is built, and blocks on network
        // I/O for each chunk. `block_in_place` is what lets it do that without
        // stalling the whole scheduler, and it is only available on a worker
        // thread of a multi-threaded runtime -- which is why `runtime()` builds
        // one of those rather than a current-thread runtime.
        let stats = runtime.block_on(async {
            tokio::task::block_in_place(|| {
                let mut source = qurb_peer::NetworkSource::new(client, &reader).for_peer(peer_device);
                engine.apply_plan(&plan, &mut source)
            })
        })?;

        Ok(Some(stats))
    }

    /// A backup's summary, kept while the index is unchanged: it reads every
    /// row of the backup, 36,062 of them for the S23's WhatsApp, and Home
    /// asks on every refresh. Computed each time, it held the engine 1.2 s
    /// on the S23, and every screen waited behind it (2026-10-15).
    fn backup_state(&self, what: &'static str, prefix: &str) -> Result<BackupState, QurbError> {
        let engine = self.look()?;
        let db = engine.store().db();
        self.remembered.answer(what, db.change_mark()?, || {
            let summary = db.outside_summary(prefix)?;
            Ok(BackupState {
                files: summary.files,
                bytes: summary.bytes,
                held_files: summary.held_files,
                held_bytes: summary.held_bytes,
                only_kept: summary.only_kept,
            })
        })
    }

    /// The index to read, without the engine: a connection of its own, which
    /// SQLite lets read while the engine's writes, so a screen is answered
    /// while a sync holds the engine. Until 2026-10-19 every screen came
    /// through `engine`, and on the S23 a 200 MB file arriving held it 64
    /// seconds, with Devices and Home waiting up to 56 of them. Only for
    /// calls that change nothing: a change still goes through the engine,
    /// one at a time, so a scan never runs while a transfer places a file.
    #[track_caller]
    fn look(&self) -> Result<Looking<'_>, QurbError> {
        let mut guard = self
            .reader
            .lock()
            .map_err(|_| QurbError::Other { detail: "the index reader failed earlier".into() })?;
        if guard.is_none() {
            *guard = Some(self.another_store(self.chunk_key.clone())?);
        }
        Ok(Looking { guard })
    }

    /// The engine, held until the guard goes. Every call that changes the
    /// index comes through here, one at a time, so a slow one holds up every
    /// call waiting behind it. A wait or a hold that runs
    /// long is logged with where in this file it was taken, which is how the
    /// S23's first minute after opening the app was taken apart (2026-10-15).
    #[track_caller]
    fn engine(&self) -> Result<Held<'_>, QurbError> {
        let at = std::panic::Location::caller();
        let asked = std::time::Instant::now();
        // A poisoned lock means an earlier call panicked while holding it. The
        // engine's state is then unknown, so this reports rather than recovers.
        let guard = self.inner.lock().map_err(|_| QurbError::Other {
            detail: "the engine is in an unknown state after an earlier failure".into(),
        })?;
        let waited = asked.elapsed();
        if waited >= WAITED_LONG {
            tracing::info!(waited_ms = waited.as_millis() as u64, at = at.line(), "waited for the engine");
        }
        Ok(Held { guard, since: std::time::Instant::now(), at })
    }
}

/// Answers kept while the index is unchanged (`Db::change_mark`): a summary
/// that reads every row is computed once per change rather than once per
/// look.
#[derive(Default)]
struct Remembered {
    answers: Mutex<std::collections::HashMap<&'static str, Kept>>,
}

/// One kept answer: the index's mark when it was worked out, and the answer.
type Kept = ((u64, i64), Arc<dyn std::any::Any + Send + Sync>);

impl Remembered {
    /// The answer to `what` as of `version`: the one kept, if the index has
    /// not changed since, or `work`'s, kept for next time.
    fn answer<T: Clone + Send + Sync + 'static>(
        &self,
        what: &'static str,
        version: (u64, i64),
        work: impl FnOnce() -> Result<T, QurbError>,
    ) -> Result<T, QurbError> {
        if let Ok(answers) = self.answers.lock() {
            if let Some((at, kept)) = answers.get(what) {
                if *at == version {
                    if let Some(kept) = kept.downcast_ref::<T>() {
                        return Ok(kept.clone());
                    }
                }
            }
        }
        let fresh = work()?;
        if let Ok(mut answers) = self.answers.lock() {
            answers.insert(what, (version, Arc::new(fresh.clone())));
        }
        Ok(fresh)
    }
}

/// The engine, held: the lock's guard, and where and when it was taken.
/// The index, read through its own connection (`Qurb::look`).
struct Looking<'a> {
    guard: std::sync::MutexGuard<'a, Option<Store>>,
}

impl Looking<'_> {
    fn store(&self) -> &Store {
        self.guard.as_ref().expect("opened by look")
    }
}

struct Held<'a> {
    guard: std::sync::MutexGuard<'a, Engine>,
    since: std::time::Instant,
    at: &'static std::panic::Location<'static>,
}

impl std::ops::Deref for Held<'_> {
    type Target = Engine;
    fn deref(&self) -> &Engine {
        &self.guard
    }
}

impl std::ops::DerefMut for Held<'_> {
    fn deref_mut(&mut self) -> &mut Engine {
        &mut self.guard
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        let held = self.since.elapsed();
        if held >= HELD_LONG {
            tracing::info!(held_ms = held.as_millis() as u64, at = self.at.line(), "held the engine");
        }
    }
}

/// A wait for the engine worth a line in the log: a screen waiting this long
/// has visibly not answered.
const WAITED_LONG: std::time::Duration = std::time::Duration::from_millis(250);

/// A hold worth a line: everything else that wants the engine waits this long.
const HELD_LONG: std::time::Duration = std::time::Duration::from_millis(500);

/// The longest a pass stays open after its own syncing, for devices it
/// reached to come back for what it has for them. Long enough for a daemon
/// that hears the phone to dial back and pull a few photos; short enough that
/// a device which never comes costs a background window little.
const LINGER: std::time::Duration = std::time::Duration::from_secs(10);

/// How often a long pass, staying to be collected from, asks the computers it
/// reached whether they want this phone's folder opened. Somebody at the
/// computer is waiting: on 2026-10-15 a laptop's ask sat unheard behind a
/// WhatsApp backup the S23 served for the half hour such a pass may last.
const ASKS_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

/// How long one ask, or one yes, is waited for.
const ASKS_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether a pass that has finished its own syncing stays to answer at all.
///
/// A device collecting from this one is served whether or not this pass
/// reached it. The pass dials out; the device collecting may have dialled in,
/// and a laptop whose firewall refuses inbound connections can always be the
/// one dialling. On 2026-10-05 that laptop was 18 seconds into collecting a
/// 512 MiB file when the phone's pass, having reached nobody itself, ended at
/// its window; it finished only because the app was on screen. Otherwise a
/// pass stays only when it reached a device and has something waiting for it.
fn stays_to_answer(reached: bool, collecting: bool, waiting: impl FnOnce() -> bool) -> bool {
    collecting || (reached && waiting())
}

/// Until when a pass that has finished its own syncing waits for a device to
/// come and collect, when none has yet.
///
/// An ordinary pass gives it `LINGER`, inside its window. A long pass -- one
/// the platform lets run on past the window -- gives it as long as a device
/// that paused part-way is given, `PAUSED`. The collector may be busy: on
/// 2026-10-13 the laptop was still timing out its connection to the phone's
/// previous process, 58 seconds after the app was replaced, and two long
/// passes in a row had waited their ten seconds and closed before it came.
fn come_by(
    now: std::time::Instant,
    started: std::time::Instant,
    budget: std::time::Duration,
    serving: std::time::Duration,
) -> std::time::Instant {
    match serving > budget {
        true => std::cmp::min(started + serving, now + PAUSED),
        false => std::cmp::min(started + budget, now + LINGER),
    }
}

/// How often a pass that is answering looks again at whether to go on.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

/// A look that comes this much later than asked means the process was not
/// running in between.
const FROZEN: std::time::Duration = std::time::Duration::from_secs(3);

/// Until when a collector's silence is excused, given when this pass last
/// looked and when it looks now.
///
/// A pass that was frozen answered nobody, so a collector that sent nothing
/// meanwhile says nothing about whether it is still there -- and its
/// connection may have timed out waiting for this one. After a freeze it is
/// given as long as one that paused part-way, `PAUSED`, to come back.
///
/// Samsung freezes an app in a foreground service while the screen is on
/// (`FreecessHandler.handleLcdOnFreeze`). On the S23 on 2026-10-13 a long pass
/// ended at 20:33:52 with 10,408 files of the WhatsApp backup still to go:
/// thawed, it found nothing had gone for a minute, and concluded the laptop
/// had left.
fn excused_until(
    looked: std::time::Instant,
    now: std::time::Instant,
    excused: std::time::Instant,
) -> std::time::Instant {
    match now.duration_since(looked) >= FROZEN {
        true => std::cmp::max(excused, now + PAUSED),
        false => excused,
    }
}

/// Whether a pass that has finished its own syncing goes on answering.
///
/// Never past `at_most`. Before that, while a device is collecting; and
/// otherwise while something is still waiting to be collected and either
/// there is time left for a device to come for it (`come_by`) or one was
/// collecting a moment ago (`paused`) and may be back. `waiting` is asked only
/// when nobody is collecting: it reads the index, and the answer does not
/// matter while chunks are going.
fn keep_answering(
    now: std::time::Instant,
    come_by: std::time::Instant,
    at_most: std::time::Instant,
    collecting: bool,
    paused: bool,
    waiting: impl FnOnce() -> bool,
) -> bool {
    now < at_most && (collecting || ((now < come_by || paused) && waiting()))
}

/// How recently a chunk must have gone for a device to count as still
/// collecting. Long enough to cover a pause between files and a slow chunk on
/// a poor link; short enough that a device that has finished, or gone, lets
/// the pass end soon after.
const COLLECTING: std::time::Duration = std::time::Duration::from_secs(10);

/// How long a device that stopped collecting part-way is waited for, while
/// what it was collecting is still waiting.
///
/// A collector pauses: its program restarts and re-reads what it already has,
/// its Wi-Fi drops, the laptop's lid opens again. On 2026-10-05 a laptop that
/// restarted mid-transfer took twelve seconds to ask again, and the phone,
/// having served nothing for `COLLECTING`, had already closed its pass. A
/// minute covers that and the laptop's half-minute retry; a device gone for
/// longer is not coming back to this pass. Only while something is still
/// waiting: a collection that finished ends the pass `COLLECTING` after.
const PAUSED: std::time::Duration = std::time::Duration::from_secs(60);

/// Where the store lives inside the synced root.
///
/// Matches the desktop layout exactly, so the same directory can be opened by
/// either. That matters for testing more than for users: a store produced on a
/// phone must be inspectable with the `qurb` command.
fn store_dir(root: &Path) -> PathBuf {
    root.join(".qurb")
}

/// Whether two paths name the same file on disk.
///
/// Compared after canonicalisation rather than as strings, so a symlink, a
/// `..`, or a differently-spelled but equivalent path is still recognised.
/// A path that does not exist cannot be the same file as one that does, so a
/// failure to canonicalise answers `false`.
/// Lowercase hex. The form a fingerprint takes when it crosses the boundary,
/// because a 32-byte array is awkward in both target languages and a string is
/// what an app puts in a list or a log.
fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The other way: 64 hex digits as 32 bytes.
fn unhex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Unix seconds.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Networking
// ---------------------------------------------------------------------------
//
// Everything below is what turns the phone from a local encrypted file store
// into a device that syncs. Three things about a phone shape it, and none of
// them apply to the desktop daemon:
//
// **The network changes under you.** Wi-Fi to cellular, cellular to nothing, a
// new address on every transition. The desktop daemon holds one long-lived
// connector because a laptop's address is stable for hours. Here a connector is
// built per sync pass, so each pass discovers the address the device has *now*
// rather than the one it had when the app launched.
//
// **Time is rationed.** iOS `BGTaskScheduler` grants short, unpredictable
// windows and kills a process that outstays one; Android's `WorkManager` is
// more generous and still finite. So the entry point that matters is not "sync"
// but "sync for at most this many seconds, and stop cleanly" — which is what
// `sync_within` is.
//
// **Calls block.** Same contract as the rest of this crate: the platform runs
// them off the main thread.

/// One trusted device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PeerInfo {
    /// Hex fingerprint. The identity; everything else here is decoration.
    pub fingerprint: String,
    /// Short form, for showing to a person.
    pub short: String,
    /// What the peer calls itself. Chosen by the peer, so display-only — never
    /// used to decide anything.
    pub name: String,
    /// Unix seconds.
    pub paired_at: i64,
    pub last_seen: Option<i64>,
    /// "own", one of this person's devices; "guest", another person's
    /// visiting this one; or "host", another person's computer this phone
    /// visits (decision 0060).
    pub relation: String,
    /// "phone", "computer" or "replica", as the device said (decision 0053):
    /// what its icon is. None for a device paired before then that has not
    /// synced since.
    pub kind: Option<String>,
}

/// A pairing that did not happen, said as what it was. A refusal -- different
/// keys, the other person declining, a code that lapsed before anybody
/// approved -- is not the network, and was filed under it: on 2026-10-11 a
/// key mismatch read "Network problem." on the phone. Said as a sentence, since
/// the engine words its errors to follow "error:" on a command line.
fn pairing_failed(e: qurb_peer::Error) -> QurbError {
    use qurb_peer::Error as E;
    match e {
        E::DifferentKey
        | E::Declined
        | E::PairingRefused
        | E::PairingAbandoned
        | E::NotApprovedInTime
        | E::NoKeyGiven
        | E::InviteExpired
        | E::SetUpFailed { .. } => QurbError::Other { detail: sentence(&e.to_string()) },
        other => QurbError::Network { detail: other.to_string() },
    }
}

/// "these two devices differ" as "These two devices differ."
fn sentence(said: &str) -> String {
    let mut chars = said.chars();
    match chars.next() {
        Some(first) => format!("{}{}.", first.to_uppercase(), chars.as_str().trim_end_matches('.')),
        None => String::new(),
    }
}

/// A pairing code as a QR code: `width` modules a side, row by row, `true`
/// for dark. No quiet zone: the screen adds its own margin at the size it
/// draws, and a scanner needs one (four modules) to find the code at all.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QrCode {
    pub width: u32,
    pub dark: Vec<bool>,
}

/// This engine's version, the protocol it speaks to other devices, and the
/// index schema it writes (decision 0047).
#[uniffi::export]
pub fn engine_version() -> String {
    format!(
        "engine {}, protocol {}, index schema {}",
        env!("CARGO_PKG_VERSION"),
        String::from_utf8_lossy(qurb_peer::tls::ALPN),
        qurb_storage::db::SCHEMA_VERSION
    )
}

/// Encode `text` -- a pairing code -- as a QR code. Low error correction, as
/// on the desktop: it is read off a screen, and lower correction means larger
/// modules at a given size.
#[uniffi::export]
pub fn qr_code(text: String) -> Result<QrCode, QurbError> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::L)
        .map_err(|e| QurbError::Other { detail: format!("cannot draw that code: {e}") })?;
    Ok(QrCode {
        width: code.width() as u32,
        dark: code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect(),
    })
}

/// A folder and who it is shared with. See [`Qurb::sharing`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SharedFolder {
    pub folder: String,
    /// Shared with every device: no rule.
    pub everyone: bool,
    /// Device ids, in hex, when not everyone.
    pub members: Vec<String>,
    /// Kept on this phone only remotely (decision 0045).
    pub remote: bool,
}

/// What keeping a folder remotely did. See [`Qurb::keep_remotely`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct KeptRemotely {
    pub freed: u32,
    pub bytes: u64,
    /// Files kept because this phone has the only copy.
    pub kept: Vec<String>,
}

/// A device a folder can be shared with. See [`Qurb::share_targets`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ShareTarget {
    pub id: String,
    pub name: String,
    /// This phone.
    pub here: bool,
}

/// One side of a conflict. See [`Qurb::conflicts`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConflictSide {
    pub path: String,
    pub size: u64,
    /// Whether its bytes are on this phone.
    pub here: bool,
    /// The name of the device that made this version.
    pub by: String,
    /// Unix seconds.
    pub changed_at: i64,
}

/// Two versions of one file, both kept. See [`Qurb::conflicts`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConflictInfo {
    /// The file's own name.
    pub path: String,
    /// The version under it, unless that has since gone.
    pub this: Option<ConflictSide>,
    pub other: ConflictSide,
}

/// A file in Recently deleted. See [`Qurb::recently_deleted`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeletedFile {
    pub id: i64,
    /// Where it was.
    pub path: String,
    pub size: u64,
    /// Unix seconds.
    pub deleted_at: i64,
    /// "this phone", or the name of the device whose deletion it was.
    pub deleted_by: Option<String>,
    /// Why, where "deleted" is not the whole story.
    pub why: Option<String>,
    /// Deleted from this phone's Private Vault, which is where restoring puts
    /// it back: on this phone, not on every device.
    pub private: bool,
}

/// What removing a device would do. See [`Qurb::removal_plan`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Removal {
    /// Sends it has not collected, which removing it cancels.
    pub waiting: u32,
    /// Files this phone keeps for it, and their total size.
    pub kept: u32,
    pub kept_bytes: u64,
    /// Files freed from this phone that only it keeps: once it is removed
    /// they cannot be fetched back.
    pub only_there: Vec<String>,
    /// Whether it keeps this phone's own files.
    pub holds_ours: bool,
}

/// What a sync pass did.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SyncOutcome {
    /// Peers that answered.
    pub reached: u32,
    /// Peers that did not. Not an error: a phone syncs against devices that are
    /// asleep most of the time, and that is the normal case rather than a fault.
    pub unreachable: u32,
    /// Files taken from a peer.
    pub adopted: u32,
    /// Conflicts, each of which left both versions on disk.
    pub conflicts: u32,
    /// Whether the pass ran out of time before finishing: a peer that
    /// answered was still being synced, or peers were left untried.
    ///
    /// Not a failure. It means the next window has work to do, and the platform
    /// side should schedule one rather than report a problem. A peer that never
    /// answered is counted in `unreachable` instead, however long it was waited
    /// for.
    pub timed_out: bool,
}

/// A place in this phone's folder on a computer (decision 0067, step 4).
#[derive(Debug, Clone, uniffi::Record)]
pub struct KeptListing {
    pub folders: Vec<String>,
    pub files: Vec<KeptFile>,
}

/// One file of this phone's folder on a computer.
#[derive(Debug, Clone, uniffi::Record)]
pub struct KeptFile {
    /// Its name, as the folder shows it.
    pub name: String,
    /// Where it is kept, in its phone's vault: under `.qurb-outside/whatsapp/`
    /// for WhatsApp, `.qurb-outside/storage/` for a backed-up folder, else a
    /// Private Vault path. Where it is restored to follows from this.
    pub path: String,
    /// Kept for this phone, rather than for it before it was set up again.
    pub mine: bool,
    /// Its content's hash, in hex: what fetching it asks for.
    pub content: String,
    pub size: u64,
    /// Unix seconds.
    pub modified_at: i64,
}

/// This phone's folder on a computer, reached once and kept open while it is
/// browsed: listed a place at a time, and any file in it fetched to where the
/// app says (decision 0067, step 4). Closed when the app leaves it.
#[derive(uniffi::Object)]
pub struct KeptFolder {
    runtime: Arc<tokio::runtime::Runtime>,
    client: qurb_peer::PeerClient,
    /// Held so that what it started -- announcing, the rendezvous connection
    /// -- lives as long as the folder is open.
    _connector: Arc<qurb_peer::Connector>,
    /// Answering the computer too, as a sync pass does: reaching a device
    /// behind a router is both sides dialling.
    accepting: Vec<tokio::task::JoinHandle<()>>,
    reader: std::sync::Mutex<Store>,
    name: String,
    me: [u8; 32],
}

#[uniffi::export]
impl KeptFolder {
    /// The computer's name.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The folders and files at `dir`: at the top, this phone and, when it
    /// was set up again, what was kept for it before.
    pub fn list(&self, dir: String) -> Result<KeptListing, QurbError> {
        let answer = self
            .runtime
            .block_on(self.client.kept(&dir))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;
        let Some((folders, files)) = answer else {
            return Err(QurbError::NotFound { detail: format!("{} keeps no folder for this phone", self.name) });
        };
        Ok(KeptListing { folders, files: self.files(files) })
    }

    /// One folder of the computer's shared folder, at `dir` (decision 0073,
    /// item 6): its own, reached live, nothing of it kept on this phone. A
    /// file in it is fetched and streamed as one of this phone's folder is.
    pub fn shared(&self, dir: String) -> Result<KeptListing, QurbError> {
        let answer = self
            .runtime
            .block_on(self.client.shared(&dir))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;
        let Some((folders, files)) = answer else {
            return Err(QurbError::NotFound { detail: format!("{} shares no folder with this phone", self.name) });
        };
        Ok(KeptListing { folders, files: self.files(files) })
    }

    /// The files whose place holds `query`: in the computer's shared folder,
    /// or in this phone's own folder there (decision 0073, item 4). Each
    /// named by its place, the newest first.
    pub fn search(&self, shared: bool, query: String) -> Result<Vec<KeptFile>, QurbError> {
        let within = if shared { qurb_peer::wire::Within::Shared } else { qurb_peer::wire::Within::Kept };
        let answer = self
            .runtime
            .block_on(self.client.search(within, &query))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;
        let files = answer.ok_or_else(|| QurbError::NotFound { detail: format!("{} cannot be searched from here", self.name) })?;
        Ok(self.files(files))
    }

    /// Make a folder in the computer's shared folder, at `path`.
    pub fn make_folder(&self, path: String) -> Result<(), QurbError> {
        changed(self.runtime.block_on(self.client.shared_mkdir(&path)))
    }

    /// Rename or move a file or folder of the computer's shared folder.
    pub fn rename(&self, from: String, to: String) -> Result<(), QurbError> {
        changed(self.runtime.block_on(self.client.shared_move(&from, &to)))
    }

    /// Delete a file or folder of the computer's shared folder, into its
    /// Recently deleted for 30 days.
    pub fn delete(&self, path: String) -> Result<(), QurbError> {
        changed(self.runtime.block_on(self.client.shared_delete(&path)))
    }

    /// Add the file at `source`, an absolute path on this phone, to the
    /// computer's shared folder at `to`, checked whole there before it is put
    /// in place. `progress` hears the bytes sent after each piece. Returns
    /// where it was put: numbered when the name was taken.
    pub fn add_file(&self, source: String, to: String, progress: Arc<dyn Progress>) -> Result<String, QurbError> {
        let placed = self
            .runtime
            .block_on(self.client.shared_upload(std::path::Path::new(&source), &to, |sent| progress.sent(sent)))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;
        placed.map_err(|why| QurbError::Refused { detail: why })
    }

    /// Fetch `file` from the computer to `to`, an absolute path, checked
    /// chunk by chunk and whole before it is put there. A file already at
    /// `to` is not replaced: the fetched one goes beside it, `name (2).ext`.
    /// Returns where it went.
    pub fn download(&self, file: KeptFile, to: String) -> Result<String, QurbError> {
        let content = unhex(&file.content).ok_or_else(|| QurbError::Other { detail: "not a file of the folder".into() })?;
        let to = free_path(std::path::Path::new(&to));
        let parent = to.parent().ok_or_else(|| QurbError::Other { detail: "nowhere to put it".into() })?;
        std::fs::create_dir_all(parent).map_err(|e| QurbError::Storage { detail: e.to_string() })?;
        let partial = parent.join(format!(".{}.qurb-part", to.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
        let fetched = (|| {
            let mut out = std::fs::File::create(&partial).map_err(|e| QurbError::Storage { detail: e.to_string() })?;
            let reader = self.reader.lock().map_err(|_| QurbError::Other { detail: "the folder is closed".into() })?;
            self.runtime
                .block_on(self.client.fetch_content_into(&reader, content, &mut out))
                .map_err(|e| QurbError::Network { detail: e.to_string() })?;
            out.sync_all().map_err(|e| QurbError::Storage { detail: e.to_string() })?;
            std::fs::rename(&partial, &to).map_err(|e| QurbError::Storage { detail: e.to_string() })
        })();
        if fetched.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        fetched?;
        Ok(to.display().to_string())
    }

    /// `file`, to be read in parts as a player asks for them: shown or
    /// played while it arrives, seeking included, with nothing kept on the
    /// phone but the few chunks being read (`qurb_peer::Ranged`). On the S23
    /// on 2026-10-15 a 1.6 GB episode had to arrive whole, in the app's
    /// cache, before it would start. Over this folder's connection, so the
    /// folder stays open while it plays.
    pub fn stream(&self, file: KeptFile) -> Result<Arc<KeptStream>, QurbError> {
        let content = unhex(&file.content).ok_or_else(|| QurbError::Other { detail: "not a file of the folder".into() })?;
        let ranged = self
            .runtime
            .block_on(qurb_peer::Ranged::open(self.client.clone(), content))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?
            .ok_or_else(|| QurbError::NotFound { detail: format!("{} no longer keeps {}", self.name, file.name) })?;
        Ok(Arc::new(KeptStream { runtime: Arc::clone(&self.runtime), ranged }))
    }

    /// Stop: the connection closes, and nothing more is answered. Not
    /// `close`, which every generated object already has.
    pub fn disconnect(&self) {
        self.client.close();
        for task in &self.accepting {
            task.abort();
        }
    }
}

impl KeptFolder {
    fn files(&self, files: Vec<qurb_peer::wire::KeptEntry>) -> Vec<KeptFile> {
        files
            .into_iter()
            .map(|f| KeptFile {
                name: f.name,
                path: f.path,
                mine: f.owner == self.me,
                content: hex(&f.content),
                size: f.size,
                modified_at: f.modified_at,
            })
            .collect()
    }
}

/// A change asked of a computer's shared folder: made, or refused with its
/// reason for the person.
fn changed(answer: qurb_peer::Result<std::result::Result<(), String>>) -> Result<(), QurbError> {
    answer
        .map_err(|e| QurbError::Network { detail: e.to_string() })?
        .map_err(|why| QurbError::Refused { detail: why })
}

/// Told how far a file being added to a computer has gone, in bytes
/// ([`KeptFolder::add_file`]).
#[uniffi::export(with_foreign)]
pub trait Progress: Send + Sync {
    fn sent(&self, bytes: u64);
}

/// A file of this phone's folder on a computer, read in parts as a player
/// asks ([`KeptFolder::stream`]). At most a megabyte crosses at a time: a
/// player's reads are smaller, and nothing here holds a whole file, which is
/// what decision 0018 is for.
#[derive(uniffi::Object)]
pub struct KeptStream {
    runtime: Arc<tokio::runtime::Runtime>,
    ranged: Arc<qurb_peer::Ranged>,
}

#[uniffi::export]
impl KeptStream {
    /// The file's size in bytes.
    pub fn size(&self) -> u64 {
        self.ranged.size()
    }

    /// Up to `length` bytes from `offset`: fewer at the end of the file, none
    /// past it, and never more than a megabyte. Each chunk is checked against
    /// its hash before a byte of it is given.
    ///
    /// Given up after [`STREAM_READ_LIMIT`]: the app reading is a player, and
    /// a player waiting on a read that never ends is declared not responding.
    /// On the S23 on 2026-10-19 a video's reads waited out a connection lost
    /// with the Wi-Fi, over a minute, and the player was.
    pub fn read_at(&self, offset: u64, length: u32) -> Result<Vec<u8>, QurbError> {
        let ranged = Arc::clone(&self.ranged);
        self.runtime
            .block_on(async move {
                tokio::time::timeout(STREAM_READ_LIMIT, ranged.read_at(offset, length as usize)).await
            })
            .map_err(|_| QurbError::Network { detail: "the computer did not answer in time".into() })?
            .map_err(|e| QurbError::Network { detail: e.to_string() })
    }

    /// Stop reading: the connection is ended, and a read waiting on it fails
    /// at once. When the phone's network changes, the app does this to every
    /// file open in a player, which then says it cannot read rather than
    /// hanging.
    ///
    /// Not `close`: UniFFI gives every object a `close` of its own, which
    /// frees it.
    pub fn stop_reading(&self) {
        self.ranged.close();
    }
}

/// The longest one read of a streamed file may take: long enough for a
/// megabyte over a slow mobile network, short of a player's patience.
const STREAM_READ_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// `path`, or the first `name (n).ext` beside it that is free.
fn free_path(path: &std::path::Path) -> std::path::PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|candidate| !candidate.exists())
        .expect("a free name")
}

/// An invitation this device is offering, while it waits to be joined.
#[derive(uniffi::Object)]
pub struct Pairing {
    code: String,
    human: String,
    expires_at: i64,
    /// Six digits a device on the same network types instead (decision 0066).
    digits: Option<String>,
    /// Whether they still work: tried wrongly too often, they stop.
    digits_work: Arc<std::sync::atomic::AtomicBool>,
    inner: Mutex<Option<qurb_peer::PairingHost>>,
    store: Arc<std::sync::Mutex<Store>>,
    name: String,
    runtime: Arc<tokio::runtime::Runtime>,
    /// Told when somebody gives up, so a `wait` already blocking returns
    /// rather than listening until the code expires.
    cancelled: Arc<tokio::sync::Notify>,
    /// Given to a device with no key that joins with this code, once
    /// (decision 0052).
    master: MasterKey,
}

#[uniffi::export]
impl Pairing {
    /// The code to put in a QR code.
    pub fn code(&self) -> String {
        self.code.clone()
    }

    /// The same code, grouped for reading aloud.
    ///
    /// Not a nicety. The code carries this device's full identity and must
    /// travel outside the network, so a channel that is nothing but a person's
    /// voice has to work.
    pub fn spoken(&self) -> String {
        self.human.clone()
    }

    /// Unix seconds after which the code stops working.
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }

    /// Six digits, spaced, that a device on the same network types instead
    /// (decision 0066); `None` once they were tried wrongly too often.
    pub fn digits(&self) -> Option<String> {
        self.digits.clone().filter(|_| self.digits_work.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Block until another device joins, or the invitation expires.
    ///
    /// Consumes the invitation: it works once, by design. A code that could be
    /// replayed would let anyone who saw it once join later. A device that
    /// presents it is let in only if `approver` says so, having been shown the
    /// number it should be showing (decision 0053).
    pub fn wait(&self, approver: Arc<dyn PairingApprover>) -> Result<PeerInfo, QurbError> {
        let host = self
            .inner
            .lock()
            .map_err(|_| QurbError::Other { detail: "pairing already failed".into() })?
            .take()
            .ok_or_else(|| QurbError::Other {
                detail: "this invitation has already been used".into(),
            })?;

        let cancelled = Arc::clone(&self.cancelled);
        let ours = qurb_peer::Ours { name: &self.name, kind: "phone", key: &self.master };
        // The person answers in the app, which blocks this call's thread: kept
        // off the runtime's own.
        let approve = |asking: qurb_peer::Asking| {
            let approver = Arc::clone(&approver);
            async move {
                let request = PairingRequest {
                    name: asking.name,
                    kind: asking.kind,
                    number: asking.number,
                    wants_key: asking.wants_key,
                    guest: asking.guest,
                };
                tokio::task::spawn_blocking(move || approver.approve(request)).await.unwrap_or(false)
            }
        };
        // Whether the digits still work, kept current for the screen.
        let digits_work = Arc::clone(&self.digits_work);
        let watching = async {
            while host.digits().is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            digits_work.store(false, std::sync::atomic::Ordering::SeqCst);
            std::future::pending::<()>().await
        };
        let outcome = self.runtime.block_on(async {
            tokio::select! {
                joined = host.wait(Arc::clone(&self.store), &ours, now(), approve) => Some(joined),
                () = cancelled.notified() => None,
                () = watching => None,
            }
        });
        // Either way nothing more is listened for: a code works once.
        host.close();
        let paired = outcome
            .ok_or_else(|| QurbError::Other { detail: "pairing was cancelled".into() })?
            .map_err(pairing_failed)?;

        Ok(PeerInfo {
            fingerprint: hex(paired.fingerprint.as_bytes()),
            short: paired.fingerprint.short(),
            name: paired.name,
            paired_at: now(),
            last_seen: None,
            relation: "own".into(),
            kind: paired.kind,
        })
    }

    /// Give up waiting, and stop listening -- whether or not `wait` has
    /// started. A permit is stored if nothing is waiting yet, so a `wait` that
    /// starts afterwards returns at once too.
    pub fn cancel(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(host) = guard.take() {
                host.close();
            }
        }
        self.cancelled.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A device collecting keeps the pass open past its window, up to the
    /// limit and no further; nobody collecting ends it once nothing is
    /// waiting or there is no time left for anyone to come.
    #[test]
    fn a_pass_answers_while_it_is_being_collected_from() {
        let start = Instant::now();
        let come_by = start + Duration::from_secs(10);
        let at_most = start + Duration::from_secs(1800);
        let at = |s: u64| start + Duration::from_secs(s);

        // Past the window, while chunks are going: on.
        assert!(keep_answering(at(600), come_by, at_most, true, true, || false));
        // At the limit, even mid-collection: off.
        assert!(!keep_answering(at(1800), come_by, at_most, true, true, || true));
        // Nobody collecting, something waiting, still time for a device to come.
        assert!(keep_answering(at(5), come_by, at_most, false, false, || true));
        // Nobody collecting, and nobody came in time.
        assert!(!keep_answering(at(11), come_by, at_most, false, false, || true));
        // Nobody collecting and nothing waiting: done.
        assert!(!keep_answering(at(5), come_by, at_most, false, false, || false));
    }

    /// A device that stops part-way -- restarting, or its Wi-Fi dropping -- is
    /// waited for while what it was collecting is still waiting; one that
    /// finished is not, and nor is one gone longer than `PAUSED`.
    #[test]
    fn a_collector_that_pauses_part_way_is_waited_for() {
        let start = Instant::now();
        let come_by = start + Duration::from_secs(10);
        let at_most = start + Duration::from_secs(1800);
        let at = |s: u64| start + Duration::from_secs(s);

        // Long past `come_by`, nothing went in ten seconds, but something did
        // within the minute and the file is not collected yet: on.
        assert!(keep_answering(at(300), come_by, at_most, false, true, || true));
        // The same pause with everything collected: off.
        assert!(!keep_answering(at(300), come_by, at_most, false, true, || false));
        // Gone longer than the minute: off, though the file still waits.
        assert!(!keep_answering(at(300), come_by, at_most, false, false, || true));
        // And never past the limit.
        assert!(!keep_answering(at(1800), come_by, at_most, false, true, || true));
    }

    /// The window is the limit when nobody asked for longer: `sync_within`
    /// passes the window as both, so a collection never holds its pass open
    /// beyond what the platform granted.
    #[test]
    fn without_asking_the_window_is_the_limit() {
        let start = Instant::now();
        let window = start + Duration::from_secs(20);
        assert!(!keep_answering(start + Duration::from_secs(20), window, window, true, true, || true));
    }

    /// A long pass gives a device a minute to come, as long as one that paused
    /// part-way; an ordinary one ten seconds, inside its window. Neither past
    /// what it may run for.
    #[test]
    fn a_long_pass_waits_a_minute_for_a_collector_to_come() {
        let started = Instant::now();
        let now = started + Duration::from_secs(12);
        let window = Duration::from_secs(20);

        let long = come_by(now, started, window, Duration::from_secs(1800));
        assert_eq!(long, now + PAUSED);
        let ordinary = come_by(now, started, window, window);
        assert_eq!(ordinary, started + window, "ten seconds would go past the window");
        let short = come_by(now, started, window, Duration::from_secs(30));
        assert_eq!(short, started + Duration::from_secs(30), "never past what it may run for");
    }

    /// Frozen, a pass excuses the collector's silence for a minute after it
    /// thaws; an ordinary look between two others excuses nothing new.
    #[test]
    fn a_pass_that_was_frozen_waits_again_for_its_collector() {
        let looked = Instant::now();
        let none = looked;

        let on_time = looked + LOOK_EVERY;
        assert_eq!(excused_until(looked, on_time, none), none);

        let thawed = looked + Duration::from_secs(90);
        let excused = excused_until(looked, thawed, none);
        assert_eq!(excused, thawed + PAUSED);
        assert!(keep_answering(thawed, none, looked + Duration::from_secs(1800), false, thawed < excused, || true));

        // A later ordinary look keeps the excuse; it does not cut it short.
        let next = thawed + LOOK_EVERY;
        assert_eq!(excused_until(thawed, next, excused), excused);
    }

    /// A computer can point a phone only at its own ordinary folders
    /// (decision 0063): nothing absolute, climbing out, hidden, or kept from
    /// every app by Android.
    #[test]
    fn a_chosen_folder_is_one_of_the_phones_own() {
        for good in ["DCIM", "DCIM/Camera", "Documents", "Android/media/com.whatsapp/WhatsApp", "Pictures/Screenshots"] {
            assert!(choosable(good), "{good} refused");
        }
        for bad in ["", "/data/data/com.qurb", "../secrets", "DCIM/../..", "DCIM//Camera", ".thumbnails", "DCIM/.trash", "Android/data/com.whatsapp", "Android/obb"] {
            assert!(!choosable(bad), "{bad} accepted");
        }
    }

    /// Collected from, a pass stays whether or not it reached anybody itself;
    /// not collected from, it stays only for a device it reached that has
    /// something waiting.
    #[test]
    fn a_device_collecting_is_served_whoever_dialled() {
        assert!(stays_to_answer(false, true, || false), "dialled in, collecting");
        assert!(stays_to_answer(true, false, || true), "reached, something waiting");
        assert!(!stays_to_answer(true, false, || false), "reached, nothing waiting");
        assert!(!stays_to_answer(false, false, || true), "reached nobody, nobody collecting");
    }

    #[test]
    fn a_chunk_just_served_counts_as_collecting() {
        let stats = ServingStats::default();
        assert!(!stats.recently(COLLECTING), "nothing has gone yet");
        qurb_peer::Served::served(&stats, &qurb_peer::Fingerprint::from_bytes([1; 32]), &blake3::hash(b"x"), 512);
        assert!(stats.recently(COLLECTING));
        assert_eq!(stats.bytes.load(std::sync::atomic::Ordering::Relaxed), 512);
    }

    /// A sync bringing a large file in holds the engine for as long as it
    /// takes: on the S23, 64 seconds for 200 MB, with Devices, Home and Files
    /// waiting behind it (2026-10-19). What they ask is read through a
    /// connection of its own, and answered meanwhile.
    #[test]
    fn the_screens_are_answered_while_the_engine_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().display().to_string();
        create(root.clone()).unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"one").unwrap();
        let qurb = std::sync::Arc::new(Qurb::open(root, None).unwrap());
        qurb.scan().unwrap();

        let holder = std::sync::Arc::clone(&qurb);
        let (held, release) = (std::sync::mpsc::channel(), std::sync::mpsc::channel::<()>());
        let holding = std::thread::spawn(move || {
            let _engine = holder.engine().unwrap();
            held.0.send(()).unwrap();
            release.1.recv().unwrap();
        });
        held.1.recv().unwrap();

        let asked = Instant::now();
        assert!(qurb.peers().unwrap().is_empty());
        assert_eq!(qurb.page(0, 10).unwrap().len(), 1);
        assert!(qurb.contains("notes.txt".into()).unwrap());
        assert!(qurb.usage().unwrap().logical > 0);
        qurb.history(10, None).unwrap();
        qurb.waiting().unwrap();
        qurb.browse_in(String::new(), false).unwrap();
        assert!(asked.elapsed() < Duration::from_secs(2), "the screens waited {:?}", asked.elapsed());

        release.0.send(()).unwrap();
        holding.join().unwrap();
    }
}
