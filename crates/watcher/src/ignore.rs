//! What not to watch.
//!
//! Two categories, for different reasons.
//!
//! **The store itself.** The chunk store and index live inside the watched
//! tree in most deployments. Watching them would make every write trigger
//! events that cause more writes. This one is not a preference, it is a
//! correctness requirement.
//!
//! **Editor and download scratch files.** Applications routinely write
//! `document.txt.tmp`, rename it over `document.txt`, and delete the original.
//! Syncing the intermediate states wastes transfer and produces files on other
//! devices that vanish seconds later.

use std::path::{Component, Path, PathBuf};

/// Directory names skipped wherever they appear in the tree.
///
/// Version control metadata is excluded because it is large, changes
/// constantly, and is meaningless without the working tree it belongs to.
const IGNORED_DIRS: &[&str] = &[".git", ".svn", ".hg", ".bzr", "node_modules", "__pycache__"];

/// Files that are never worth syncing.
const IGNORED_NAMES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

/// Suffixes marking work in progress.
///
/// `.incoming` is ours: a file arriving from a peer is assembled beside its
/// destination and moved into place once verified. Without this line the
/// watcher would index the half-written file, then see it vanish at the rename,
/// and send both the phantom and its deletion to every other device.
const IGNORED_SUFFIXES: &[&str] = &[
    ".tmp",
    ".temp",
    ".swp",
    ".swx",
    ".part",
    ".partial",
    ".crdownload",
    ".download",
    ".incoming",
    "~",
];

/// Prefixes marking work in progress: Emacs lock files, Office lock files,
/// LibreOffice lock files, and our own interrupted chunk writes.
const IGNORED_PREFIXES: &[&str] = &[".#", "~$", ".~lock."];

#[derive(Debug, Clone)]
pub struct IgnoreRules {
    /// Absolute path to the store directory, if it lives inside the watched
    /// tree. Everything beneath it is ignored.
    store_dir: Option<PathBuf>,
    extra_dirs: Vec<String>,
}

impl IgnoreRules {
    pub fn new() -> Self {
        Self { store_dir: None, extra_dirs: Vec::new() }
    }

    /// Exclude the store's own directory. Call this whenever the store lives
    /// inside the tree being watched.
    pub fn with_store_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.store_dir = Some(dir.into());
        self
    }

    /// Exclude an additional directory name, matched anywhere in the tree.
    pub fn ignoring_dir(mut self, name: impl Into<String>) -> Self {
        self.extra_dirs.push(name.into());
        self
    }

    /// Whether this path should be skipped.
    ///
    /// Takes absolute paths. A path inside an ignored directory is ignored even
    /// if the path itself looks ordinary, so a whole subtree disappears at once.
    pub fn is_ignored(&self, path: &Path) -> bool {
        if let Some(store) = &self.store_dir {
            if path == store || path.starts_with(store) {
                return true;
            }
        }

        for component in path.components() {
            let Component::Normal(os) = component else { continue };
            let Some(name) = os.to_str() else { continue };

            if IGNORED_DIRS.contains(&name) || self.extra_dirs.iter().any(|d| d == name) {
                return true;
            }
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        is_ignored_name(name)
    }
}

impl Default for IgnoreRules {
    fn default() -> Self {
        Self::new()
    }
}

fn is_ignored_name(name: &str) -> bool {
    if IGNORED_NAMES.contains(&name) {
        return true;
    }
    if IGNORED_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return true;
    }
    // Case-insensitive: Windows and macOS filesystems routinely differ in case
    // from what an application wrote.
    let lower = name.to_ascii_lowercase();
    IGNORED_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> IgnoreRules {
        IgnoreRules::new().with_store_dir("/home/u/sync/.qurb")
    }

    #[test]
    fn ordinary_files_are_watched() {
        let r = rules();
        assert!(!r.is_ignored(Path::new("/home/u/sync/notes.txt")));
        assert!(!r.is_ignored(Path::new("/home/u/sync/photos/trip.jpg")));
        assert!(!r.is_ignored(Path::new("/home/u/sync/.config-backup")));
    }

    #[test]
    fn the_store_is_never_watched() {
        // Watching our own writes would feed back into itself.
        let r = rules();
        assert!(r.is_ignored(Path::new("/home/u/sync/.qurb")));
        assert!(r.is_ignored(Path::new("/home/u/sync/.qurb/index.db")));
        assert!(r.is_ignored(Path::new("/home/u/sync/.qurb/chunks/9f/86d0")));
    }

    #[test]
    fn version_control_metadata_is_skipped_at_any_depth() {
        let r = rules();
        assert!(r.is_ignored(Path::new("/home/u/sync/proj/.git/objects/ab/cdef")));
        assert!(r.is_ignored(Path::new("/home/u/sync/a/b/node_modules/x/y.js")));
    }

    #[test]
    fn editor_scratch_files_are_skipped() {
        let r = rules();
        for name in [
            "doc.txt.tmp",
            "doc.txt.swp",
            "movie.mp4.part",
            "installer.exe.crdownload",
            "doc.txt~",
            ".#doc.txt",
            "~$report.docx",
            ".~lock.sheet.ods#",
            ".DS_Store",
        ] {
            let p = format!("/home/u/sync/{name}");
            assert!(r.is_ignored(Path::new(&p)), "{name} should be ignored");
        }
    }

    #[test]
    fn suffix_matching_is_case_insensitive() {
        let r = rules();
        assert!(r.is_ignored(Path::new("/home/u/sync/Setup.TMP")));
        assert!(r.is_ignored(Path::new("/home/u/sync/Video.Part")));
    }

    #[test]
    fn a_file_merely_containing_an_ignored_word_is_kept() {
        let r = rules();
        assert!(!r.is_ignored(Path::new("/home/u/sync/tmp-notes.txt")));
        assert!(!r.is_ignored(Path::new("/home/u/sync/git-guide.md")));
    }

    /// The staging name the engine writes must be one the watcher skips. These
    /// live in different crates, so nothing but this test connects them.
    #[test]
    fn staging_files_are_ignored() {
        let rules = IgnoreRules::new();
        assert!(rules.is_ignored(Path::new("/tree/.report.pdf.incoming")));
        assert!(!rules.is_ignored(Path::new("/tree/report.pdf")));
    }
}
