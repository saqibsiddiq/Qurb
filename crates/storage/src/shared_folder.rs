//! A computer's shared folder as a phone reaches it, live (decision 0073,
//! item 6): one folder of it at a time -- what the index has there, and the
//! empty folders on disk, which the index cannot know -- leaving out every
//! folder shared without the asking device (decision 0044).

use crate::db::Db;
use crate::error::Result;
use crate::person::Shown;
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
    for (path, size, mtime, content) in db.shared_entries_under(dir, device)? {
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
