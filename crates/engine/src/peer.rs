//! Comparing with another device, and acting on the result.
//!
//! [`qurb_sync`] decides *what* should happen when two devices disagree. This
//! module carries it out: writing files, recording vectors, and asking for the
//! bytes it does not have.
//!
//! ```text
//!   tree()          what this device would tell a peer it has
//!   plan_against()  qurb_sync::reconcile, given the peer's tree
//!   apply_plan()    do the local half, fetching content as needed
//! ```
//!
//! The network does not exist yet. [`ContentSource`] is the seam where it will
//! go: everything above it is finished, and everything below it is a trait with
//! one method.

use crate::{Engine, Error, Result};
use qurb_storage::Store;
use qurb_sync::{Action, Content, FileVersion};
use std::path::Path;


/// Supplies file content by hash.
///
/// Keyed on content rather than path deliberately. The content a plan calls for
/// may live under a different name on the device that has it — that is exactly
/// the case when a conflict renames a file, or when something was renamed
/// locally — and asking for a path would fail where asking for bytes succeeds.
pub trait ContentSource {
    /// Produce the bytes with this BLAKE3 hash.
    fn fetch(&mut self, hash: &[u8; 32], size: u64) -> Result<Vec<u8>>;
}

/// A source that has nothing, for plans expected not to need content.
pub struct NoContent;

impl ContentSource for NoContent {
    fn fetch(&mut self, hash: &[u8; 32], _size: u64) -> Result<Vec<u8>> {
        Err(Error::ContentUnavailable { hash: hex(hash) })
    }
}

/// Serves content out of a local store.
///
/// Used in tests to stand in for a peer, and useful on its own for copying
/// between two stores on one machine.
pub struct StoreSource<'a> {
    store: &'a Store,
}

impl<'a> StoreSource<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }
}

impl ContentSource for StoreSource<'_> {
    fn fetch(&mut self, hash: &[u8; 32], _size: u64) -> Result<Vec<u8>> {
        let hash = blake3::Hash::from(*hash);
        let path = self
            .store
            .db()
            .live_path_with_content(&hash)?
            .ok_or_else(|| Error::ContentUnavailable { hash: hex(hash.as_bytes()) })?;
        Ok(self.store.read_file(&path)?)
    }
}

/// What applying a plan did.
#[derive(Debug, Default)]
pub struct PlanStats {
    /// Versions taken from the peer, content and tombstones alike.
    pub adopted: usize,
    /// Histories converged without moving any data.
    pub merged: usize,
    /// Conflicts resolved. Each writes two files.
    pub conflicts: usize,
    /// Files brought back after a concurrent delete lost to an edit.
    pub resurrected: usize,
    /// Actions that are the peer's to perform. Counted, not done.
    pub offered: usize,
    /// Content that had to come from the source rather than from disk.
    pub fetched: usize,
    pub failures: Vec<crate::FileFailure>,
}

impl PlanStats {
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}

impl Engine {
    /// What this device would tell a peer it has.
    ///
    /// Tombstones included. A peer not told about a deletion still holds the
    /// file, offers it back, and the deletion undoes itself.
    pub fn tree(&self) -> Result<Vec<FileVersion>> {
        Ok(self.store().tree()?)
    }

    /// Work out what should happen, given the peer's view.
    ///
    /// Pure: it reads the index and decides, but changes nothing.
    pub fn plan_against(&self, remote: &[FileVersion]) -> Result<Vec<Action>> {
        Ok(qurb_sync::reconcile(&self.tree()?, remote))
    }

    /// Carry out the local half of a plan.
    ///
    /// [`Action::Offer`] is the peer's work and is only counted here. Everything
    /// else is applied, fetching content from `source` when this device does not
    /// already hold it.
    ///
    /// A failure on one path is recorded and the rest of the plan continues, for
    /// the same reason the rest of the engine works that way: one unreadable
    /// file must not leave everything else unsynced.
    pub fn apply_plan(
        &mut self,
        actions: &[Action],
        source: &mut dyn ContentSource,
    ) -> Result<PlanStats> {
        let mut stats = PlanStats::default();

        // Additions before removals. Within one plan a rename is both, and
        // applying the removal first would throw away content the addition is
        // about to want -- turning a free rename into a full re-transfer. It
        // also shortens the window in which a renamed file exists at neither
        // path.
        // A replica holding a subset ignores what it was not asked to hold.
        let mut ordered: Vec<&Action> =
            actions.iter().filter(|a| self.role().wants(a.path())).collect();
        ordered.sort_by_key(|action| match action {
            Action::Adopt { remote } if remote.is_deleted() => 1,
            _ => 0,
        });

        for action in ordered {
            let outcome = match action {
                Action::Offer { .. } => {
                    stats.offered += 1;
                    continue;
                }
                Action::Merge { resolved } => {
                    stats.merged += 1;
                    self.store_mut().merge_version(resolved).map_err(Error::from)
                }
                Action::Adopt { remote } => {
                    stats.adopted += 1;
                    self.take(remote, source, &mut stats)
                }
                Action::Resurrect { resolved } => {
                    stats.resurrected += 1;
                    self.take(resolved, source, &mut stats)
                }
                Action::Conflict { keeps_path, renamed } => {
                    stats.conflicts += 1;
                    // Order matters. The renamed copy is written first, so an
                    // interruption leaves the losing version saved beside the
                    // original rather than lost with the original overwritten.
                    self.take(renamed, source, &mut stats)
                        .and_then(|()| self.take(keeps_path, source, &mut stats))
                }
            };

            if let Err(e) = outcome {
                let path = self.root().join(action.path());
                tracing::warn!(path = %path.display(), error = %e, "plan step failed, continuing");
                stats.failures.push(crate::FileFailure { path, error: e });
            }
        }

        Ok(stats)
    }

    /// Make this device hold `version`, on disk and in the index.
    fn take(
        &mut self,
        version: &FileVersion,
        source: &mut dyn ContentSource,
        stats: &mut PlanStats,
    ) -> Result<()> {
        let path = self.root().join(&version.path);

        match &version.content {
            Content::Deleted => {
                // A replica records the tombstone but has no file to remove.
                if !self.role().is_replica() {
                    match std::fs::remove_file(&path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(Error::Io { path: path.clone(), source: e }),
                    }
                }
                self.store_mut().adopt(version, None, 0)?;
                if !self.role().is_replica() {
                    prune_empty_parents(&path, self.root());
                }
            }
            Content::File { hash, size } => {
                // Refuse a path this filesystem cannot keep separate from one
                // already here. Writing it would destroy the existing file, and
                // the next reconciliation would report that loss as a deletion
                // and push it to every other device -- one confusing sync
                // turning into data loss everywhere.
                if self.folds_case() {
                    if let Some(existing) =
                        self.store().db().live_path_colliding_with(&version.path)?
                    {
                        return Err(Error::CaseCollision {
                            wanted: version.path.clone(),
                            existing,
                        });
                    }
                }

                // Prefer content already on this device. A renamed or copied
                // file, or one that arrived by another route, is already here
                // under some name, and fetching it again would be pure waste.
                //
                // Asked by hash rather than by live path: a rename arrives as
                // additions and deletions applied in path order, so the old path
                // may already be tombstoned by the time the new one is written.
                // Its chunks are still on disk, and they are what matters.
                let bytes = match self.store().read_content(&blake3::Hash::from(*hash))? {
                    Some(held) => held,
                    None => {
                        stats.fetched += 1;
                        source.fetch(hash, *size)?
                    }
                };

                if self.role().is_replica() {
                    // Storage only. Materialising the file as well would cost
                    // roughly twice the space for a copy nobody reads, and
                    // would make a filesystem the replica does not really have
                    // authoritative for what it holds.
                    self.store_mut().adopt(version, Some(&bytes), version.modified_at)?;
                } else {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
                    }
                    std::fs::write(&path, &bytes)
                        .map_err(|e| Error::Io { path: path.clone(), source: e })?;

                    // Record the modification time the file actually ended up
                    // with, so the engine's size-and-mtime fast path recognises
                    // it and does not immediately re-read what it just wrote.
                    let mtime =
                        std::fs::metadata(&path).ok().map(|m| mtime_ns(&m)).unwrap_or(0);
                    self.store_mut().adopt(version, Some(&bytes), mtime)?;
                }
            }
        }
        Ok(())
    }
}

/// Remove directories left empty by a deletion, up to but never including the
/// synced root.
///
/// Without this, renaming a directory leaves the other device holding a skeleton
/// of empty folders where the old tree was. The index is correct and every file
/// is in the right place, but what the user sees is their old directory still
/// sitting there, apparently half-deleted.
///
/// Failures are ignored on purpose: a directory that is not empty, or that
/// another process is using, is not a problem worth failing a sync over.
fn prune_empty_parents(removed: &Path, root: &Path) {
    let mut dir = match removed.parent() {
        Some(d) => d.to_path_buf(),
        None => return,
    };

    while dir.starts_with(root) && dir != root {
        if std::fs::remove_dir(&dir).is_err() {
            // Not empty, or not removable. Either way there is nothing above it
            // worth trying.
            return;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return,
        }
    }
}

fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

