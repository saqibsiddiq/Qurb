//! Full directory walk.
//!
//! Needed in two situations: at startup, because anything that changed while
//! the process was not running produced no event; and after the platform drops
//! events, because the event stream is then no longer a complete description of
//! what happened.

use crate::error::{Error, Result};
use crate::ignore::IgnoreRules;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    /// Absolute path on this machine.
    pub path: PathBuf,
    /// Path relative to the watched root, as the index stores it.
    pub logical: String,
    pub size: u64,
    pub mtime_ns: i64,
}

/// Walk `root`, returning every file that is not ignored.
///
/// Symbolic links are skipped rather than followed. Following them invites
/// cycles, and a link's *target* is usually outside the synced tree, so copying
/// its contents to another device would be wrong even when it terminates.
/// Representing links properly is a separate feature.
pub fn scan(root: &Path, ignore: &IgnoreRules) -> Result<Vec<ScanEntry>> {
    let mut out = Vec::new();

    let walk = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !ignore.is_ignored(e.path()));

    for entry in walk {
        let entry = entry.map_err(|e| Error::Scan {
            path: e.path().unwrap_or(root).to_path_buf(),
            detail: e.to_string(),
        })?;

        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(logical) = logical_path(root, path) else { continue };

        let meta = entry.metadata().map_err(|e| Error::Scan {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;

        out.push(ScanEntry {
            path: path.to_path_buf(),
            logical,
            size: meta.len(),
            mtime_ns: mtime_ns(&meta),
        });
    }

    out.sort_by(|a, b| a.logical.cmp(&b.logical));
    Ok(out)
}

/// Path relative to the root, with forward slashes.
///
/// The separator is normalised because the logical path is what gets stored in
/// the index and sent to peers. A file synced from Windows as `notes\a.txt`
/// must be the same file on Linux, not a second one.
pub fn logical_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            std::path::Component::Normal(s) => parts.push(s.to_str()?),
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

pub(crate) fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Whether `dir` is on a filesystem that treats `README` and `readme` as the
/// same file.
///
/// Probed rather than assumed from the platform: macOS is case-insensitive by
/// default but can be formatted either way, Windows is case-insensitive, Linux
/// is usually but not always case-sensitive, and a network mount can be
/// anything regardless of the host.
///
/// Returns `false` if the probe cannot be carried out, which is the safe
/// direction: treating a case-insensitive filesystem as sensitive risks losing
/// a file, while the reverse merely declines to merge two paths that were
/// always distinct.
pub fn is_case_insensitive(dir: &Path) -> bool {
    let probe = dir.join(".qurb-case-probe-Aa");
    let other = dir.join(".qurb-case-probe-aA");

    let _ = std::fs::remove_file(&probe);
    let _ = std::fs::remove_file(&other);

    if std::fs::write(&probe, b"probe").is_err() {
        return false;
    }
    let collided = std::fs::metadata(&other).is_ok();
    let _ = std::fs::remove_file(&probe);
    let _ = std::fs::remove_file(&other);
    collided
}

/// Paths that a case-insensitive filesystem cannot keep apart.
///
/// Returns the groups, each holding two or more paths that fold to the same
/// name. On a case-sensitive filesystem they are ordinary distinct files; on a
/// case-insensitive one only one of them can exist, and writing the second
/// destroys the first.
pub fn case_collisions(paths: &[String]) -> Vec<Vec<String>> {
    let mut folded: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for path in paths {
        folded.entry(path.to_lowercase()).or_default().push(path.clone());
    }
    folded.into_values().filter(|group| group.len() > 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("photos")).unwrap();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join(".qurb/chunks")).unwrap();
        fs::write(root.join("notes.txt"), b"hello").unwrap();
        fs::write(root.join("photos/a.jpg"), b"jpeg data").unwrap();
        fs::write(root.join("photos/b.jpg.tmp"), b"partial").unwrap();
        fs::write(root.join(".git/objects/abcdef"), b"git internals").unwrap();
        fs::write(root.join(".qurb/chunks/deadbeef"), b"our own chunk").unwrap();
        dir
    }

    #[test]
    fn scan_finds_files_and_skips_what_it_should() {
        let dir = tree();
        let ignore = IgnoreRules::new().with_store_dir(dir.path().join(".qurb"));
        let found = scan(dir.path(), &ignore).unwrap();

        let logical: Vec<_> = found.iter().map(|e| e.logical.as_str()).collect();
        assert_eq!(logical, vec!["notes.txt", "photos/a.jpg"]);
    }

    #[test]
    fn scan_records_sizes() {
        let dir = tree();
        let ignore = IgnoreRules::new().with_store_dir(dir.path().join(".qurb"));
        let found = scan(dir.path(), &ignore).unwrap();
        let notes = found.iter().find(|e| e.logical == "notes.txt").unwrap();
        assert_eq!(notes.size, 5);
    }

    #[test]
    fn scanning_an_empty_directory_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan(dir.path(), &IgnoreRules::new()).unwrap().is_empty());
    }

    #[test]
    fn logical_paths_use_forward_slashes_and_are_root_relative() {
        let root = Path::new("/home/u/sync");
        assert_eq!(logical_path(root, Path::new("/home/u/sync/a/b.txt")).as_deref(), Some("a/b.txt"));
        assert_eq!(logical_path(root, Path::new("/home/u/sync")), None, "the root itself is not a file");
        assert_eq!(logical_path(root, Path::new("/elsewhere/x")), None, "outside the tree");
    }

    #[test]
    fn this_filesystem_is_probed_not_assumed() {
        // Asserts the probe runs and agrees with what the platform actually
        // does, rather than asserting a particular answer -- the answer depends
        // on the machine the tests are run on.
        let dir = tempfile::tempdir().unwrap();
        let detected = is_case_insensitive(dir.path());

        fs::write(dir.path().join("Probe"), b"x").unwrap();
        let actual = fs::metadata(dir.path().join("probe")).is_ok();
        assert_eq!(detected, actual, "the probe disagreed with the filesystem");
    }

    #[test]
    fn colliding_paths_are_grouped() {
        let paths: Vec<String> = ["README", "readme", "notes.txt", "dir/A.txt", "dir/a.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let groups = case_collisions(&paths);

        assert_eq!(groups.len(), 2, "got {groups:?}");
        assert!(groups.iter().any(|g| g.contains(&"README".to_string())
            && g.contains(&"readme".to_string())));
        assert!(groups.iter().all(|g| !g.contains(&"notes.txt".to_string())));
    }

    #[test]
    fn paths_that_only_look_similar_are_not_collisions() {
        let paths: Vec<String> =
            ["a/b.txt", "a-b.txt", "ab.txt"].iter().map(|s| s.to_string()).collect();
        assert!(case_collisions(&paths).is_empty());
    }

    #[test]
    fn symlinks_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), b"not ours").unwrap();
        fs::write(dir.path().join("real.txt"), b"ours").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();

        let found = scan(dir.path(), &IgnoreRules::new()).unwrap();
        let logical: Vec<_> = found.iter().map(|e| e.logical.as_str()).collect();
        assert_eq!(logical, vec!["real.txt"]);
    }
}
