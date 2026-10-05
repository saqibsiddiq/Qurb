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

/// Set `root` up as another of the same person's devices, from the code a
/// device of theirs is showing, and pair the two.
///
/// The key arrives over the pairing connection, pinned to the device whose
/// fingerprint the code carries, so nobody types 24 words (decision 0052).
/// Nothing is installed unless it arrives: a code that is wrong, expired or
/// already used leaves only this device's new certificate behind, which the
/// next attempt reuses.
pub async fn join(root: &Path, code: &str) -> Result<qurb_peer::Paired> {
    let invite = qurb_peer::Invite::parse(code.trim()).context("that is not a valid pairing code")?;
    let dir = store_dir(root);
    if Vault::at(&dir).exists() {
        bail!("{} is already set up", root.display());
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let identity = Identity::load_or_create(&dir)?;
    let name = Config::default().name;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let root = root.to_path_buf();
    let paired = qurb_peer::join(&invite, &identity, &name, now, |key| {
        enrol(&root, &key.to_phrase()).map_err(|e| format!("{e:#}"))?;
        let chunk_key = ChunkKey::from_bytes(key.derive(Purpose::ChunkEncryption).to_bytes());
        let store = Store::open(&dir, chunk_key).map_err(|e| e.to_string())?.in_tree(&root);
        Ok(std::sync::Arc::new(std::sync::Mutex::new(store)))
    })
    .await?;
    Ok(paired)
}

/// How much disk a device may use, as somebody typed it while setting it up.
///
/// The question is asked in gigabytes, so a bare number is gigabytes; a unit
/// may still be given, with or without its `B`: `75`, `75 GB`, `1.5T`,
/// `500 MB`. The unit is the one `qurb config` uses, counted in 1024s, so the
/// figure chosen is the figure the Storage screen shows afterwards. Nothing is
/// refused here as well as nonsense: setup asks for an allowance
/// (decision 0038), and "no limit" stays what `qurb config <dir> limit=0` sets.
pub fn allowance(text: &str) -> Result<u64> {
    let trimmed = text.trim();
    let unit = ["iB", "ib", "IB", "B", "b"]
        .iter()
        .find_map(|suffix| trimmed.strip_suffix(suffix))
        .unwrap_or(trimmed)
        .replace(' ', "");
    let unit = match unit.chars().last() {
        Some(c) if c.is_ascii_digit() || c == '.' => format!("{unit}G"),
        _ => unit,
    };
    let bytes = crate::config::parse_size(&unit)
        .map_err(|_| anyhow::anyhow!("`{}` is not an amount of space — try 75 or 1.5 TB", text.trim()))?;
    if bytes == 0 {
        bail!("an allowance of nothing would leave qurb unable to keep a file");
    }
    Ok(bytes)
}

/// Write down the allowance chosen while setting up: the same setting the
/// Storage screen and `qurb config <dir> limit=` change later.
pub fn allow(root: &Path, bytes: u64) -> Result<()> {
    let dir = store_dir(root);
    let mut config = Config::load(&dir)?;
    config.limit = bytes;
    config.save(&dir)
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

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    #[test]
    fn an_allowance_is_read_the_way_people_type_it() {
        assert_eq!(allowance("75").unwrap(), 75 * GB, "a bare number is gigabytes");
        assert_eq!(allowance(" 75 GB ").unwrap(), 75 * GB);
        assert_eq!(allowance("75gb").unwrap(), 75 * GB);
        assert_eq!(allowance("75G").unwrap(), 75 * GB);
        assert_eq!(allowance("1.5 TB").unwrap(), (1.5 * (1u64 << 40) as f64) as u64);
        assert_eq!(allowance("500 MB").unwrap(), 500 << 20);
        assert_eq!(allowance("2 TiB").unwrap(), 2 << 40);
    }

    #[test]
    fn nothing_and_nonsense_are_refused() {
        for text in ["", "0", "0 GB", "  ", "lots", "GB", "-5"] {
            assert!(allowance(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn the_allowance_is_written_where_the_storage_screen_reads_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(store_dir(dir.path())).unwrap();
        allow(dir.path(), 100 * GB).unwrap();
        assert_eq!(Config::load(&store_dir(dir.path())).unwrap().limit, 100 * GB);
    }
}

