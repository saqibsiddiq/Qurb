//! The engine, as a phone can call it.
//!
//! Android and iOS cannot call Rust directly. This crate is the seam: a small,
//! deliberately boring surface that [UniFFI] turns into Kotlin and Swift. It
//! contains no sync logic of its own — everything here delegates to
//! [`qurb_engine`] — because logic that lives behind an FFI boundary is logic
//! that cannot be tested from the rest of the workspace.
//!
//! Three constraints shape it, and all three come from the platforms rather
//! than from taste.
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
//! **Errors.** A Rust error chain does not survive the crossing. Everything
//! becomes [`QurbError`], flat and matchable, with the detail kept as text.
//!
//! [UniFFI]: https://mozilla.github.io/uniffi-rs/

use qurb_engine::{Engine, SyncStats};
use qurb_keys::{MasterKey, Purpose, RecoveryPhrase, Vault};
use qurb_storage::{ChunkKey, Store};
use qurb_watcher::IgnoreRules;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
    let root = PathBuf::from(root);
    let store_dir = store_dir(&root);
    let vault = Vault::at(&store_dir);

    if vault.exists() {
        return Err(QurbError::Other {
            detail: format!("{} is already set up", root.display()),
        });
    }

    std::fs::create_dir_all(&store_dir)
        .map_err(|e| QurbError::Storage { detail: e.to_string() })?;

    match vault.open_or_create()? {
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
    let root = PathBuf::from(root);
    let store_dir = store_dir(&root);

    let phrase = RecoveryPhrase::parse(&phrase)
        .map_err(|e| QurbError::BadPhrase { detail: e.to_string() })?;

    std::fs::create_dir_all(&store_dir)
        .map_err(|e| QurbError::Storage { detail: e.to_string() })?;

    Vault::at(&store_dir).restore(&phrase)?;
    Ok(())
}

/// Whether this directory has been set up.
#[uniffi::export]
pub fn is_set_up(root: String) -> bool {
    Vault::at(&store_dir(Path::new(&root))).exists()
}

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
        let root = PathBuf::from(root);
        let store_dir = store_dir(&root);
        let vault = Vault::at(&store_dir);

        if !vault.exists() {
            return Err(QurbError::NotSetUp {
                detail: format!("{} has no vault", root.display()),
            });
        }

        let master: MasterKey = vault.unlock(passphrase.as_deref())?;
        let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
        let store = Store::open(&store_dir, chunk_key)?;
        let ignore = IgnoreRules::new().with_store_dir(&store_dir);

        Ok(Self { inner: Mutex::new(Engine::new(&root, store, ignore)), root: root.clone() })
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

/// Not exported. `#[uniffi::export]` takes every method in the block it is
/// applied to, and a `MutexGuard` cannot cross an FFI boundary — nor should it.
impl Qurb {
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
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
