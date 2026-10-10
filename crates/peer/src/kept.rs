//! A device's own vault, kept sealed by a computer of another person
//! (decision 0060): fetched back, unsealed and checked; and learned from that
//! computer's list by a device set up again with the same key.
//!
//! The computer was shown only sealed names and sealed files, so what comes
//! back from it is sealed too, and nothing the ordinary transfer does fits:
//! the content hash it knows is the sealed file's, not the file's. So this
//! asks for the sealed chunks itself, opens them with the key only this
//! person holds, and checks the result against the content hash in the
//! sealed header before anything is put back.

use crate::client::PeerClient;
use qurb_storage::{sealed, Store};
use qurb_sync::{Area, Content, DeviceId, FileVersion};
use std::io::Write;

/// Fetch back each file of this device's vault that is wanted here and kept,
/// sealed, by `host`. `tree` is what `host` showed this device. Returns how
/// many came back.
///
/// Best effort, file by file: one that cannot be fetched, opened or checked
/// stays wanted, and is tried again at the next sync.
pub async fn fetch_kept(client: &PeerClient, store: &mut Store, host: &DeviceId, tree: &[FileVersion]) -> usize {
    let key = sealed::FolderKey::for_host(&store.chunk_key(), host);
    let Ok(wanted) = store.db().wanted_paths() else { return 0 };
    let mut back = 0;
    for path in wanted {
        let Ok(Some(row)) = store.db().own_vault_row(&path) else { continue };
        let Some(name) = sealed::seal_name(&key, &path) else { continue };
        let Some(Content::File { hash, .. }) =
            tree.iter().find(|v| v.area == Area::Held && v.path == name).map(|v| v.content.clone())
        else {
            continue;
        };
        let staging = store.root().join(format!("unsealing-{}", row.id));
        match unseal(client, &key, &hash, &path, &row.content_hash, &staging).await {
            Ok(()) => match store.restore_kept(&path, &staging) {
                Ok(()) => back += 1,
                Err(e) => tracing::warn!(path, error = %e, "a kept file came back and could not be put back"),
            },
            Err(why) => tracing::warn!(path, why, "a kept file could not be fetched back"),
        }
        let _ = std::fs::remove_file(&staging);
    }
    back
}

/// Learn what `host` keeps of this device's vault that this device does not
/// know of: a phone set up again, from Block Store or a code, with the same
/// key. Each is listed, kept there, and fetched when opened. Reads one small
/// sealed header per file. Returns how many were learned.
pub async fn learn_kept(client: &PeerClient, store: &mut Store, host: &DeviceId, tree: &[FileVersion]) -> usize {
    let key = sealed::FolderKey::for_host(&store.chunk_key(), host);
    let Ok(me) = store.device_id() else { return 0 };
    let mut learned = 0;
    for entry in tree.iter().filter(|v| v.area == Area::Held) {
        let Content::File { hash, .. } = entry.content else { continue };
        let Some(path) = sealed::open_name(&key, &entry.path) else { continue };
        if !matches!(store.db().own_vault_row(&path), Ok(None)) {
            continue;
        }
        let Ok(Some(meta)) = header(client, &key, &hash).await else { continue };
        if meta.path != path {
            continue;
        }
        let version = FileVersion {
            path,
            content: Content::File { hash: *meta.content.as_bytes(), size: meta.size },
            vector: entry.vector.clone(),
            modified_by: me,
            modified_at: meta.modified_at,
            area: Area::Shared,
        };
        if matches!(store.know_kept(&version, host), Ok(true)) {
            learned += 1;
        }
    }
    learned
}

/// The sealed header of a sealed file `host` keeps: read from its first
/// chunks until it has all arrived.
async fn header(
    client: &PeerClient,
    key: &sealed::FolderKey,
    sealed_hash: &[u8; 32],
) -> crate::Result<Option<sealed::Meta>> {
    let Some(ids) = client.manifest(*sealed_hash).await? else { return Ok(None) };
    let mut reader = sealed::Unsealer::new(key.clone());
    for id in &ids {
        let Some(piece) = client.chunk(*id).await? else { return Ok(None) };
        if blake3::hash(&piece).as_bytes() != id || reader.push(&piece).is_err() {
            return Ok(None);
        }
        if let Some(meta) = reader.meta() {
            return Ok(Some(meta.clone()));
        }
    }
    Ok(None)
}

/// Fetch a sealed file, open it as it arrives into `staging`, and check it is
/// the file `path` with content `expected`. Why not, in words, if it is not.
///
/// The computer keeping it cut it into chunks of its own, so the pieces are
/// read as one stream, whatever their boundaries.
async fn unseal(
    client: &PeerClient,
    key: &sealed::FolderKey,
    sealed_hash: &[u8; 32],
    path: &str,
    expected: &blake3::Hash,
    staging: &std::path::Path,
) -> std::result::Result<(), String> {
    let ids = client
        .manifest(*sealed_hash)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("the computer does not have it")?;
    let mut reader = sealed::Unsealer::new(key.clone());
    let mut out = std::fs::File::create(staging).map_err(|e| e.to_string())?;
    let mut whole = blake3::Hasher::new();
    let mut checked = false;
    for id in &ids {
        let piece = fetch(client, id).await?;
        for plain in reader.push(&piece)? {
            whole.update(&plain);
            out.write_all(&plain).map_err(|e| e.to_string())?;
        }
        if !checked {
            if let Some(meta) = reader.meta() {
                if meta.path != path || meta.content != *expected {
                    return Err("it is not the file asked for".into());
                }
                checked = true;
            }
        }
    }
    if !reader.finished() {
        return Err("it ended early".into());
    }
    out.sync_all().map_err(|e| e.to_string())?;
    if whole.finalize() != *expected {
        return Err("what came back is not what was kept".into());
    }
    Ok(())
}

/// One chunk of what the computer keeps, checked against its id.
async fn fetch(client: &PeerClient, id: &[u8; 32]) -> std::result::Result<Vec<u8>, String> {
    let piece = client.chunk(*id).await.map_err(|e| e.to_string())?.ok_or("a chunk is missing")?;
    if blake3::hash(&piece).as_bytes() != id {
        return Err("a chunk is not what it says".into());
    }
    Ok(piece)
}
