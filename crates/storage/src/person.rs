//! A person's folder as it is shown (decision 0067): one folder per phone,
//! and in each what a computer keeps for it -- WhatsApp under *WhatsApp*,
//! each backed-up folder under its own name, and the phone's Private Vault
//! under *Private Vault*. One definition, so that the window and the phone
//! draw the same tree.

use crate::db::{Db, OUTSIDE};
use crate::error::Result;
use qurb_sync::DeviceId;

/// One file in a person's folder, at one place in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// Its name, as the folder shows it.
    pub name: String,
    /// Where it is kept, in its phone's vault.
    pub path: String,
    /// The phone it is kept for.
    pub owner: DeviceId,
    pub content: blake3::Hash,
    pub size: u64,
    /// Unix seconds.
    pub modified_at: i64,
}

/// Where a kept file shows in its phone's folder, or `None` for qurb's own
/// bookkeeping.
pub fn shown_as(path: &str) -> Option<String> {
    let outside = format!("{OUTSIDE}/");
    match path.strip_prefix(&outside) {
        Some(rest) => match rest.split_once('/') {
            Some(("whatsapp", rest)) => Some(format!("WhatsApp/{rest}")),
            Some(("storage", rest)) => Some(rest.to_string()),
            // A computer's own folders, backed up here (`qurb_cli::backup`).
            Some(("folders", rest)) => Some(rest.to_string()),
            _ => None,
        },
        None => Some(format!("Private Vault/{path}")),
    }
}

/// One device's folder: the name it is shown under, and every identity it
/// has had -- the current one first, then each it was before it was set up
/// again (`Db::earlier_selves`), the latest first. One device is one folder,
/// however many identities it has had: on 2026-11-01 the owner's S23, three
/// identities after two reinstalls, showed as three folders.
pub type DeviceFolder = (String, Vec<DeviceId>);

/// Everything kept for one device's identities, each place in its folder
/// once: where two identities kept a file at the same place -- a phone set
/// up again backs up the same photos again -- the newest identity's.
fn merged(db: &Db, name: &str, ids: &[DeviceId]) -> Result<Vec<(String, Shown)>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for owner in ids {
        for (path, size, mtime, content) in db.held_entries(owner)? {
            let Some(shown) = shown_as(&path) else { continue };
            let shown = format!("{name}/{shown}");
            if !seen.insert(shown.clone()) {
                continue;
            }
            out.push((shown, Shown { name: String::new(), path, owner: *owner, content, size, modified_at: mtime / 1_000_000_000 }));
        }
    }
    Ok(out)
}

/// Every folder of one device's folder, each with the folders and files in
/// it, its paths from the device's folder down ("" its top): read once, for
/// a mounted folder to answer each look from rather than reading everything
/// kept for the device again for each (2026-11-10: a file manager opening a
/// folder of the S23's, thousands of files kept, looked up each name in it,
/// and each look read all of them).
pub fn tree(db: &Db, device: &DeviceFolder) -> Result<std::collections::HashMap<String, (Vec<String>, Vec<Shown>)>> {
    let (name, ids) = device;
    let mut dirs: std::collections::HashMap<String, (std::collections::BTreeSet<String>, Vec<Shown>)> =
        std::collections::HashMap::new();
    dirs.entry(String::new()).or_default();
    for (shown, mut file) in merged(db, name, ids)? {
        let Some(inside) = shown.strip_prefix(name.as_str()).and_then(|r| r.strip_prefix('/')) else { continue };
        let (parent, file_name) = inside.rsplit_once('/').unwrap_or(("", inside));
        // Each folder above it, named in its own parent.
        let mut at = parent;
        while !at.is_empty() {
            let (up, folder) = at.rsplit_once('/').unwrap_or(("", at));
            if !dirs.entry(up.to_string()).or_default().0.insert(folder.to_string()) {
                break;
            }
            at = up;
        }
        file.name = file_name.to_string();
        dirs.entry(parent.to_string()).or_default().1.push(file);
    }
    Ok(dirs
        .into_iter()
        .map(|(dir, (folders, mut files))| {
            files.sort_by(|a, b| a.name.cmp(&b.name));
            (dir, (folders.into_iter().collect(), files))
        })
        .collect())
}

/// The folders and files at `dir` of a person's folder made of `devices`.
pub fn listing(db: &Db, devices: &[DeviceFolder], dir: &str) -> Result<(Vec<String>, Vec<Shown>)> {
    let dir = dir.trim_matches('/');
    if dir.is_empty() {
        return Ok((devices.iter().map(|(name, _)| name.clone()).collect(), Vec::new()));
    }
    let top = dir.split_once('/').map_or(dir, |(top, _)| top);
    let Some((name, ids)) = devices.iter().find(|(name, _)| name == top) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut folders = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    for (shown, mut file) in merged(db, name, ids)? {
        let Some(rest) = shown.strip_prefix(dir).and_then(|r| r.strip_prefix('/')) else { continue };
        match rest.split_once('/') {
            Some((folder, _)) => {
                folders.insert(folder.to_string());
            }
            None => {
                file.name = rest.to_string();
                files.push(file);
            }
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((folders.into_iter().collect(), files))
}

/// The files of a person's folder whose place in it holds `text`, ignoring
/// case, the newest first and at most `limit` (decision 0073, item 4). Each
/// is named by its place in the folder, from the device's name down, which
/// is what a list of results shows.
pub fn search(db: &Db, devices: &[DeviceFolder], text: &str, limit: usize) -> Result<Vec<Shown>> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for (name, ids) in devices {
        for (shown, mut file) in merged(db, name, ids)? {
            if shown.to_lowercase().contains(&text) {
                file.name = shown;
                found.push(file);
            }
        }
    }
    found.sort_by(|a, b| b.modified_at.cmp(&a.modified_at).then_with(|| a.name.cmp(&b.name)));
    found.truncate(limit);
    Ok(found)
}

/// The folder a phone sees of its own on a computer: one, under its own name,
/// made of its identity and every one it had before (`Db::earlier_selves`)
/// that this computer keeps anything for.
pub fn own_phones(db: &Db, phone: &DeviceId) -> Result<Vec<DeviceFolder>> {
    let peers = db.trusted_peers()?;
    let name = peers.iter().find(|p| p.device_id == *phone).map(|p| p.name.clone()).unwrap_or_default();
    let mut ids = vec![*phone];
    for id in db.earlier_selves(phone)? {
        if db.held_for(&id)?.0 > 0 {
            ids.push(id);
        }
    }
    Ok(vec![(name, ids)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_paths_show_where_a_person_looks_for_them() {
        assert_eq!(shown_as(".qurb-outside/whatsapp/Media/a.jpg").as_deref(), Some("WhatsApp/Media/a.jpg"));
        assert_eq!(shown_as(".qurb-outside/folders/Pictures/a.jpg").as_deref(), Some("Pictures/a.jpg"));
        assert_eq!(shown_as(".qurb-outside/storage/DCIM/Screenshots/b.png").as_deref(), Some("DCIM/Screenshots/b.png"));
        assert_eq!(shown_as("Notes/c.md").as_deref(), Some("Private Vault/Notes/c.md"));
        assert_eq!(shown_as(".qurb-outside/other/d"), None);
    }

    /// One phone with three identities -- set up again twice -- is one
    /// folder: what each kept, merged, and a file kept by two at the same
    /// place shown once, the newest identity's (2026-11-01).
    #[test]
    fn a_device_with_several_identities_is_one_folder() {
        let db = Db::open_in_memory().unwrap();
        let (now, before, first) = (DeviceId::from_bytes([1; 32]), DeviceId::from_bytes([2; 32]), DeviceId::from_bytes([3; 32]));
        let keep = |owner: &DeviceId, path: &str, content: u8| {
            db.conn_for_tests()
                .execute(
                    "INSERT INTO files (path, size, content_hash, mtime_ns, created_at, updated_at, scope, held)
                     VALUES (?1, 1, ?2, 0, 0, 0, ?3, 1)",
                    rusqlite::params![path, vec![content; 32], owner.as_bytes().as_slice()],
                )
                .unwrap();
        };
        keep(&now, ".qurb-outside/storage/DCIM/a.jpg", 1);
        keep(&before, ".qurb-outside/storage/DCIM/a.jpg", 9);
        keep(&before, ".qurb-outside/whatsapp/Media/w.jpg", 2);
        keep(&first, "notes.txt", 3);
        let devices = vec![("S23".to_string(), vec![now, before, first])];

        let (top, _) = listing(&db, &devices, "").unwrap();
        assert_eq!(top, vec!["S23".to_string()], "one device, one folder");
        let (folders, _) = listing(&db, &devices, "S23").unwrap();
        assert_eq!(folders, vec!["DCIM".to_string(), "Private Vault".to_string(), "WhatsApp".to_string()]);
        let (_, dcim) = listing(&db, &devices, "S23/DCIM").unwrap();
        assert_eq!(dcim.len(), 1, "a file kept twice shown twice");
        assert_eq!(dcim[0].owner, now, "the older identity's copy shown over the newer's");
        assert_eq!(search(&db, &devices, "w.jpg", 10).unwrap()[0].owner, before);
    }
}
