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
use qurb_storage::db;
use qurb_storage::Store;
use qurb_sync::{Action, Content, FileVersion};
use std::io::Write;
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

    /// Write those bytes out instead of returning them.
    ///
    /// The default assembles the whole thing first, which is correct and costs
    /// memory proportional to the file. A source that can stream should say so
    /// by overriding this — on a phone the difference is whether a large file
    /// syncs at all.
    fn fetch_into(&mut self, hash: &[u8; 32], size: u64, out: &mut dyn Write) -> Result<u64> {
        let bytes = self.fetch(hash, size)?;
        out.write_all(&bytes).map_err(|e| Error::Io { path: "the destination".into(), source: e })?;
        Ok(bytes.len() as u64)
    }

    /// Told once content has been committed here, so the source can stop
    /// counting it as undelivered.
    ///
    /// Nothing depends on it arriving — it is a courtesy to the other end, and
    /// a source with nobody to tell does nothing. The default is therefore to
    /// do nothing, which is right for a local store: reading from a directory
    /// on this disk delivers nothing to anyone.
    ///
    /// Called only after the content is written and verified. Reporting a
    /// delivery that then failed would be worse than reporting none, because
    /// it is evidence the other device may drop its own copy on.
    fn received(&mut self, _content: &[u8; 32]) {}
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
        Ok(self.store.read_file(&self.path_for(hash)?)?)
    }

    fn fetch_into(&mut self, hash: &[u8; 32], _size: u64, out: &mut dyn Write) -> Result<u64> {
        let path = self.path_for(hash)?;
        Ok(self.store.read_file_into(&path, &mut Adapter(out))?)
    }
}

impl StoreSource<'_> {
    fn path_for(&self, hash: &[u8; 32]) -> Result<String> {
        let hash = blake3::Hash::from(*hash);
        self.store
            .db()
            .live_path_with_content(&hash)?
            .ok_or_else(|| Error::ContentUnavailable { hash: hex(hash.as_bytes()) })
    }
}

/// Lets a `&mut dyn Write` satisfy an `impl Write` parameter.
struct Adapter<'a>(&'a mut dyn Write);

impl Write for Adapter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
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
        Ok(self.store().shared_tree()?)
    }

    /// What this device would tell one particular peer it has: the shared area,
    /// plus anything sitting in that peer's vault waiting to be collected.
    ///
    /// This is what the network server answers with. [`Engine::tree`] is the
    /// same thing for a peer with nothing waiting.
    pub fn tree_for(&self, peer: &qurb_sync::DeviceId) -> Result<Vec<FileVersion>> {
        Ok(self.store().tree_for(qurb_storage::db::Audience::Device(peer))?)
    }

    /// Work out what should happen, given the peer's view.
    ///
    /// Pure: it reads the index and decides, but changes nothing.
    pub fn plan_against(&self, remote: &[FileVersion]) -> Result<Vec<Action>> {
        // Vault entries are *delivered*, not reconciled. Reconciliation asks
        // which of two histories of a shared path should win; a file somebody
        // sent you has no shared history and no counterpart here to lose to.
        // Running it through the same machinery would have the recipient offer
        // the sender their own file back, and a deletion on either side argue
        // with the other.
        let (offered, shared): (Vec<_>, Vec<_>) =
            remote.iter().cloned().partition(|v| v.private);

        let mut actions = qurb_sync::reconcile(&self.tree()?, &shared);
        actions.extend(self.deliveries(&offered)?);
        Ok(actions)
    }

    /// Content waiting in this device's vault that it has not taken yet.
    ///
    /// Keyed by content rather than by path, and counting tombstones, so that a
    /// delivery is taken exactly once. Keyed by path it would arrive again
    /// under a new name every time the sender reappeared; ignoring tombstones,
    /// deleting something somebody sent you would be impossible.
    fn deliveries(&self, offered: &[FileVersion]) -> Result<Vec<Action>> {
        let mut out = Vec::new();
        for version in offered {
            // A tombstone in a vault is the sender tidying up their side. What
            // the recipient does with content it has already taken is the
            // recipient's business.
            let Some(hash) = version.content.hash() else { continue };
            if self.store().vault_knows(&blake3::Hash::from(*hash))? {
                continue;
            }
            out.push(Action::Adopt { remote: version.clone() });
        }
        Ok(out)
    }

    /// Actions that bring back files whose local copy was dropped.
    ///
    /// These are not part of a reconciliation plan and could not be: this
    /// device and the peer agree completely about such a file — same path,
    /// same content hash, same vector — so there is nothing to reconcile. What
    /// differs is only whether the bytes are here, which is a local matter the
    /// sync protocol has no opinion about.
    ///
    /// So the request is carried in the index, as a flag on the file, and
    /// turned into work here. A request outlives being offline: it is acted on
    /// whenever a peer next becomes reachable, not at the moment it was made.
    pub fn wanted_actions(&self) -> Result<Vec<Action>> {
        let wanted = self.store().db().wanted_paths()?;
        if wanted.is_empty() {
            return Ok(Vec::new());
        }

        Ok(self
            .tree()?
            .into_iter()
            .filter(|version| !version.content.is_deleted() && wanted.contains(&version.path))
            .map(|remote| Action::Adopt { remote })
            .collect())
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
                    crate::note(
                        self.store(),
                        db::Event::Conflicted,
                        Some(&keeps_path.path),
                        None,
                        Some(&renamed.modified_by),
                        Some(&format!("the other version was kept as {}", renamed.path)),
                    );
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
                crate::note(
                    self.store(),
                    db::Event::Failed,
                    Some(action.path()),
                    None,
                    None,
                    Some(&e.to_string()),
                );
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
        // Content sent to this device's vault is filed under a name that is
        // free here. The sender named it the way *they* think of it, so a
        // clash with something the recipient already has is ordinary rather
        // than exceptional -- two people can both have a `report.pdf` -- and
        // neither file may be overwritten.
        let version = &if version.private && !version.is_deleted() {
            match self.store().db().live_path_anywhere(&version.path)? {
                true => {
                    let renamed = qurb_sync::received_path(version);
                    tracing::info!(sent_as = %version.path, filed_as = %renamed, "that name was taken");
                    FileVersion { path: renamed, ..version.clone() }
                }
                false => version.clone(),
            }
        } else {
            version.clone()
        };

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
                // Remember that the device which made this version has the
                // bytes. It is the only evidence available without asking, and
                // it is what later lets the storage cap drop this content: a
                // photo the phone took and sent here may be dropped here,
                // because the phone made it and still has it.
                //
                // Deliberately narrow. Nothing is recorded for content this
                // device originated, so a device can never evict its way out of
                // being the last holder of its own work. See
                // [decision 0025] for what this does not cover.
                //
                // [decision 0025]: ../../../docs/decisions/0025-a-storage-cap-that-cannot-lose-data.md
                if version.modified_by != self.store().device_id()? {
                    let content = blake3::Hash::from(*hash);
                    if version.private {
                        // The sender is holding this *for us*, and will stop as
                        // soon as we confirm we have it. Recorded as the vault
                        // delivery it is, so the storage cap never treats the
                        // sender as a copy this device can fall back on -- the
                        // two of us releasing in turn would lose the file.
                        self.store().note_replica_in_vault(&content, &version.modified_by)?;
                    } else {
                        self.store().note_replica(&content, &version.modified_by)?;
                    }
                }

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

                // A replica holds content without a tree, so there is nowhere
                // to write and nothing to stream to. It still has to avoid
                // holding the file in memory, which is what `adopt` would do,
                // so it takes the buffered path only because there is no file
                // to map -- a gap worth closing when replicas meet large files.
                if self.role().is_replica() {
                    let bytes = match self.store().read_content(&blake3::Hash::from(*hash))? {
                        Some(held) => held,
                        None => {
                            stats.fetched += 1;
                            source.fetch(hash, *size)?
                        }
                    };
                    self.store_mut().adopt(version, Some(&bytes), version.modified_at)?;
                    source.received(hash);
                    return Ok(());
                }

                // Asked before the write, because writing it is what makes it
                // stop being true.
                let was_evicted = self.store().is_materialised(&version.path)? == Some(false);

                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
                }

                // Written beside the destination and moved into place, for two
                // reasons. Content is only verified once its last byte has
                // arrived, so writing straight to the destination would put
                // unverified bytes where the user can see them. And a transfer
                // interrupted halfway would otherwise leave a truncated file
                // that looks complete.
                let staging = staging_path(&path);
                let written = {
                    let mut file = std::fs::File::create(&staging)
                        .map_err(|e| Error::Io { path: staging.clone(), source: e })?;

                    // Prefer content already on this device. A renamed or copied
                    // file, or one that arrived by another route, is already
                    // here under some name.
                    //
                    // Asked by hash rather than by live path: a rename arrives
                    // as additions and deletions applied in path order, so the
                    // old path may already be tombstoned by the time the new one
                    // is written. Its chunks are still on disk.
                    match self.store().read_content_into(
                        &blake3::Hash::from(*hash),
                        &mut file,
                    )? {
                        Some(bytes) => bytes,
                        None => {
                            stats.fetched += 1;
                            source.fetch_into(hash, *size, &mut file)?
                        }
                    }
                };
                let _ = written;

                std::fs::rename(&staging, &path).map_err(|e| {
                    let _ = std::fs::remove_file(&staging);
                    Error::Io { path: path.clone(), source: e }
                })?;

                // Record the modification time the file actually ended up with,
                // so the engine's size-and-mtime fast path recognises it and
                // does not immediately re-read what it just wrote.
                let mtime = std::fs::metadata(&path).ok().map(|m| mtime_ns(&m)).unwrap_or(0);
                if version.private {
                    self.store_mut().adopt_file_privately(version, &path, mtime)?;
                } else {
                    self.store_mut().adopt_file(version, &path, mtime)?;
                }

                // A file this device had dropped to stay under its limit is
                // coming *back*, which is a different thing to tell somebody
                // than a file arriving for the first time -- especially since
                // they are the ones who asked for it.
                let kind = match was_evicted {
                    true => db::Event::Restored,
                    false => db::Event::Received,
                };
                crate::note(
                    self.store(),
                    kind,
                    Some(&version.path),
                    Some(*size),
                    Some(&version.modified_by),
                    version.private.then_some("sent to this device"),
                );

                // Committed, so it is now true to say this device holds it.
                // Told after the rename rather than after the fetch: the point
                // at which this becomes a fact the other end may rely on is the
                // point the file is in place, not the point the bytes arrived.
                source.received(hash);
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

/// Where a file is assembled before being moved into place.
///
/// Beside the destination rather than in a temporary directory, so the move is
/// a rename within one filesystem and therefore atomic. Across filesystems it
/// would be a copy, which reintroduces the half-written file this avoids.
fn staging_path(destination: &Path) -> std::path::PathBuf {
    let name = destination
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "incoming".to_string());
    destination.with_file_name(format!(".{name}.incoming"))
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

