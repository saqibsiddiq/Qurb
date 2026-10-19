//! The file lists other devices sent, kept with their fingerprints, so that a
//! sync whose other side has not changed need not fetch its list again.
//!
//! A phone's list on the computer keeping its WhatsApp backup holds every file
//! of it: on the S23 on 2026-10-19, 37,959 of them, 9.8 MB received each pass
//! with nothing changed. On another phone's hotspot that was more than the
//! pass's 25 seconds could carry, so every pass ran out of time before the list
//! arrived and nothing ever synced; on mobile data it was 9.8 MB each pass.
//!
//! So a sync first asks for the list's fingerprint ([`Request::TreeDigest`]),
//! 32 bytes, and fetches the list only if it differs from the one kept here
//! from last time. A device too old to answer is asked for the list, as
//! before.
//!
//! [`Request::TreeDigest`]: crate::wire::Request::TreeDigest

use crate::client::PeerClient;
use crate::error::Result;
use crate::identity::Fingerprint;
use crate::wire::{decode_tree_frame, tree_digest, tree_frames, MAX_TREE, MAX_TREE_FRAME};
use qurb_sync::FileVersion;
use std::path::{Path, PathBuf};

/// Where the lists are kept: a folder of one file for each device, its
/// fingerprint and then the list's frames as they are sent.
#[derive(Debug, Clone)]
pub struct Trees {
    dir: PathBuf,
}

impl Trees {
    pub fn at(dir: impl AsRef<Path>) -> Self {
        Self { dir: dir.as_ref().to_path_buf() }
    }

    fn path(&self, peer: &Fingerprint) -> PathBuf {
        self.dir.join(format!("tree-{peer}.bin"))
    }

    /// The list kept for `peer`, if it is the one with `digest`.
    pub fn get(&self, peer: &Fingerprint, digest: &[u8; 32]) -> Option<Vec<FileVersion>> {
        let bytes = std::fs::read(self.path(peer)).ok()?;
        if bytes.get(..32)? != digest {
            return None;
        }
        let mut at = 32;
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
        (tree_digest(&versions) == *digest).then_some(versions)
    }

    /// Keep `versions` as `peer`'s list. Best effort: a list not kept is
    /// fetched again next time, which is how it was before.
    pub fn put(&self, peer: &Fingerprint, versions: &[FileVersion]) {
        let digest = tree_digest(versions);
        let mut bytes = digest.to_vec();
        for frame in tree_frames(versions) {
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
    }
}

impl PeerClient {
    /// The other device's file list: the one kept in `trees` from an earlier
    /// sync if it has not changed since, else fetched and kept.
    pub async fn tree_kept(&self, peer: &Fingerprint, trees: &Trees) -> Result<Vec<FileVersion>> {
        if let Some(digest) = self.tree_digest().await? {
            if let Some(versions) = trees.get(peer, &digest) {
                tracing::debug!(peer = %peer.short(), files = versions.len(), "its file list unchanged");
                return Ok(versions);
            }
        }
        let versions = self.tree().await?;
        trees.put(peer, &versions);
        Ok(versions)
    }
}
