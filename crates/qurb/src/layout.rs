//! `~/qurb` laid out by person (decision 0067, step 2).
//!
//! The owner's answer of 2026-10-14: `~/qurb` holds only people's folders.
//! The folder that copies to every one of their devices is `~/qurb/Shared`,
//! and beside it each person this computer keeps files for has
//! `~/qurb/<name>/`, a placeholder: their files are kept encrypted in Qurb's
//! index and open only inside Qurb, with a yes on their phone. The
//! placeholder holds one note saying so, and is read-only, so that nothing
//! dropped into it is mistaken for kept.
//!
//! A folder set up before this is moved once, while Qurb is stopped: its
//! contents, index included, into `Shared`. Paths inside it are relative to
//! it, so nothing in the index changes and no other device notices.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// The shared folder's name inside `~/qurb`.
pub const SHARED: &str = "Shared";

/// The note in each person's placeholder.
pub const NOTE: &str = "Opens in Qurb.txt";

/// Whether `root` is a folder of the old shape: set up, with its index at its
/// top rather than in `Shared`, and so to be moved.
pub fn needs_moving(root: &Path) -> bool {
    crate::is_set_up(root) && !crate::is_set_up(&root.join(SHARED))
}

/// Move everything in `root` into `root/Shared`, the index last, and return
/// the new folder. Undone, as far as it got, if any move fails: a folder
/// half in and half out is worse than one not moved. Remembered in place of
/// the old one, so the command line and the window open the new one.
///
/// Only for a stopped Qurb: a daemon watching `root` would see its files go.
pub fn move_into_shared(root: &Path) -> Result<PathBuf> {
    if !needs_moving(root) {
        bail!("{} is not a folder to move", root.display());
    }
    let shared = root.join(SHARED);
    if shared.exists() && std::fs::read_dir(&shared)?.next().is_some() {
        bail!("{} already exists and is not empty", shared.display());
    }
    std::fs::create_dir_all(&shared).with_context(|| format!("creating {}", shared.display()))?;

    let index = crate::store_dir(root);
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| *p != shared && *p != index)
        .collect();
    entries.sort();
    entries.push(index);

    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for from in entries {
        let to = shared.join(from.file_name().expect("an entry has a name"));
        if let Err(e) = std::fs::rename(&from, &to) {
            for (back, was) in moved.iter().rev() {
                let _ = std::fs::rename(was, back);
            }
            let _ = std::fs::remove_dir(&shared);
            return Err(e).with_context(|| format!("moving {} into {}", from.display(), shared.display()));
        }
        moved.push((from, to));
    }

    let _ = crate::profiles::forget(root);
    crate::profiles::remember(&shared)?;
    tracing::info!(from = %root.display(), to = %shared.display(), "moved the shared folder into place");
    Ok(shared)
}

/// The folder to run, once laid out: `~/qurb` of the old shape is moved
/// into `~/qurb/Shared` and that returned; anything else is returned as it
/// is. Only the default place moves -- a folder somebody chose elsewhere is
/// theirs to arrange. A move that fails is logged, and the folder runs where
/// it was.
pub fn settle(root: PathBuf) -> PathBuf {
    let default = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("qurb"));
    if default.as_deref() != Some(root.as_path()) || !needs_moving(&root) {
        return root;
    }
    match move_into_shared(&root) {
        Ok(shared) => shared,
        Err(e) => {
            tracing::warn!(error = %format!("{e:#}"), "could not move the shared folder into place");
            root
        }
    }
}

/// The folder people's placeholders go in, if `root` is laid out by person:
/// the parent of a folder named `Shared`.
pub fn people_dir(root: &Path) -> Option<PathBuf> {
    (root.file_name()? == SHARED).then(|| root.parent().map(Path::to_path_buf)).flatten()
}

/// A name as a folder: no separators, not hidden, not empty, not `Shared`.
fn folder_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '-' } else { c }).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    match cleaned.as_str() {
        "" => "Someone".to_string(),
        n if n.eq_ignore_ascii_case(SHARED) => format!("{n} (person)"),
        n => n.to_string(),
    }
}

/// A placeholder for each of `people` -- `(name, what the note says)` -- in
/// `dir`, and none for anyone gone: a placeholder holding nothing but its
/// note is removed when its person is no longer listed. Nothing else in `dir`
/// is touched.
pub fn place_people(dir: &Path, people: &[(String, String)]) -> Result<()> {
    let wanted: Vec<(String, &String)> = people.iter().map(|(name, note)| (folder_name(name), note)).collect();
    for (name, note) in &wanted {
        let folder = dir.join(name);
        let file = folder.join(NOTE);
        if std::fs::read_to_string(&file).ok().as_deref() == Some(note.as_str()) {
            continue;
        }
        std::fs::create_dir_all(&folder).with_context(|| format!("creating {}", folder.display()))?;
        writable(&folder, true);
        let written = std::fs::write(&file, note.as_bytes());
        writable(&folder, false);
        written.with_context(|| format!("writing {}", file.display()))?;
    }
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == SHARED || name.starts_with('.') || wanted.iter().any(|(w, _)| *w == name) || !path.is_dir() {
            continue;
        }
        let only_note = std::fs::read_dir(&path)
            .map(|mut inside| inside.all(|e| e.map(|e| e.file_name() == NOTE).unwrap_or(false)))
            .unwrap_or(false);
        if only_note {
            writable(&path, true);
            let _ = std::fs::remove_file(path.join(NOTE));
            let _ = std::fs::remove_dir(&path);
        }
    }
    Ok(())
}

/// A placeholder is read-only, so that nothing put in it is taken for kept.
fn writable(folder: &Path, on: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(folder, std::fs::Permissions::from_mode(if on { 0o755 } else { 0o555 }));
    }
    // Windows' read-only attribute on a folder does not stop anything being
    // put in it, so there a placeholder is only its note.
    #[cfg(not(unix))]
    let _ = (folder, on);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Setting up and moving record the folder in the list of known ones,
    /// under `XDG_CONFIG_HOME`: kept out of the real list, once for the whole
    /// test binary, as `tests/setup.rs` does.
    fn isolated() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let scratch = std::env::temp_dir().join(format!("qurb-layout-tests-{}", std::process::id()));
            std::fs::create_dir_all(&scratch).expect("scratch config directory");
            std::env::set_var("XDG_CONFIG_HOME", &scratch);
        });
    }

    fn set_up(root: &Path) {
        isolated();
        crate::setup::create(root).unwrap();
    }

    /// A folder of the old shape moves into `Shared`, index and files alike,
    /// and the move is remembered; one already moved is left alone.
    #[test]
    fn a_folder_moves_into_shared_once() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("qurb");
        set_up(&root);
        std::fs::create_dir_all(root.join("Photos")).unwrap();
        std::fs::write(root.join("Photos/cat.jpg"), b"a cat").unwrap();
        std::fs::write(root.join("notes.md"), b"notes").unwrap();
        assert!(needs_moving(&root));

        let shared = move_into_shared(&root).unwrap();
        assert_eq!(shared, root.join(SHARED));
        assert!(crate::is_set_up(&shared), "the index did not move");
        assert_eq!(std::fs::read(shared.join("Photos/cat.jpg")).unwrap(), b"a cat");
        assert_eq!(std::fs::read(shared.join("notes.md")).unwrap(), b"notes");
        let left: Vec<_> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from(SHARED)], "something stayed behind");
        assert_eq!(crate::profiles::known().first(), Some(&shared));
        assert!(!needs_moving(&root));
        assert!(move_into_shared(&root).is_err(), "moved twice");
        assert_eq!(people_dir(&shared), Some(root.clone()));
        assert_eq!(people_dir(&root), None);
    }

    /// Each person gets a read-only placeholder with its note; one no longer
    /// listed goes, if it holds nothing but its note; anything else stays.
    #[test]
    fn placeholders_follow_the_people() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(SHARED)).unwrap();
        let note = |who: &str| format!("{who}'s folder opens in Qurb.\n");
        place_people(dir.path(), &[("Saqib".into(), note("Saqib")), ("Ammi".into(), note("Ammi"))]).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("Saqib").join(NOTE)).unwrap(), note("Saqib"));
        assert!(std::fs::write(dir.path().join("Saqib/dropped.txt"), b"x").is_err(), "a placeholder took a file");

        // Somebody's own folder of the same shape is not ours to remove.
        std::fs::create_dir_all(dir.path().join("Projects")).unwrap();
        std::fs::write(dir.path().join("Projects/plan.md"), b"mine").unwrap();
        place_people(dir.path(), &[("Saqib".into(), note("Saqib"))]).unwrap();
        assert!(!dir.path().join("Ammi").exists(), "a person's placeholder outlived them");
        assert!(dir.path().join("Projects/plan.md").exists(), "a folder that is not a placeholder went");
        assert!(dir.path().join(SHARED).exists());

        // Names that cannot be folders become ones that can.
        assert_eq!(folder_name("a/b"), "a-b");
        assert_eq!(folder_name(".hidden"), "hidden");
        assert_eq!(folder_name("shared"), "shared (person)");
        assert_eq!(folder_name("  "), "Someone");
    }
}
