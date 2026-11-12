//! Resetting this device: QurbSpace leaves it, as if never set up
//! (decision 0086).
//!
//! In two halves, because the daemon has the index open while it runs. The
//! window says goodbye to every paired device and marks the folder
//! ([`mark`]); the next start, before anything opens the index, finishes
//! ([`finish_pending`]): the key forgotten, the store deleted -- the index,
//! and every chunk, the phones' backups kept here among them -- and the
//! read-only placeholders in `~/qurb` removed.
//!
//! Not deleted: the files in the shared folder, which stay as ordinary
//! files, and files sent here, already ordinary files in Downloads.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn marker() -> Result<PathBuf> {
    let base = match std::env::var("XDG_CONFIG_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var("HOME").context("no HOME set")?).join(".config"),
    };
    Ok(base.join("qurb").join("reset"))
}

/// Mark `root` to be reset at the next start.
pub fn mark(root: &Path) -> Result<()> {
    let marker = marker()?;
    if let Some(dir) = marker.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&marker, root.display().to_string()).with_context(|| format!("writing {}", marker.display()))
}

/// Finish a reset marked by an earlier run, if there is one. Returns the
/// folder reset.
pub fn finish_pending() -> Result<Option<PathBuf>> {
    let marker = marker()?;
    let Ok(text) = std::fs::read_to_string(&marker) else { return Ok(None) };
    let root = PathBuf::from(text.trim());
    finish(&root)?;
    let _ = crate::profiles::forget(&root);
    std::fs::remove_file(&marker)?;
    Ok(Some(root))
}

/// Forget the key, delete the store, and remove the placeholders beside the
/// shared folder. The shared folder's own files stay.
pub fn finish(root: &Path) -> Result<()> {
    let store = crate::store_dir(root);
    qurb_keys::Vault::at(&store).erase().context("forgetting the key")?;
    if store.exists() {
        std::fs::remove_dir_all(&store).with_context(|| format!("deleting {}", store.display()))?;
    }
    if let Some(people) = crate::layout::people_dir(root) {
        for entry in std::fs::read_dir(&people).into_iter().flatten().flatten() {
            let dir = entry.path();
            if dir == root || !dir.is_dir() || crate::layout::mounted(&dir) {
                continue;
            }
            // A placeholder holds the note and nothing else; anything more
            // is somebody's, and stays.
            let only_note = std::fs::read_dir(&dir)
                .map(|mut e| e.all(|f| f.is_ok_and(|f| f.file_name() == crate::layout::NOTE)))
                .unwrap_or(false);
            if only_note {
                let note = dir.join(crate::layout::NOTE);
                if let Ok(meta) = std::fs::metadata(&note) {
                    let mut perms = meta.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    perms.set_readonly(false);
                    let _ = std::fs::set_permissions(&note, perms);
                }
                let mut perms = std::fs::metadata(&dir)?.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(&dir, perms);
                let _ = std::fs::remove_file(&note);
                let _ = std::fs::remove_dir(&dir);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reset_deletes_qurbspace_and_leaves_the_files() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("qurb").join("Shared");
        std::fs::create_dir_all(crate::store_dir(&root)).unwrap();
        std::fs::write(root.join("holiday.jpg"), b"a photo").unwrap();
        std::fs::write(crate::store_dir(&root).join("index.db"), b"the index").unwrap();
        qurb_keys::Vault::at(&crate::store_dir(&root)).open_or_create().unwrap();
        crate::layout::place_people(&home.path().join("qurb"), &[("sakib".into(), "a note".into())]).unwrap();
        let mine = home.path().join("qurb").join("mine");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(mine.join("keep.txt"), b"somebody's").unwrap();

        finish(&root).unwrap();

        assert!(!crate::store_dir(&root).exists(), "the store is still there");
        assert!(!crate::is_set_up(&root));
        assert_eq!(std::fs::read(root.join("holiday.jpg")).unwrap(), b"a photo", "a shared file went");
        assert!(!home.path().join("qurb").join("sakib").exists(), "a placeholder stayed");
        assert!(mine.join("keep.txt").exists(), "a folder that is not a placeholder went");
    }
}
