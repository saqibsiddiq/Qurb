//! Which folders this person syncs, and where to put a new one.
//!
//! A person may have more than one — work and personal, say — and a machine
//! may have more than one person. Both need answering before anything else can
//! be built on top, because "the sync folder" is not a constant.
//!
//! # Several people, one computer
//!
//! Separate operating-system accounts already give complete separation, and
//! qurb inherits it rather than reimplementing it. Each account has its own
//! `$HOME`, so its own folders, its own vault, its own identity and its own
//! paired devices; the store is owner-only on disk, and on a phone the key
//! lives in a keystore that is per-app-per-user. One person cannot read
//! another's files, and neither can qurb.
//!
//! What this module adds is the case *inside* one account: someone who wants
//! two identities without two logins, and a list of what exists so an
//! interface can offer it rather than demanding a path.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Where the list of known folders is kept.
///
/// Under the XDG config directory rather than beside a store, because it
/// describes *which* stores exist and cannot live inside any one of them.
fn registry_path() -> Result<PathBuf> {
    let base = match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var("HOME").context("no HOME set")?).join(".config"),
    };
    Ok(base.join("qurb").join("folders"))
}

/// The folders this person has set up, most recently used first.
///
/// Paths that no longer exist are dropped on read rather than pruned on write:
/// a folder on a removable disk is absent while it is unplugged and should
/// come back, not be forgotten the first time someone opens the list.
///
/// Every path comes back absolute. A relative one can only be an entry written
/// before [`remember`] made them absolute, and it is read from the home
/// folder, which is where the desktop -- started at login, in the home folder
/// -- has been opening it. Read from wherever a terminal happened to be, the
/// same line named a different folder in the window and on the command line,
/// and a folder typed as `home/project/qurb` became a second device the window
/// ran while `qurb status` showed the first.
pub fn known() -> Vec<PathBuf> {
    let Ok(path) = registry_path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    let home = std::env::var_os("HOME").map(PathBuf::from);

    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .map(|folder| match &home {
            Some(home) if folder.is_relative() => home.join(folder),
            _ => folder,
        })
        .collect()
}

/// A folder as the list records it: absolute, so it names the same folder
/// whichever directory the next reader starts in.
fn absolute(root: &Path) -> PathBuf {
    std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Remember a folder, moving it to the front.
pub fn remember(root: &Path) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let root = absolute(root);
    let mut folders: Vec<PathBuf> = known().into_iter().filter(|f| *f != root).collect();
    folders.insert(0, root);
    folders.truncate(16);

    let body = folders
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, format!("# folders qurb knows about, most recent first\n{body}\n"))?;
    Ok(())
}

/// Stop listing a folder. Does not touch its contents.
pub fn forget(root: &Path) -> Result<()> {
    let path = registry_path()?;
    let root = absolute(root);
    let folders: Vec<PathBuf> = known().into_iter().filter(|f| *f != root).collect();
    let body = folders.iter().map(|f| f.display().to_string()).collect::<Vec<_>>().join("\n");
    std::fs::write(&path, format!("# folders qurb knows about, most recent first\n{body}\n"))?;
    Ok(())
}

/// Where to put a folder when the person has not said.
///
/// `~/qurb`. It was `Downloads/qurb` for a while, on the reasoning that files
/// from another device are files somebody wants to find; decision 0037 gave
/// that place to files sent *to* this device instead, which are ordinary files
/// qurb stops tracking, and the two must not overlap. Existing folders in
/// Downloads stay where they are -- see [`current`].
pub fn default_root() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var("HOME").context("no HOME set")?).join("qurb"))
}

/// The person's Downloads folder.
///
/// From the XDG user-dirs file, which is what the file manager itself reads,
/// because on a non-English system the folder is not called "Downloads".
pub fn user_downloads() -> Result<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").context("no HOME set")?);

    let config = home.join(".config").join("user-dirs.dirs");
    if let Ok(text) = std::fs::read_to_string(&config) {
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("XDG_DOWNLOAD_DIR=") {
                let value = value.trim().trim_matches('"');
                let expanded = value.replace("$HOME", &home.display().to_string());
                if !expanded.is_empty() {
                    return Ok(PathBuf::from(expanded));
                }
            }
        }
    }

    Ok(home.join("Downloads"))
}

/// The folder to act on when none was named.
///
/// In order: the most recently used that is actually set up, then the default
/// location if *it* is set up. Returns `None` rather than inventing one — a
/// program that silently creates a store somewhere is worse than one that asks.
pub fn current() -> Option<PathBuf> {
    for folder in known() {
        if crate::is_set_up(&folder) {
            return Some(folder);
        }
    }
    // The default, and where the default was before, so an existing install
    // keeps working after it moved.
    [default_root().ok(), user_downloads().ok().map(|d| d.join("qurb"))]
        .into_iter()
        .flatten()
        .find(|candidate| crate::is_set_up(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new folder goes in the home directory, and deliveries go to Downloads:
    /// the two must never be the same place, or a received file would be
    /// synced to every device.
    #[test]
    fn the_default_folder_is_not_where_deliveries_go() {
        let (Ok(root), Ok(downloads)) = (default_root(), user_downloads()) else { return };
        assert!(root.ends_with("qurb"), "the folder should be named qurb, got {}", root.display());
        assert!(
            !crate::config::overlaps(&root, &downloads.join("qurb")),
            "the default folder {} overlaps the default downloads directory",
            root.display()
        );
    }
}
