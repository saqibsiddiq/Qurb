//! Bringing a device into existence.
//!
//! Lifted out of the terminal front end because the window needs exactly the
//! same steps, and an interface that reimplemented them would be a second
//! definition of what a set-up device *is* — with its own bugs about which
//! files get written and in what order.
//!
//! # The recovery phrase
//!
//! [`create`] returns the phrase once and never writes it down. That is the
//! whole of the handling rule, and everything that touches it afterwards
//! inherits it: it is not logged, not persisted, not sent anywhere, and not
//! put in any debugging output. It reaches the person who owns it and nowhere
//! else.
//!
//! It can be shown again later — [`reveal`] derives it from the stored key —
//! because the key is already on this device and anybody who can read it can
//! read the files anyway. What cannot happen is recovering it from somewhere
//! else: there is nowhere else.

use crate::{store_dir, Config};
use anyhow::{bail, Context, Result};
use qurb_keys::{MasterKey, Opened, Purpose, RecoveryPhrase, Vault};
use qurb_peer::Identity;
use qurb_storage::{ChunkKey, Store};
use std::path::{Path, PathBuf};

/// What is true of a folder, before committing to it.
///
/// Answered for a path that may not exist yet, because the commonest first
/// action is to type somewhere new.
#[derive(Debug, Clone)]
pub struct Folder {
    pub path: PathBuf,
    pub exists: bool,
    /// Already has a device in it. Setting up again would refuse.
    pub set_up: bool,
    /// Nothing in it but, possibly, a store. An empty folder is the ordinary
    /// case; a full one is allowed and worth mentioning, because every file in
    /// it is about to be synced to every other device.
    pub empty: bool,
    /// Whether it could be written to — tested against the nearest existing
    /// ancestor when the folder itself does not exist yet.
    pub writable: bool,
    /// How many files are already there, if it exists. Capped while counting,
    /// because this is asked while somebody is typing.
    pub existing_files: usize,
    /// Size of the filesystem it is or would be on. Zero if unknown.
    pub disk: u64,
    pub free: u64,
}

/// At most this many files are counted before giving up and saying "many".
///
/// The count exists to warn somebody pointing qurb at a folder that already has
/// things in it. Two thousand is past the point where the warning has landed,
/// and this runs on every keystroke.
pub const COUNT_CAP: usize = 2_000;

/// What could be seen about `path` without changing anything.
pub fn inspect(path: &Path) -> Folder {
    let exists = path.is_dir();
    let set_up = crate::is_set_up(path);

    let mut existing_files = 0;
    if exists {
        existing_files = count_files(path, COUNT_CAP);
    }

    // For a folder that does not exist yet, the question is really about the
    // nearest ancestor that does: that is what has to be writable for the rest
    // to be creatable.
    let anchor = nearest_existing(path);
    let writable = anchor.as_ref().is_some_and(|dir| is_writable(dir));
    let (disk, free) = anchor.as_deref().map(space).unwrap_or((0, 0));

    Folder {
        path: path.to_path_buf(),
        exists,
        set_up,
        empty: existing_files == 0,
        writable,
        existing_files,
        disk,
        free,
    }
}

/// Create a device in `root` and return its recovery phrase, once.
///
/// The caller is responsible for showing the phrase and for not keeping it. It
/// is not stored anywhere by this function and cannot be asked for again from
/// the return value — only re-derived, from the key, by [`reveal`].
pub fn create(root: &Path) -> Result<RecoveryPhrase> {
    let dir = store_dir(root);
    std::fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;

    let vault = Vault::at(&dir);
    if vault.exists() {
        bail!("{} already has a key", root.display());
    }

    match vault.open_or_create()? {
        Opened::Created { key, phrase } => {
            furnish(&dir, &key)?;
            // Recorded so later commands, and the next launch, can find this
            // folder with no path at all.
            let _ = crate::profiles::remember(root);
            Ok(phrase)
        }
        // The vault did not exist a moment ago. Something else created one
        // between the two calls, and continuing would mean silently adopting a
        // key this process did not make.
        Opened::Existing(_) => bail!("a key appeared while we were creating one"),
    }
}

/// Set up `root` with a key that already exists elsewhere.
///
/// This is what makes two devices *yours*. It does not introduce them to each
/// other — that is pairing, and it is separate on purpose.
pub fn enrol(root: &Path, phrase: &RecoveryPhrase) -> Result<()> {
    let dir = store_dir(root);
    std::fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;

    let key = Vault::at(&dir).restore(phrase).context("installing the key")?;
    furnish(&dir, &key)?;
    let _ = crate::profiles::remember(root);
    Ok(())
}

/// The recovery phrase for a device that is already set up.
///
/// Derived from the stored key rather than remembered, because it was never
/// remembered. Offered at all because the key is already here: somebody who can
/// read this folder can read the files, so showing them the words gives away
/// nothing they did not have. Somebody who *cannot* is not helped by this.
pub fn reveal(key: &MasterKey) -> RecoveryPhrase {
    key.to_phrase()
}

/// Everything a set-up device has besides its key: an identity, a config, and
/// an index.
///
/// One function so that the two ways in cannot drift. A device created by one
/// path and missing a file the other writes is the kind of difference that only
/// shows up much later, on the device that was set up the unusual way.
fn furnish(dir: &Path, key: &MasterKey) -> Result<()> {
    Identity::load_or_create(dir)?;
    Config::default().save(dir)?;
    let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
    Store::open(dir, chunk_key)?;
    Ok(())
}

/// Files directly in `dir`, ignoring the store, stopping at `cap`.
fn count_files(dir: &Path, cap: usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|e| e.file_name() != ".qurb")
        .take(cap)
        .count()
}

/// The path itself if it exists, else the closest ancestor that does.
fn nearest_existing(path: &Path) -> Option<PathBuf> {
    let mut candidate = path;
    loop {
        if candidate.is_dir() {
            return Some(candidate.to_path_buf());
        }
        candidate = candidate.parent()?;
    }
}

/// Whether this process could create something in `dir`.
///
/// Asked of the operating system rather than inferred from the mode bits: the
/// answer depends on the effective user, the group list, and on Linux the
/// filesystem's own opinion, none of which a permission triple reports.
fn is_writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return false };
    // SAFETY: `access` reads a NUL-terminated path and returns a status. It
    // changes nothing.
    unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

/// Total and available bytes on the filesystem holding `dir`.
fn space(dir: &Path) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return (0, 0) };

    // SAFETY: `statvfs` writes into the struct and reads a NUL-terminated path,
    // both of which hold here. A failure leaves the struct untouched, which is
    // why the return value is checked before anything is read out of it.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) != 0 {
            return (0, 0);
        }
        let block = stat.f_frsize as u64;
        // `f_bavail` rather than `f_bfree`: some of the free space is reserved
        // for root, and offering it to somebody choosing an allowance would be
        // offering space they cannot have.
        ((stat.f_blocks as u64).saturating_mul(block), (stat.f_bavail as u64).saturating_mul(block))
    }
}
