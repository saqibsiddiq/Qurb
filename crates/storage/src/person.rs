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
            _ => None,
        },
        None => Some(format!("Private Vault/{path}")),
    }
}

/// The folders and files at `dir` of a folder made of `phones` -- each
/// `(name, device)`, the name its folder is shown under -- from what `db`
/// keeps for them.
pub fn listing(db: &Db, phones: &[(String, DeviceId)], dir: &str) -> Result<(Vec<String>, Vec<Shown>)> {
    let dir = dir.trim_matches('/');
    if dir.is_empty() {
        return Ok((phones.iter().map(|(name, _)| name.clone()).collect(), Vec::new()));
    }
    let phone = dir.split_once('/').map_or(dir, |(phone, _)| phone);
    let Some((name, owner)) = phones.iter().find(|(name, _)| name == phone) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let mut folders = std::collections::BTreeSet::new();
    let mut files = Vec::new();
    for (path, size, mtime, content) in db.held_entries(owner)? {
        let Some(shown) = shown_as(&path) else { continue };
        let shown = format!("{name}/{shown}");
        let Some(rest) = shown.strip_prefix(dir).and_then(|r| r.strip_prefix('/')) else { continue };
        match rest.split_once('/') {
            Some((folder, _)) => {
                folders.insert(folder.to_string());
            }
            None => files.push(Shown {
                name: rest.to_string(),
                path,
                owner: *owner,
                content,
                size,
                modified_at: mtime / 1_000_000_000,
            }),
        }
    }
    Ok((folders.into_iter().collect(), files))
}

/// The folders a phone sees of its own on a computer: its own, and each
/// earlier self's, set up again (`Db::earlier_selves`), named as the window
/// names them.
pub fn own_phones(db: &Db, phone: &DeviceId) -> Result<Vec<(String, DeviceId)>> {
    let peers = db.trusted_peers()?;
    let name = |id: &DeviceId| peers.iter().find(|p| p.device_id == *id).map(|p| p.name.clone()).unwrap_or_default();
    let mut named = vec![(name(phone), *phone)];
    let mut earlier = db.earlier_selves(phone)?;
    // The latest first.
    earlier.sort_by_key(|id| std::cmp::Reverse(peers.iter().find(|p| p.device_id == *id).map(|p| p.paired_at)));
    for (i, id) in earlier.into_iter().enumerate() {
        if db.held_for(&id)?.0 == 0 {
            continue;
        }
        let base = name(&id);
        named.push((if i == 0 { format!("{base}, earlier") } else { format!("{base}, earlier ({})", i + 1) }, id));
    }
    Ok(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_paths_show_where_a_person_looks_for_them() {
        assert_eq!(shown_as(".qurb-outside/whatsapp/Media/a.jpg").as_deref(), Some("WhatsApp/Media/a.jpg"));
        assert_eq!(shown_as(".qurb-outside/storage/DCIM/Screenshots/b.png").as_deref(), Some("DCIM/Screenshots/b.png"));
        assert_eq!(shown_as("Notes/c.md").as_deref(), Some("Private Vault/Notes/c.md"));
        assert_eq!(shown_as(".qurb-outside/other/d"), None);
    }
}
