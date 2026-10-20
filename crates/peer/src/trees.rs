//! The file lists other devices sent, kept, so that a sync fetches only what
//! changed in one since the last (decisions 0071 and 0072).
//!
//! A phone's list on the computer keeping its WhatsApp backup holds every file
//! of it: on the S23 on 2026-10-19, 37,959 of them, 9.8 MB each time it was
//! fetched. On another phone's hotspot that was more than a 25-second pass
//! could carry, so every pass ran out of time and nothing synced; on mobile
//! data it was 9.8 MB a pass.
//!
//! So a sync asks what changed since the list kept here ([`Request::TreeSince`],
//! by the other device's list counter as it stood then), puts the changed
//! entries in place of the kept ones, and checks the result against the
//! fingerprint of the other device's whole list as it is now. A counter that
//! missed a change therefore costs a fetch, never a wrong list. A device that
//! does not send changes -- too old, too much changed, or a computer keeping
//! this device's vault sealed -- is asked the fingerprint alone, and the list
//! is fetched whole only when it differs from the kept one's.
//!
//! [`Request::TreeSince`]: crate::wire::Request::TreeSince

use crate::client::PeerClient;
use crate::error::{Error, Result};
use crate::identity::Fingerprint;
use crate::wire::{decode_tree_frame, listed_order, tree_digest, tree_frames, Request, Response, MAX_TREE, MAX_TREE_FRAME};
use qurb_sync::FileVersion;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

/// A list kept: the other device's list counter when it was its list, its
/// fingerprint, and its entries in fingerprint order.
#[derive(Debug, Clone)]
pub struct KeptList {
    pub counter: u64,
    pub digest: [u8; 32],
    pub versions: Vec<FileVersion>,
}

/// What changed in a device's list ([`Request::TreeSince`]).
#[derive(Debug, Clone)]
pub struct ListChanges {
    pub now: u64,
    pub digest: [u8; 32],
    pub changed: Vec<FileVersion>,
}

/// Where the lists are kept: a folder of one file for each device, its
/// fingerprint, its counter, and then the list's frames.
#[derive(Debug, Clone)]
pub struct Trees {
    dir: PathBuf,
}

/// Fingerprint and counter, before the frames.
const HEADER: usize = 32 + 8;

impl Trees {
    pub fn at(dir: impl AsRef<Path>) -> Self {
        Self { dir: dir.as_ref().to_path_buf() }
    }

    fn path(&self, peer: &Fingerprint) -> PathBuf {
        self.dir.join(format!("list-{peer}.bin"))
    }

    /// The list kept for `peer`, if there is one and it is whole.
    pub fn load(&self, peer: &Fingerprint) -> Option<KeptList> {
        let bytes = std::fs::read(self.path(peer)).ok()?;
        let digest: [u8; 32] = bytes.get(..32)?.try_into().ok()?;
        let counter = u64::from_le_bytes(bytes.get(32..HEADER)?.try_into().ok()?);
        let mut at = HEADER;
        let mut versions = Vec::new();
        loop {
            let length = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
            at += 4;
            if length == 0 {
                break;
            }
            if length > MAX_TREE_FRAME || at > MAX_TREE {
                return None;
            }
            versions.extend(decode_tree_frame(bytes.get(at..at + length)?).ok()?);
            at += length;
        }
        // What was kept is what was fingerprinted, or it is not used.
        (tree_digest(&versions) == digest).then_some(KeptList { counter, digest, versions })
    }

    /// Keep `versions` as `peer`'s list at `counter`. Best effort: a list not
    /// kept is fetched whole next time, which is how it was before.
    pub fn save(&self, peer: &Fingerprint, counter: u64, versions: &[FileVersion]) {
        let mut sorted: Vec<FileVersion> = versions.to_vec();
        sorted.sort_by(|a, b| listed_order(a).cmp(&listed_order(b)));
        let mut bytes = tree_digest(&sorted).to_vec();
        bytes.extend_from_slice(&counter.to_le_bytes());
        for frame in tree_frames(&sorted) {
            bytes.extend_from_slice(&frame);
        }
        let path = self.path(peer);
        let partial = path.with_extension("partial");
        let kept = std::fs::create_dir_all(&self.dir)
            .and_then(|()| std::fs::write(&partial, &bytes))
            .and_then(|()| std::fs::rename(&partial, &path));
        if let Err(e) = kept {
            tracing::debug!(error = %e, "could not keep a device's file list");
        }
        // As the list's first build kept it, under another name and without
        // a counter.
        let _ = std::fs::remove_file(self.dir.join(format!("tree-{peer}.bin")));
    }

    /// Move the kept list's counter on without writing the list again: it is
    /// the same list, the other device's counter having moved for a change
    /// that left it as it was.
    fn advance(&self, peer: &Fingerprint, counter: u64) {
        let moved = std::fs::OpenOptions::new().write(true).open(self.path(peer)).and_then(|mut file| {
            file.seek(std::io::SeekFrom::Start(32))?;
            file.write_all(&counter.to_le_bytes())
        });
        if let Err(e) = moved {
            tracing::debug!(error = %e, "could not move a kept list's counter on");
        }
    }
}

/// `kept` with `changed` put in place of its entries for the same paths, in
/// fingerprint order.
fn rebuilt(kept: Vec<FileVersion>, changed: Vec<FileVersion>) -> Vec<FileVersion> {
    let mut by_entry: std::collections::BTreeMap<(u8, String), FileVersion> = kept
        .into_iter()
        .map(|version| {
            let (class, path) = listed_order(&version);
            ((class, path.to_string()), version)
        })
        .collect();
    for version in changed {
        let (class, path) = listed_order(&version);
        by_entry.insert((class, path.to_string()), version);
    }
    by_entry.into_values().collect()
}

impl PeerClient {
    /// What changed in this device's list since its counter stood at `since`,
    /// or `None` from a device too old to say -- which fails the one request,
    /// and the connection goes on -- or with too much changed to send apart.
    pub async fn tree_since(&self, since: u64) -> Result<Option<ListChanges>> {
        match self.request(Request::TreeSince { since }).await {
            Ok(Response::TreeDelta { now, digest, changed }) => Ok(Some(ListChanges { now, digest, changed })),
            Ok(_) => Ok(None),
            // The connection itself gone is not an old device.
            Err(Error::Connection(e)) => Err(Error::Connection(e)),
            Err(e) => {
                tracing::debug!(error = %e, "no changes to its file list; it will be fetched whole");
                Ok(None)
            }
        }
    }

    /// The other device's file list: the one kept in `trees`, brought up to
    /// date with what changed in it, if that comes out as the other device's
    /// list; else fetched whole, and kept.
    pub async fn tree_kept(&self, peer: &Fingerprint, trees: &Trees) -> Result<Vec<FileVersion>> {
        if let Some(kept) = trees.load(peer) {
            match self.tree_since(kept.counter).await? {
                Some(changes) => {
                    if changes.digest == kept.digest {
                        if changes.now != kept.counter {
                            trees.advance(peer, changes.now);
                        }
                        tracing::debug!(peer = %peer.short(), files = kept.versions.len(), "its file list unchanged");
                        return Ok(kept.versions);
                    }
                    let count = changes.changed.len();
                    let versions = rebuilt(kept.versions, changes.changed);
                    if tree_digest(&versions) == changes.digest {
                        trees.save(peer, changes.now, &versions);
                        tracing::debug!(peer = %peer.short(), changed = count, files = versions.len(), "its file list brought up to date");
                        return Ok(versions);
                    }
                    tracing::info!(peer = %peer.short(), changed = count, "its file list rebuilt did not match; fetching it whole");
                }
                // Not sending changes -- a computer keeping this device's
                // vault sealed, or too much changed -- it may still say
                // whether the list is the one kept (decision 0071).
                None => {
                    if self.tree_digest().await? == Some(kept.digest) {
                        tracing::debug!(peer = %peer.short(), files = kept.versions.len(), "its file list unchanged");
                        return Ok(kept.versions);
                    }
                }
            }
        }
        // Where its counter stands, asked first, so that the list fetched is
        // no older than the counter kept with it. Asking for nothing changed
        // since the end of time is how.
        let mark = self.tree_since(i64::MAX as u64).await?;
        let versions = self.tree().await?;
        trees.save(peer, mark.map_or(0, |m| m.now), &versions);
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qurb_sync::{DeviceId, VersionVector};

    fn version(path: &str, hash: u8) -> FileVersion {
        let mut vector = VersionVector::new();
        vector.set(DeviceId::from_bytes([7; 32]), hash as u64);
        FileVersion::file(path, [hash; 32], 10, vector, DeviceId::from_bytes([9; 32]), 1_760_000_000)
    }

    #[test]
    fn a_list_rebuilt_from_its_changes_is_the_list() {
        let mut before: Vec<FileVersion> = (0..1000).map(|i| version(&format!("f{i:04}"), 1)).collect();
        // One sent into the asker's vault, which the other device then holds:
        // the same entry, its area changed.
        before[500].area = qurb_sync::Area::Sent;
        let mut after = before.clone();
        after[10] = version("f0010", 2);
        after.push(version("new", 3));
        let mut held = version("f0500", 1);
        held.area = qurb_sync::Area::Held;
        after[500] = held.clone();
        let changed = vec![after[10].clone(), version("new", 3), held];
        assert_eq!(tree_digest(&rebuilt(before, changed)), tree_digest(&after));
    }

    #[test]
    fn a_kept_list_comes_back_as_it_was_kept_and_a_damaged_one_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let trees = Trees::at(dir.path());
        let peer = Fingerprint::from_bytes([5; 32]);
        let versions: Vec<FileVersion> = (0..300).map(|i| version(&format!("f{i:03}"), 1)).collect();
        trees.save(&peer, 42, &versions);
        let kept = trees.load(&peer).unwrap();
        assert_eq!((kept.counter, kept.digest), (42, tree_digest(&versions)));
        trees.advance(&peer, 50);
        assert_eq!(trees.load(&peer).unwrap().counter, 50);

        let path = trees.path(&peer);
        let mut bytes = std::fs::read(&path).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        std::fs::write(&path, bytes).unwrap();
        assert!(trees.load(&peer).is_none(), "a damaged list was used");
    }
}
