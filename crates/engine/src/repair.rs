//! Putting back what the disk lost.
//!
//! Verification finds chunks that are missing or that no longer hash to their
//! own name. Finding them is not the same as fixing them, and until now nothing
//! did the second part — a corrupt chunk was detected and then left in place,
//! making every file that used it permanently unreadable on this device even
//! though a peer had a perfect copy.
//!
//! Repair is only possible because content is addressed by hash. A damaged
//! chunk is not a lost chunk: it is a chunk whose bytes are wrong, and any peer
//! holding the same content can supply the right ones. Nothing has to be
//! reconciled or agreed — the hash says exactly what is wanted.

use crate::peer::ContentSource;
use crate::{Engine, Error, Result};
use qurb_sync::Content;
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct RepairStats {
    /// Damaged or absent payloads that were discarded and refetched.
    pub chunks_repaired: usize,
    /// Files made readable again.
    pub files_restored: usize,
    /// Files that could not be repaired, with the reason.
    pub unrepairable: Vec<(String, String)>,
}

impl RepairStats {
    pub fn is_clean(&self) -> bool {
        self.unrepairable.is_empty()
    }

    pub fn did_nothing(&self) -> bool {
        self.chunks_repaired == 0 && self.files_restored == 0 && self.unrepairable.is_empty()
    }
}

impl Engine {
    /// Find damaged content and refetch it.
    ///
    /// Reads every chunk, so this is a deliberate maintenance pass rather than
    /// something to run on each change.
    ///
    /// # Order of operations
    ///
    /// The damaged payload is discarded *before* the replacement is fetched.
    /// That looks backwards — it widens the window in which the file is
    /// unreadable — but the alternative is worse: storing the refetched content
    /// while the bad payload is still on disk would find the chunk already
    /// "present" and keep the damaged copy, leaving repair silently doing
    /// nothing. A file that was already unreadable is not made worse by being
    /// briefly more unreadable.
    pub fn repair(&mut self, source: &mut dyn ContentSource) -> Result<RepairStats> {
        let mut stats = RepairStats::default();

        let report = self.store().verify(true)?;
        let damaged: Vec<_> =
            report.missing.iter().chain(report.corrupt.iter()).copied().collect();
        if damaged.is_empty() {
            return Ok(stats);
        }

        tracing::warn!(count = damaged.len(), "found damaged chunks, attempting repair");

        // Which files stopped being readable, before anything is changed.
        let mut affected: BTreeSet<String> = BTreeSet::new();
        for hash in &damaged {
            for path in self.store().db().live_paths_using_chunk(hash)? {
                affected.insert(path);
            }
        }

        for hash in &damaged {
            self.store_mut().discard_payload(hash)?;
            stats.chunks_repaired += 1;
        }

        for path in affected {
            match self.restore_one(&path, source) {
                Ok(()) => stats.files_restored += 1,
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "could not repair");
                    stats.unrepairable.push((path, e.to_string()));
                }
            }
        }

        Ok(stats)
    }

    /// Refetch one file's content and write it back, on disk and in the store.
    fn restore_one(&mut self, path: &str, source: &mut dyn ContentSource) -> Result<()> {
        let version = self
            .store()
            .db()
            .version(path)?
            .ok_or_else(|| Error::Source { detail: format!("{path} vanished from the index") })?;

        let Content::File { hash, size } = version.content else {
            // A tombstone has no content to repair.
            return Ok(());
        };

        let bytes = source.fetch(&hash, size)?;
        // The source is not trusted to send what was asked for. Writing
        // unverified bytes over a file we already know is damaged would turn a
        // detectable problem into an undetectable one.
        if blake3::hash(&bytes) != blake3::Hash::from(hash) {
            return Err(Error::Source {
                detail: format!("{path}: the source supplied content that is not what was asked for"),
            });
        }

        let on_disk = self.root().join(path);
        if let Some(parent) = on_disk.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }
        std::fs::write(&on_disk, &bytes)
            .map_err(|e| Error::Io { path: on_disk.clone(), source: e })?;

        let mtime = std::fs::metadata(&on_disk)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);

        // Keep the version exactly as it was: the content is what this device
        // already believed it had. Stamping a new version would make recovering
        // from a bad disk look like an edit and propagate it to every peer.
        //
        // `rewrite_payloads` rather than `adopt`, because adopt trusts the index
        // when the content hash matches — which it does here, since the bytes
        // are the ones that were supposed to be on disk all along.
        self.store_mut().rewrite_payloads(&version, &bytes, mtime)?;
        Ok(())
    }
}
