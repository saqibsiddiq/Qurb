//! A computer's own folders, backed up to another computer (the owner's list
//! of 2026-10-28).
//!
//! What a phone does with its chosen folders (decision 0063), done by a
//! computer with folders of its own: each is kept in this device's vault
//! under `.qurb-outside/folders/<name>/`, read where it lies -- recorded, not
//! copied (decision 0062) -- and so kept by every device named to keep this
//! one's own files (`qurb holders`). A file deleted from the folder stays
//! with them while one of them holds it, as a phone's do.

use crate::config::Config;
use qurb_storage::{OutsideEntry, OutsideReport, Store};
use std::path::{Path, PathBuf};

/// Where a folder is kept in the vault, under this.
pub fn prefix() -> String {
    format!("{}/folders", qurb_storage::db::OUTSIDE)
}

/// Each folder with the name it is kept under: its own, made unique by a
/// number after it, in the order the setting lists them. The order is the
/// setting's, so the names stay put while the list does.
pub fn named(folders: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut named: Vec<(String, PathBuf)> = Vec::new();
    for folder in folders {
        let own = folder.file_name().and_then(|n| n.to_str()).unwrap_or("Folder").to_string();
        let mut name = own.clone();
        let mut n = 2;
        while named.iter().any(|(taken, _)| *taken == name) {
            name = format!("{own} ({n})");
            n += 1;
        }
        named.push((name, folder.clone()));
    }
    named
}

/// Why a folder cannot be backed up, if it cannot: it must be a directory,
/// and must not overlap the synced folder, which is synced already.
pub fn refused(folder: &Path, root: &Path) -> Option<String> {
    if !folder.is_dir() {
        return Some(format!("{} is not a folder", folder.display()));
    }
    if crate::config::overlaps(folder, root) {
        return Some(format!("{} overlaps your QurbSpace folder, which is synced already", folder.display()));
    }
    None
}

/// The files under `dir`, as `keep_outside_folder` takes them: paths within
/// it, read by their place on disk. Links are not followed, and what cannot
/// be read is left out rather than stopping the rest.
pub fn list(dir: &Path) -> Vec<OutsideEntry> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(at) = pending.pop() {
        let Ok(read) = std::fs::read_dir(&at) else { continue };
        for entry in read.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let Some(relative) = path.strip_prefix(dir).ok().and_then(|p| p.to_str()) else { continue };
            let relative = relative.replace('\\', "/");
            if !qurb_sync::is_safe_path(&relative) {
                continue;
            }
            let mtime_ns = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos() as i64);
            out.push(OutsideEntry {
                path: relative,
                source: path.display().to_string(),
                size: meta.len(),
                mtime_ns,
            });
        }
    }
    out
}

/// Bring the vault into line with each folder the setting names, until
/// `until`: what is left is done on the next pass, since a file already kept
/// and unchanged is not read again. Returns each folder's report.
pub fn run(
    store: &mut Store,
    config: &Config,
    root: &Path,
    until: std::time::Instant,
) -> Vec<(String, anyhow::Result<OutsideReport>)> {
    let mut reports = Vec::new();
    // Offered to the devices keeping this vault: a phone offers its folders
    // only on Wi-Fi (decision 0062), and a computer is taken to be on a
    // network that does not count what it carries -- the bandwidth limit is
    // the way to hold it back.
    if !config.backup_folders.is_empty() {
        if let Err(e) = store.db().set_outside_offered(true) {
            reports.push(("this computer".to_string(), Err(e.into())));
            return reports;
        }
    }
    for (name, folder) in named(&config.backup_folders) {
        if std::time::Instant::now() >= until {
            break;
        }
        if let Some(why) = refused(&folder, root) {
            reports.push((name, Err(anyhow::anyhow!(why))));
            continue;
        }
        let listing = list(&folder);
        let kept = store
            .keep_outside_folder(&format!("{}/{name}", prefix()), &listing, &|_| true, Some(until), &|_| false)
            .map_err(anyhow::Error::from);
        reports.push((name, kept));
    }
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_named_by_their_own_names_made_unique() {
        let named = named(&[PathBuf::from("/a/Photos"), PathBuf::from("/b/Photos"), PathBuf::from("/c/Work")]);
        let names: Vec<&str> = named.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Photos", "Photos (2)", "Work"]);
    }

    #[test]
    fn a_folder_is_listed_with_paths_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("top.txt"), b"1").unwrap();
        std::fs::write(dir.path().join("a/b/deep.txt"), b"22").unwrap();
        let mut listed: Vec<(String, u64)> = list(dir.path()).into_iter().map(|e| (e.path, e.size)).collect();
        listed.sort();
        assert_eq!(listed, [("a/b/deep.txt".to_string(), 2), ("top.txt".to_string(), 1)]);
    }

    #[test]
    fn the_synced_folder_is_not_backed_up_again() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("qurb");
        std::fs::create_dir_all(root.join("inside")).unwrap();
        assert!(refused(&root.join("inside"), &root).is_some());
        assert!(refused(dir.path(), &root).is_some());
        assert!(refused(&dir.path().join("missing"), &root).is_some());
        std::fs::create_dir_all(dir.path().join("Pictures")).unwrap();
        assert!(refused(&dir.path().join("Pictures"), &root).is_none());
    }
}
