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

/// What the store costs on this device.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Usage {
    /// What the user's files add up to, counted the way they would count them:
    /// three copies of one photo are three photos.
    pub logical: u64,
    /// What they actually occupy here, after identical content is stored once
    /// and compressed. The gap between this and `logical` is the saving.
    pub on_disk: u64,
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
    /// A relay to fall back to when no direct path exists. `None` means direct
    /// connections only, which on a cellular network often means none at all.
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device_name: "phone".to_string(),
            signal_url: "ws://localhost:9000".to_string(),
            relay: None,
            port: 0,
            discover: true,
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
    /// Built on first use and kept.
    ///
    /// Lazy because a phone that only browses its files should not pay for a
    /// thread pool, and kept because building one per call would be worse.
    runtime: Mutex<Option<Arc<tokio::runtime::Runtime>>>,
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

    /// Every live file, sorted by path.
    pub fn list(&self) -> Result<Vec<FileEntry>, QurbError> {
        let engine = self.engine()?;
        let db = engine.store().db();
        let mut out = Vec::new();
        for path in db.live_paths()? {
            let Some(file) = db.file_by_path(&path)? else { continue };
            out.push(FileEntry {
                path,
                size: file.size,
                modified_at: file.mtime_ns,
            });
        }
        Ok(out)
    }

    /// Whether a path exists in the index.
    pub fn contains(&self, path: String) -> Result<bool, QurbError> {
        Ok(self.engine()?.store().db().file_by_path(&path)?.is_some_and(|f| f.deleted_at.is_none()))
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
        let logical = qurb_watcher::normalize(&path);
        let destination = self.root.join(&logical);

        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| QurbError::Storage { detail: e.to_string() })?;
        }

        if !same_file(Path::new(&source), &destination) {
            std::fs::copy(&source, &destination)
                .map_err(|e| QurbError::Storage { detail: format!("{source}: {e}") })?;
        }

        let mut engine = self.engine()?;
        engine.store_mut().put_file(&logical, &destination)?;
        Ok(())
    }

    /// Remove a file from the synced tree.
    ///
    /// Tombstoned rather than erased, so the deletion reaches other devices
    /// instead of looking to them like a file they should send back.
    pub fn remove(&self, path: String) -> Result<(), QurbError> {
        let destination = self.root.join(&path);
        match std::fs::remove_file(&destination) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(QurbError::Storage { detail: e.to_string() }),
        }
        self.engine()?.store_mut().delete_file(&qurb_watcher::normalize(&path))?;
        Ok(())
    }

    /// Devices this one trusts.
    pub fn peers(&self) -> Result<Vec<PeerInfo>, QurbError> {
        let engine = self.engine()?;
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
        let port = self.port;
        let runtime = self.runtime()?;

        // Inside the runtime even though `open` is not async: it binds a QUIC
        // endpoint, and quinn registers the socket with whatever reactor is
        // current. Without this it fails with "no async runtime found" — which
        // names the cause but not the fix, since nothing in the call is awaited.
        let host = {
            let _guard = runtime.enter();
            qurb_peer::PairingHost::open(
                format!("0.0.0.0:{port}").parse().expect("a literal address"),
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
            store: self.shared_store()?,
            name: self.device_name.clone(),
            runtime,
            inner: Mutex::new(Some(host)),
        }))
    }

    /// Accept an invitation offered by another device.
    ///
    /// The usual direction for a phone: the desktop shows a QR code and the
    /// phone's camera reads it.
    pub fn join_pairing(&self, code: String) -> Result<PeerInfo, QurbError> {
        let invite = qurb_peer::Invite::parse(&code)
            .map_err(|e| QurbError::BadCode { detail: e.to_string() })?;

        let identity = self.identity()?;
        let store = self.shared_store()?;
        let runtime = self.runtime()?;

        let paired = runtime
            .block_on(qurb_peer::accept(&invite, &identity, store, &self.device_name, now()))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;

        Ok(PeerInfo {
            fingerprint: hex(paired.fingerprint.as_bytes()),
            short: paired.fingerprint.short(),
            name: paired.name,
            paired_at: now(),
            last_seen: None,
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
        self.sync_inner(deadline)
    }

    /// How much space the store occupies on this device.
    ///
    /// Both numbers, because on a phone the difference is the selling point:
    /// `logical` is what the files add up to, `on_disk` is what they cost after
    /// deduplication and compression.
    pub fn usage(&self) -> Result<Usage, QurbError> {
        let engine = self.engine()?;
        let db = engine.store().db();
        // Live *files*, not chunks. Summing chunks would count shared content
        // once and report that three copies of a photo take up one photo's
        // worth of space, which is true of the disk and not of the library.
        let (_, on_disk) = db.size_totals()?;
        Ok(Usage { logical: db.live_bytes()?, on_disk })
    }
}

/// Not exported. `#[uniffi::export]` takes every function in the block it is
/// applied to, and neither a `MutexGuard` nor a four-argument constructor
/// taking an optional callback interface can cross an FFI boundary.
impl Qurb {
    fn open_inner(
        root: String,
        passphrase: Option<String>,
        settings: Settings,
        keystore: Option<Arc<dyn KeyStore>>,
    ) -> Result<Self, QurbError> {
        let store_dir = store_dir(Path::new(&root));
        let vault = vault_at(&store_dir, keystore.as_ref());

        if !vault.exists() {
            return Err(QurbError::NotSetUp { detail: format!("{root} has no vault") });
        }

        let master: MasterKey = vault.unlock(passphrase.as_deref())?;
        let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
        let root = PathBuf::from(&root);
        let store = Store::open(&store_dir, chunk_key)?;
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);

        Ok(Self {
            inner: Mutex::new(Engine::new(&root, store, ignore)),
            root,
            store_dir,
            master,
            device_name: settings.device_name,
            signal_url: settings.signal_url,
            relay: settings.relay,
            port: settings.port,
            discover: settings.discover,
            runtime: Mutex::new(None),
        })
    }
}


/// Not exported. `#[uniffi::export]` takes every method in the block it is
/// applied to, and a `MutexGuard` cannot cross an FFI boundary — nor should it.
impl Qurb {
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
        let key = self.engine()?.store().chunk_key();
        let store = Store::open(&self.store_dir, key)?;
        Ok(Arc::new(std::sync::Mutex::new(store)))
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
    fn sync_inner(&self, budget: std::time::Duration) -> Result<SyncOutcome, QurbError> {
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
        let relay = match &self.relay {
            Some(text) => Some(
                text.parse::<std::net::SocketAddr>()
                    .map_err(|e| QurbError::Other { detail: format!("bad relay address: {e}") })?,
            ),
            None => None,
        };

        // Built per pass rather than kept. A phone's address changes with every
        // move between Wi-Fi and cellular, and a connector holding a stale
        // public address announces somewhere nothing can reach.
        let connector = runtime
            .block_on(qurb_peer::Connector::start(
                format!("0.0.0.0:{}", self.port).parse().expect("a literal address"),
                identity,
                self.master.clone(),
                &peers,
                self.signal_url.clone(),
                self.discover,
                relay,
            ))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;

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
            accepting.push(runtime.spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let store = Arc::clone(&store);
                    let generation = Arc::clone(&generation);
                    tokio::spawn(async move {
                        if let Ok(connection) = incoming.await {
                            qurb_peer::server::serve_connection(connection, store, generation)
                                .await;
                        }
                    });
                }
            }));
        }

        for peer in peers {
            let left = match budget.checked_sub(started.elapsed()) {
                Some(left) if !left.is_zero() => left,
                _ => {
                    outcome.timed_out = true;
                    break;
                }
            };

            match self.sync_one(&runtime, &connector, peer, left) {
                Ok(Some(stats)) => {
                    outcome.reached += 1;
                    outcome.adopted += stats.adopted as u32;
                    outcome.conflicts += stats.conflicts as u32;
                }
                // Ran out of time mid-peer. Whatever landed is already
                // committed; the rest is the next window's problem.
                Ok(None) => {
                    outcome.timed_out = true;
                    break;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "peer unreachable");
                    outcome.unreachable += 1;
                }
            }
        }

        for task in accepting {
            task.abort();
        }
        Ok(outcome)
    }

    /// Sync against one peer. `Ok(None)` means the time ran out.
    fn sync_one(
        &self,
        runtime: &tokio::runtime::Runtime,
        connector: &qurb_peer::Connector,
        peer: qurb_peer::Fingerprint,
        budget: std::time::Duration,
    ) -> Result<Option<qurb_engine::PlanStats>, QurbError> {
        // The timeout is constructed *inside* the async block. `timeout` needs a
        // reactor when it is built, not when it is awaited, so building it
        // outside `block_on` panics with a message about being called from
        // outside a runtime -- which is true and reads like it is about the
        // future it wraps.
        let client = match runtime
            .block_on(async { tokio::time::timeout(budget, connector.reach(peer)).await })
        {
            Ok(Ok(client)) => client,
            Ok(Err(e)) => return Err(QurbError::Network { detail: e.to_string() }),
            Err(_) => return Ok(None),
        };

        let started = std::time::Instant::now();
        let tree = match runtime
            .block_on(async { tokio::time::timeout(budget, client.tree()).await })
        {
            Ok(Ok(tree)) => tree,
            Ok(Err(e)) => return Err(QurbError::Network { detail: e.to_string() }),
            Err(_) => return Ok(None),
        };

        let mut engine = self.engine()?;
        let plan = engine.plan_against(&tree)?;
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
        let reader = Store::open(&self.store_dir, engine.store().chunk_key())?;
        let stats = runtime.block_on(async {
            tokio::task::block_in_place(|| {
                let mut source = qurb_peer::NetworkSource::new(&client, &reader);
                engine.apply_plan(&plan, &mut source)
            })
        })?;
        Ok(Some(stats))
    }

    fn engine(&self) -> Result<std::sync::MutexGuard<'_, Engine>, QurbError> {
        // A poisoned lock means an earlier call panicked while holding it. The
        // engine's state is then unknown, so this reports rather than recovers.
        self.inner.lock().map_err(|_| QurbError::Other {
            detail: "the engine is in an unknown state after an earlier failure".into(),
        })
    }
}

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
    /// Whether the pass ran out of time before finishing.
    ///
    /// Not a failure. It means the next window has work to do, and the platform
    /// side should schedule one rather than report a problem.
    pub timed_out: bool,
}

/// An invitation this device is offering, while it waits to be joined.
#[derive(uniffi::Object)]
pub struct Pairing {
    code: String,
    human: String,
    expires_at: i64,
    inner: Mutex<Option<qurb_peer::PairingHost>>,
    store: Arc<std::sync::Mutex<Store>>,
    name: String,
    runtime: Arc<tokio::runtime::Runtime>,
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

    /// Block until another device joins, or the invitation expires.
    ///
    /// Consumes the invitation: it works once, by design. A code that could be
    /// replayed would let anyone who saw it once join later.
    pub fn wait(&self) -> Result<PeerInfo, QurbError> {
        let host = self
            .inner
            .lock()
            .map_err(|_| QurbError::Other { detail: "pairing already failed".into() })?
            .take()
            .ok_or_else(|| QurbError::Other {
                detail: "this invitation has already been used".into(),
            })?;

        let paired = self
            .runtime
            .block_on(host.wait(Arc::clone(&self.store), &self.name, now()))
            .map_err(|e| QurbError::Network { detail: e.to_string() })?;

        Ok(PeerInfo {
            fingerprint: hex(paired.fingerprint.as_bytes()),
            short: paired.fingerprint.short(),
            name: paired.name,
            paired_at: now(),
            last_seen: None,
        })
    }

    /// Give up waiting, and stop listening.
    pub fn cancel(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(host) = guard.take() {
                host.close();
            }
        }
    }
}

