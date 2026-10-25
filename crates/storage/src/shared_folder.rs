//! A computer's shared folder as a phone reaches it, live (decision 0073,
//! item 6): one folder of it at a time -- the files the index has there with
//! their bytes on this disk, and the empty folders on disk, which the index
//! cannot know -- leaving out every folder shared without the asking device
//! (decision 0044). A file listed with no bytes here is not in the folder,
//! and not shown (the owner's list of 2026-10-26, item 14).

use crate::db::Db;
use crate::error::{Error, Result};
use crate::person::Shown;
use crate::store::Store;
use qurb_sync::DeviceId;
use std::collections::BTreeSet;
use std::path::Path;

/// The folders and files at `dir` of the shared area, as `device` may see
/// them. `folder` is the shared folder on disk, when this device has one;
/// `here` is this device, which every file is shown as kept by.
pub fn listing(
    db: &Db,
    folder: Option<&Path>,
    here: &DeviceId,
    device: &DeviceId,
    dir: &str,
) -> Result<(Vec<String>, Vec<Shown>)> {
    let dir = dir.trim_matches('/');
    if !dir.is_empty() && (!qurb_sync::is_safe_path(dir) || !db.shared_path_reachable(dir, device)?) {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut folders = BTreeSet::new();
    let mut files = Vec::new();
    for (path, size, mtime, content) in db.shared_entries_here(dir, device)? {
        if qurb_sync::sharing::is_rule_path(&path) {
            continue;
        }
        let rest = if dir.is_empty() { path.as_str() } else { &path[dir.len() + 1..] };
        match rest.split_once('/') {
            Some((name, _)) => {
                folders.insert(name.to_string());
            }
            None => files.push(Shown {
                name: rest.to_string(),
                path: path.clone(),
                owner: *here,
                content,
                size,
                modified_at: mtime / 1_000_000_000,
            }),
        }
    }
    // Folders on disk with nothing in them yet, made here or from a phone.
    // Not the store's own, nor anything hidden, nor a link out of the folder:
    // a hidden folder is never read from disk, even asked for by name, or
    // `.qurb` would list the store (it did, in the first build's test).
    let hidden = dir.split('/').any(|part| part.starts_with('.'));
    if let (Some(root), false) = (folder, hidden) {
        if let Ok(read) = std::fs::read_dir(root.join(dir)) {
            for entry in read.flatten() {
                let Ok(kind) = entry.file_type() else { continue };
                let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
                if !kind.is_dir() || name.starts_with('.') {
                    continue;
                }
                let path = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
                if db.shared_path_reachable(&path, device)? {
                    folders.insert(name);
                }
            }
        }
    }
    Ok((folders.into_iter().collect(), files))
}

/// The files of the shared area whose path holds `text`, ignoring case, as
/// `device` may see them, the newest first and at most `limit` (decision 0073,
/// item 4). Each is named by its whole path, which a list of results shows.
pub fn search(db: &Db, here: &DeviceId, device: &DeviceId, text: &str, limit: usize) -> Result<Vec<Shown>> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut found: Vec<Shown> = db
        .shared_entries_here("", device)?
        .into_iter()
        .filter(|(path, ..)| !qurb_sync::sharing::is_rule_path(path) && path.to_lowercase().contains(&text))
        .map(|(path, size, mtime, content)| Shown {
            name: path.clone(),
            path,
            owner: *here,
            content,
            size,
            modified_at: mtime / 1_000_000_000,
        })
        .collect();
    found.sort_by(|a, b| b.modified_at.cmp(&a.modified_at).then_with(|| a.name.cmp(&b.name)));
    found.truncate(limit);
    Ok(found)
}

// ---------------------------------------------------------------- changes
//
// Made by this computer on its own shared folder when one of its own
// person's phones, or a guest, asks (decision 0073, item 6): the exception to
// a device serving read-only. Each is an ordinary change made here, which
// every other device then takes as one. A reason a person can read comes
// back as `Error::Sharing`, as `Store::rename_file` already says one.

/// The most copies of one name an added file is numbered up to before it is
/// refused: `photo (2).jpg`, `photo (3).jpg`, ...
const NUMBERED_UP_TO: u32 = 1000;

/// `path`, as a place a phone may change in the shared folder: a name a file
/// can have, nothing hidden -- which keeps the store's own folder and the
/// sharing rules out of reach -- and outside every folder shared without it.
fn checked(store: &Store, device: &DeviceId, path: &str) -> Result<String> {
    // Relative to the shared folder: one that starts at the root of the disk
    // is refused, not quietly taken as inside it.
    let path = path.trim_end_matches('/');
    if path.is_empty()
        || path.starts_with('/')
        || !qurb_sync::is_safe_path(path)
        || qurb_sync::sharing::is_rule_path(path)
        || path.split('/').any(|part| part.starts_with('.'))
    {
        return Err(Error::Sharing { why: format!("{path:?} is not a name a file can have here") });
    }
    if !store.db().shared_path_reachable(path, device)? {
        return Err(Error::Sharing { why: format!("{path} is not in the shared folder") });
    }
    Ok(path.to_string())
}

fn folder(store: &Store) -> Result<std::path::PathBuf> {
    store
        .tree_root()
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::Sharing { why: "this computer keeps no shared folder".into() })
}

fn taken(store: &Store, root: &Path, path: &str) -> Result<bool> {
    Ok(store.db().live_path_anywhere(path)? || root.join(path).symlink_metadata().is_ok())
}

/// Whether `path` is a live file of the shared area here.
fn shared_file(store: &Store, path: &str) -> Result<bool> {
    Ok(matches!(store.db().folder_row(path)?, Some((_, None))))
}

/// Make a folder, empty, in the shared folder.
pub fn make_folder(store: &mut Store, device: &DeviceId, path: &str) -> Result<()> {
    let path = checked(store, device, path)?;
    let root = folder(store)?;
    if taken(store, &root, &path)? {
        return Err(Error::Sharing { why: format!("something is already called {path}") });
    }
    std::fs::create_dir_all(root.join(&path))
        .map_err(|_| Error::Sharing { why: format!("{path} cannot be made here") })
}

/// Rename or move a file or a folder of the shared folder. A folder moves
/// file by file, each a rename every device takes; one with a sharing rule
/// in it, shared with some devices only, is not moved from a phone.
pub fn move_path(store: &mut Store, device: &DeviceId, from: &str, to: &str) -> Result<()> {
    let from = checked(store, device, from)?;
    let to = checked(store, device, to)?;
    if from == to {
        return Ok(());
    }
    let root = folder(store)?;
    if to.starts_with(&format!("{from}/")) {
        return Err(Error::Sharing { why: format!("{from} cannot go inside itself") });
    }
    if taken(store, &root, &to)? {
        return Err(Error::Sharing { why: format!("something is already called {to}") });
    }
    if let Some(parent) = Path::new(&to).parent() {
        std::fs::create_dir_all(root.join(parent)).map_err(|e| Error::io(root.join(parent), e))?;
    }
    if shared_file(store, &from)? {
        return store.rename_file(&from, &to);
    }
    if !root.join(&from).is_dir() {
        return Err(Error::NotFound { path: from });
    }
    let inside = store.db().shared_entries_under(&from, device)?;
    if store.db().shared_under(&from)?.iter().any(|(path, _)| qurb_sync::sharing::is_rule_path(path)) {
        return Err(Error::Sharing { why: format!("{from} is shared with some devices only, and is moved at the computer") });
    }
    if inside.is_empty() {
        return std::fs::rename(root.join(&from), root.join(&to)).map_err(|e| Error::io(root.join(&from), e));
    }
    for (path, ..) in inside {
        let rest = &path[from.len() + 1..];
        let target = format!("{to}/{rest}");
        if let Some(parent) = Path::new(&target).parent() {
            std::fs::create_dir_all(root.join(parent)).map_err(|e| Error::io(root.join(parent), e))?;
        }
        store.rename_file(&path, &target)?;
    }
    remove_empty_folders(&root.join(&from));
    Ok(())
}

/// Delete a file or a folder of the shared folder, into this computer's
/// Recently deleted for 30 days, naming `device` as who did it. A folder
/// with a sharing rule in it is not deleted from a phone.
pub fn delete_path(store: &mut Store, device: &DeviceId, path: &str) -> Result<()> {
    let path = checked(store, device, path)?;
    let root = folder(store)?;
    if shared_file(store, &path)? {
        store.delete_to_trash_by(&path, None, device)?;
        return Ok(());
    }
    if !root.join(&path).is_dir() {
        return Err(Error::NotFound { path });
    }
    if store.db().shared_under(&path)?.iter().any(|(p, _)| qurb_sync::sharing::is_rule_path(p)) {
        return Err(Error::Sharing { why: format!("{path} is shared with some devices only, and is deleted at the computer") });
    }
    for (file, ..) in store.db().shared_entries_under(&path, device)? {
        store.delete_to_trash_by(&file, None, device)?;
    }
    remove_empty_folders(&root.join(&path));
    Ok(())
}

/// Put a file a phone sent, whole in `staged`, at `path` in the shared
/// folder -- once its bytes are what the phone said they are -- and index it
/// as a change made here. Numbered when the name is taken, never put over
/// another file. Returns where it was put.
pub fn place(store: &mut Store, device: &DeviceId, staged: &Path, path: &str, content: &blake3::Hash) -> Result<String> {
    let path = checked(store, device, path)?;
    let root = folder(store)?;
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(staged).map_err(|e| Error::io(staged, e))?;
    std::io::copy(&mut file, &mut hasher).map_err(|e| Error::io(staged, e))?;
    if hasher.finalize() != *content {
        return Err(Error::Sharing { why: "the file arrived damaged; send it again".into() });
    }
    let mut placed = path.clone();
    let mut n = 1;
    while taken(store, &root, &placed)? {
        n += 1;
        if n > NUMBERED_UP_TO {
            return Err(Error::Sharing { why: format!("too many files are called {path}") });
        }
        placed = numbered(&path, n);
    }
    let target = root.join(&placed);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    if std::fs::rename(staged, &target).is_err() {
        std::fs::copy(staged, &target).map_err(|e| Error::io(&target, e))?;
        let _ = std::fs::remove_file(staged);
    }
    store.put_file(&placed, &target)?;
    Ok(placed)
}

/// `Photos/beach.jpg` as `Photos/beach (n).jpg`.
pub(crate) fn numbered(path: &str, n: u32) -> String {
    let (dir, name) = match path.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name),
        None => (String::new(), path),
    };
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{dir}{stem} ({n}).{ext}"),
        _ => format!("{dir}{name} ({n})"),
    }
}

/// Folders left with nothing in them, from the deepest up: what moving or
/// deleting a folder file by file leaves behind. Anything still in one keeps
/// it.
fn remove_empty_folders(at: &Path) {
    if let Ok(read) = std::fs::read_dir(at) {
        for entry in read.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                remove_empty_folders(&entry.path());
            }
        }
    }
    let _ = std::fs::remove_dir(at);
}

#[cfg(test)]
mod tests {
    use super::numbered;

    #[test]
    fn a_taken_name_is_numbered_before_its_extension() {
        assert_eq!(numbered("Photos/beach.jpg", 2), "Photos/beach (2).jpg");
        assert_eq!(numbered("notes", 3), "notes (3)");
        assert_eq!(numbered(".profile", 2), ".profile (2)");
        assert_eq!(numbered("a.tar.gz", 2), "a.tar (2).gz");
    }
}
