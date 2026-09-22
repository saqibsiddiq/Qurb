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
pub fn known() -> Vec<PathBuf> {
    let Ok(path) = registry_path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };

    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .collect()
}

/// Remember a folder, moving it to the front.
pub fn remember(root: &Path) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut folders: Vec<PathBuf> = known().into_iter().filter(|f| f != root).collect();
    folders.insert(0, root.to_path_buf());
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
    let folders: Vec<PathBuf> = known().into_iter().filter(|f| f != root).collect();
    let body = folders.iter().map(|f| f.display().to_string()).collect::<Vec<_>>().join("\n");
    std::fs::write(&path, format!("# folders qurb knows about, most recent first\n{body}\n"))?;
    Ok(())
}

/// Where to put a folder when the person has not said.
///
/// Under Downloads rather than a hidden directory or the home root: files that
/// sync between devices are files someone wants to *find*, and Downloads is
/// where every desktop already looks. `XDG_DOWNLOAD_DIR` is honoured because on
/// a non-English system the folder is not called "Downloads".
pub fn default_root() -> Result<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").context("no HOME set")?);

    // The XDG user-dirs file, which is what the file manager itself reads.
    let config = home.join(".config").join("user-dirs.dirs");
    if let Ok(text) = std::fs::read_to_string(&config) {
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("XDG_DOWNLOAD_DIR=") {
                let value = value.trim().trim_matches('"');
                let expanded = value.replace("$HOME", &home.display().to_string());
                if !expanded.is_empty() {
                    return Ok(PathBuf::from(expanded).join("qurb"));
                }
            }
        }
    }

    Ok(home.join("Downloads").join("qurb"))
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
    // Both the new default and the old one, so an existing install keeps
    // working after the default moved.
    [default_root().ok(), legacy_root()]
        .into_iter()
        .flatten()
        .find(|candidate| crate::is_set_up(candidate))
}

/// Where folders used to go, before the default moved to Downloads.
fn legacy_root() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join("qurb"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default has to land inside Downloads, because that is where someone
    /// looks for a file that arrived from another device.
    #[test]
    fn the_default_is_under_downloads() {
        let Ok(root) = default_root() else { return };
        let text = root.display().to_string();
        assert!(
            text.to_lowercase().contains("download"),
            "the default should be under Downloads, got {text}"
        );
        assert!(text.ends_with("qurb"), "the folder should be named qurb, got {text}");
    }
}
