//! The daemon and the pieces around it, as a library.
//!
//! `qurb` is a command-line program, and this exists so that something other
//! than a terminal can drive the same daemon — an interface should be a
//! *display* of the engine rather than a second implementation of it.
//!
//! The binary in `main.rs` is the terminal front end. `qurb-tray` is another.
//! Neither owns the daemon; both run the same one.

pub mod config;
pub mod daemon;
pub mod lock;
pub mod profiles;
pub mod qr;
pub mod status;

use anyhow::{bail, Result};
use qurb_keys::{MasterKey, Purpose, Vault};
use qurb_peer::Identity;
use qurb_storage::{ChunkKey, Store};
use std::path::{Path, PathBuf};

pub use config::Config;
pub use daemon::Daemon;
pub use status::{Status, State, Watcher};

/// Where the store lives inside a synced root.
pub fn store_dir(root: &Path) -> PathBuf {
    root.join(".qurb")
}

/// Whether a directory has been set up.
pub fn is_set_up(root: &Path) -> bool {
    Vault::at(&store_dir(root)).exists()
}

/// Open everything a daemon needs.
///
/// Takes the passphrase as a closure rather than a value because most callers
/// do not have one and must not be made to invent one: a terminal prompts, and
/// an interface shows a dialog, and neither should happen for a vault that does
/// not need it. The closure is called only when the vault says so.
pub fn open_with(
    root: &Path,
    ask: impl FnOnce() -> Result<String>,
) -> Result<(MasterKey, Identity, Store, Config)> {
    let store_dir = store_dir(root);
    let vault = Vault::at(&store_dir);
    if !vault.exists() {
        bail!(
            "{} is not set up yet — run `qurb init {}` first",
            root.display(),
            root.display()
        );
    }

    let master = match vault.protection()? {
        qurb_keys::Protection::Passphrase => vault.unlock(Some(&ask()?))?,
        _ => vault.unlock(None)?,
    };

    let identity = Identity::load_or_create(&store_dir)?;
    let chunk_key = ChunkKey::from_bytes(master.derive(Purpose::ChunkEncryption).to_bytes());
    // In a tree: this device materialises its files, so the tree supplies
    // the payloads and the chunk store keeps only what it cannot.
    let store = Store::open(&store_dir, chunk_key)?.in_tree(root);
    let config = Config::load(&store_dir)?;
    Ok((master, identity, store, config))
}
