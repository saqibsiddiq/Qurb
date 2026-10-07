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
use qurb_sync::{Action, Area, Content, DeviceId, FileVersion};
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

    /// Fetch into the file at `partial`, keeping whatever of the content an
    /// earlier attempt that was cut off left there, and reporting the bytes
    /// written -- kept ones included -- to `progress`.
    ///
    /// The default starts again from nothing, which is right for a source that
    /// cannot tell what the partial file holds. A network source can, and
    /// should override this: a large file from a phone may take several of its
    /// short windows to arrive, and starting each from zero means it never
    /// does.
    fn resume_into(
        &mut self,
        hash: &[u8; 32],
        size: u64,
        partial: &Path,
        progress: &mut dyn Progress,
    ) -> Result<u64> {
        let mut file = std::fs::File::create(partial)
            .map_err(|e| Error::Io { path: partial.to_path_buf(), source: e })?;
        self.fetch_into(hash, size, &mut Counting { out: &mut file, progress })
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

/// Told about content as it arrives from another device, so that something
/// can show a transfer moving.
///
/// Only content that actually crosses from the source is reported. A file
/// whose bytes were already here under another name -- a rename, a copy, an
/// earlier version -- costs a lookup, not a transfer, and showing it as one
/// would be showing work that is not happening.
///
/// `finished` is called whether or not the fetch succeeded, and before the
/// file is moved into place. It means "no longer in flight", never "arrived":
/// what arrived is the activity record, written once the file is committed.
pub trait Progress {
    /// The content of `path`, `size` bytes, is about to be fetched.
    fn started(&mut self, _path: &str, _size: u64) {}
    /// `bytes` more of it have been written.
    fn advanced(&mut self, _bytes: u64) {}
    /// It is no longer in flight.
    fn finished(&mut self, _path: &str) {}
}

/// Nobody is watching.
pub struct NoProgress;

impl Progress for NoProgress {}

/// Counts what passes through to `out`, for [`Progress`].
struct Counting<'a> {
    out: &'a mut dyn Write,
    progress: &'a mut dyn Progress,
}

impl Write for Counting<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.out.write(buf)?;
        self.progress.advanced(n as u64);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
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

// By content, the way a peer is asked over the network (decision 0010). Asking
// by path could never serve content held for another device's vault: there is
// no path for it in this device's folder.
impl ContentSource for StoreSource<'_> {
    fn fetch(&mut self, hash: &[u8; 32], _size: u64) -> Result<Vec<u8>> {
        self.store
            .read_content(&blake3::Hash::from(*hash))?
            .ok_or_else(|| Error::ContentUnavailable { hash: hex(hash) })
    }

    fn fetch_into(&mut self, hash: &[u8; 32], _size: u64, out: &mut dyn Write) -> Result<u64> {
        self.store
            .read_content_into(&blake3::Hash::from(*hash), &mut Adapter(out))?
            .ok_or_else(|| Error::ContentUnavailable { hash: hex(hash) })
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
    /// Another device's own files taken or dropped here on its behalf
    /// (decision 0036). Kept apart from `adopted`: nothing about them is this
    /// device's own, or anybody else's news.
    pub held: usize,
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
    /// This device's shared area, all of it: its own side of a plan.
    ///
    /// Tombstones included. A peer not told about a deletion still holds the
    /// file, offers it back, and the deletion undoes itself. What a *peer* is
    /// shown is [`tree_for`](Self::tree_for), which leaves out folders shared
    /// without it (decision 0044).
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
        let shared: Vec<FileVersion> =
            remote.iter().filter(|v| v.area == Area::Shared).cloned().collect();
        // Held and hold entries are for holding a vault, decision 0036, and
        // are not acted on here: holding is a different operation from both
        // converging and collecting.
        let offered: Vec<FileVersion> =
            remote.iter().filter(|v| v.area == Area::Sent).cloned().collect();

        let mut actions = qurb_sync::reconcile(&self.tree()?, &shared);
        actions.extend(self.deliveries(&offered)?);
        Ok(actions)
    }

    /// The same, knowing which device `remote` came from.
    ///
    /// Holding needs to know: an entry marked *hold* is that device's own file,
    /// kept here in its vault, and nothing in the entry says whose vault that
    /// is -- the file's author need not be its owner. And this device's own
    /// freed files are fetched back from the device holding them, which only
    /// its entries marked *held* say. See decision 0036.
    ///
    /// And files this device asked to have back, from whichever peer it is
    /// syncing with ([`wanted_actions`](Self::wanted_actions)). That used to be
    /// added by the desktop daemon after calling this, and the phone, which
    /// calls this too, never added it: asking for a freed shared file back on
    /// a phone did nothing, pass after pass. One planning function for both.
    pub fn plan_with(&self, remote: &[FileVersion], peer: Option<&DeviceId>) -> Result<Vec<Action>> {
        let remote = self.shared_with(remote, peer)?;
        let remote = remote.as_slice();
        let mut actions = self.plan_against(remote)?;
        if let Some(owner) = peer {
            actions.extend(self.holding(remote, owner)?);
        }
        actions.extend(self.own_wanted(remote)?);
        actions.extend(self.wanted_actions()?);
        Ok(actions)
    }

    /// What of a peer's list this device takes, given the sharing rules
    /// (decision 0044).
    ///
    /// The peer's own server already leaves out what it should not show this
    /// device, but by its copy of the rules, which may be behind; and a device
    /// left out of a folder still has its old copy, and still offers it. So
    /// this device checks by its own rules too: nothing under a folder shared
    /// without the peer, or without this device, is taken from it -- and a
    /// change to a folder's rule is taken only from a device the folder is
    /// shared with, so a device left out cannot write itself back in.
    ///
    /// What is left out looks, to the planner, like something the peer simply
    /// does not have, which is exactly what it should be.
    fn shared_with(&self, remote: &[FileVersion], peer: Option<&DeviceId>) -> Result<Vec<FileVersion>> {
        let rules = self.store().sharing()?;
        if rules.is_empty() && !remote.iter().any(|v| qurb_sync::sharing::is_rule_path(&v.path)) {
            return Ok(remote.to_vec());
        }
        let me = self.store().device_id()?;
        Ok(remote
            .iter()
            .filter(|version| {
                if version.area != Area::Shared {
                    return true;
                }
                if let Some(folder) = qurb_sync::sharing::rule_folder(&version.path) {
                    let allowed = peer.is_some_and(|p| rules.may_change(&folder, p));
                    if !allowed {
                        tracing::warn!(folder, "refused a sharing change from a device the folder is not shared with");
                    }
                    return allowed;
                }
                match rules.covering(&version.path) {
                    None => true,
                    Some((_, members)) => {
                        members.contains(&me) && peer.is_some_and(|p| members.contains(p))
                    }
                }
            })
            .cloned()
            .collect())
    }

    /// What to keep, or drop, for `owner`.
    ///
    /// Only on instruction. A file missing from the owner's list is left
    /// alone: a phone that was wiped, or lost its index, must not delete its
    /// own backup here at the first sync. Only a tombstone drops a file.
    fn holding(&self, remote: &[FileVersion], owner: &DeviceId) -> Result<Vec<Action>> {
        let db = self.store().db();
        let mut out = Vec::new();
        for version in remote.iter().filter(|v| v.area == Area::Hold) {
            let existing = db.live_row_in(&version.path, Some(owner))?;
            let needed = match (&version.content, &existing) {
                (Content::Deleted, None) => false,
                (Content::Deleted, Some(_)) => true,
                (Content::File { hash, .. }, Some(row)) => {
                    row.content_hash != blake3::Hash::from(*hash)
                        || !db.is_held(&version.path, owner)?
                }
                (Content::File { .. }, None) => true,
            };
            if needed {
                out.push(Action::Hold { owner: *owner, remote: version.clone() });
            }
        }
        Ok(out)
    }

    /// This device's own files it asked to have back, from a device that
    /// holds them.
    ///
    /// Only what that device's entries say it has, byte for byte: asking a
    /// device that does not hold a file would fail, and a failure is reported
    /// to the person.
    fn own_wanted(&self, remote: &[FileVersion]) -> Result<Vec<Action>> {
        let wanted: std::collections::HashSet<String> =
            self.store().db().wanted_paths()?.into_iter().collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for version in remote.iter().filter(|v| v.area == Area::Held && wanted.contains(&v.path)) {
            let Some(hash) = version.content.hash() else { continue };
            let Some(mine) = self.store().db().own_vault_row(&version.path)? else { continue };
            if mine.content_hash == blake3::Hash::from(*hash) {
                out.push(Action::Adopt { remote: version.clone() });
            }
        }
        Ok(out)
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
        self.apply_plan_reporting(actions, source, &mut NoProgress)
    }

    /// [`apply_plan`](Self::apply_plan), telling `progress` about content as it
    /// arrives.
    pub fn apply_plan_reporting(
        &mut self,
        actions: &[Action],
        source: &mut dyn ContentSource,
        progress: &mut dyn Progress,
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
            Action::Adopt { remote } | Action::Hold { remote, .. } if remote.is_deleted() => 1,
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
                    self.take(remote, source, &mut stats, progress)
                }
                Action::Resurrect { resolved } => {
                    stats.resurrected += 1;
                    self.take(resolved, source, &mut stats, progress)
                }
                // No progress reported, and nothing recorded in history: the
                // files are the owner's, and their names have no business on
                // this device's screen.
                Action::Hold { owner, remote } => {
                    stats.held += 1;
                    self.take_hold(owner, remote, source, &mut stats)
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
                    self.take(renamed, source, &mut stats, progress)
                        .and_then(|()| self.take(keeps_path, source, &mut stats, progress))
                }
            };

            if let Err(e) = outcome {
                let path = self.root().join(action.path());
                tracing::warn!(path = %path.display(), error = %e, "plan step failed, continuing");
                // Said once. A failure that will happen again next sync -- a
                // name taken by a file sent here, a file no device it meets
                // has -- was recorded on every one, and a phone's Home showed
                // nothing else (2026-10-07). Still a failure of this sync.
                let detail = e.to_string();
                if !self.store().db().failed_last_with(action.path(), &detail).unwrap_or(false) {
                    crate::note(
                        self.store(),
                        db::Event::Failed,
                        Some(action.path()),
                        None,
                        None,
                        Some(&detail),
                    );
                }
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
        progress: &mut dyn Progress,
    ) -> Result<()> {
        // Checked before anything else, because everything after this joins
        // the path onto a directory and writes or deletes there. The wire
        // refuses these already; this is the same rule for any other route in,
        // and it also keeps a peer out of qurb's own store inside the folder.
        if !qurb_sync::is_safe_path(&version.path)
            || self.ignore.is_ignored(&self.root().join(&version.path))
        {
            return Err(Error::UnsafePath { path: version.path.clone() });
        }

        // A delivery, on a device that files them outside the folder.
        if version.area == Area::Sent && !version.is_deleted() {
            if let Some(dir) = self.downloads().map(Path::to_path_buf) {
                return self.take_into_downloads(version, &dir, source, stats, progress);
            }
        }

        // Content sent to this device's vault is filed under a name that is
        // free here. The sender named it the way *they* think of it, so a
        // clash with something the recipient already has is ordinary rather
        // than exceptional -- two people can both have a `report.pdf` -- and
        // neither file may be overwritten.
        let version = &if version.area == Area::Sent && !version.is_deleted() {
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

        // Whether the file at this path is one somebody sent here, rather than
        // the shared file of the same name. Shared paths and received ones sit
        // in one folder, so a name can mean either -- and a version arriving
        // for the shared area is never about the private one.
        let private_here = version.area == Area::Shared
            && matches!(self.store().db().folder_row(&version.path)?, Some((_, Some(_))));

        match &version.content {
            Content::Deleted => {
                // A replica records the tombstone but has no file to remove.
                // Nor is there one to remove when the file at this path is the
                // private namesake: another device deleting its `notes.txt` is
                // not a reason to delete the `notes.txt` somebody sent here.
                let removes = !self.role().is_replica() && !private_here;
                if removes {
                    // Into Recently deleted rather than unlinked. The file in
                    // the folder is the only copy of its bytes here, and the
                    // deletion is somebody else's: if it was a mistake, this
                    // is where it gets put right (decision 0042).
                    let was = self.store().db().folder_row(&version.path)?;
                    let content = was.as_ref().map(|(row, _)| row.content_hash);
                    let trashed = self.store_mut().move_to_trash(
                        &path,
                        &qurb_storage::db::NewTrash {
                            path: &version.path,
                            scope: None,
                            content: &content.unwrap_or(blake3::Hash::from([0; 32])),
                            size: 0,
                            by: Some(&version.modified_by),
                            why: None,
                        },
                    );
                    if let Err(e) = trashed {
                        // Kept is better than lost, and deleted is what was
                        // asked: fall back to removing it rather than refusing
                        // the deletion and diverging from every other device.
                        tracing::warn!(path = %path.display(), error = %e, "could not keep a deleted file; removing it");
                        match std::fs::remove_file(&path) {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(Error::Io { path: path.clone(), source: e }),
                        }
                    }
                }
                self.store_mut().adopt(version, None, 0)?;
                if removes {
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
                    if version.area == Area::Sent {
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

                // Refuse a shared file whose name a received one already has.
                // Writing it would overwrite the private file, whose bytes live
                // nowhere else here -- the folder *is* its payload store. The
                // shared file is not lost by waiting: it arrives once the
                // received one is renamed or deleted.
                if private_here {
                    return Err(Error::TakenPrivately { path: version.path.clone() });
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
                            progress.started(&version.path, *size);
                            let fetched = source.fetch(hash, *size);
                            if let Ok(bytes) = &fetched {
                                progress.advanced(bytes.len() as u64);
                            }
                            progress.finished(&version.path);
                            fetched?
                        }
                    };
                    self.store_mut().adopt(version, Some(&bytes), version.modified_at)?;
                    source.received(hash);
                    return Ok(());
                }

                // Asked before the write, because writing it is what makes it
                // stop being true.
                let was_evicted = self.store().is_materialised(&version.path)? == Some(false);

                // A folder this device keeps only remotely (decision 0045):
                // the new version is known and listed, not downloaded because
                // another device changed it. Taken as usual when the file is
                // here already, or somebody asked for it.
                if version.area == Area::Shared
                    && self.store().is_materialised(&version.path)? != Some(true)
                    && self.store().db().kept_remotely(&version.path)?
                    && !self.store().db().is_wanted(&version.path)?
                {
                    self.store_mut().know_elsewhere(version)?;
                    return Ok(());
                }

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
                //
                // Left where it is if the fetch is cut off, so the next attempt
                // carries on from it; the scan ignores `.incoming` files.
                let staging = staging_path(&path);
                // Asked by hash rather than by live path: a rename arrives as
                // additions and deletions applied in path order, so the old
                // path may already be tombstoned by the time the new one is
                // written. Its chunks are still on disk.
                match self.bring_in(hash, *size, &staging, &version.path, source, stats, progress) {
                    Ok(()) => {}
                    // The other device does not hold these bytes: it freed
                    // them, or keeps them for somebody else, or they are gone.
                    // A shared file not here is then recorded as it is --
                    // listed, and elsewhere, the state freeing leaves. Before
                    // 2026-10-07 it was asked for again on every sync and
                    // failed every time: a phone showed *Didn't finish* for 18
                    // files lost with another phone's data. A file that is
                    // here keeps its version and the failure, since a stub
                    // would replace bytes; so does one somebody asked for, who
                    // should hear that it did not come.
                    Err(Error::ContentUnavailable { .. })
                        if version.area == Area::Shared
                            && self.store().is_materialised(&version.path)? != Some(true)
                            && !self.store().db().is_wanted(&version.path)? =>
                    {
                        let _ = std::fs::remove_file(&staging);
                        self.store_mut().know_elsewhere(version)?;
                        self.store().db().forget_failures(&version.path)?;
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }

                std::fs::rename(&staging, &path).map_err(|e| {
                    let _ = std::fs::remove_file(&staging);
                    Error::Io { path: path.clone(), source: e }
                })?;

                // Record the modification time the file actually ended up with,
                // so the engine's size-and-mtime fast path recognises it and
                // does not immediately re-read what it just wrote.
                let mtime = std::fs::metadata(&path).ok().map(|m| mtime_ns(&m)).unwrap_or(0);
                // Into this device's own vault: something sent here, or its
                // own file fetched back from a device holding it.
                if matches!(version.area, Area::Sent | Area::Held) {
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
                // Who from: the author of the version -- except for this
                // device's own file fetched back, whose author is this device.
                // History naming it as the sender reads as a stranger.
                let from = (version.area != Area::Held).then_some(&version.modified_by);
                crate::note(
                    self.store(),
                    kind,
                    Some(&version.path),
                    Some(*size),
                    from,
                    (version.area == Area::Sent).then_some("sent to this device"),
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

impl Engine {
    /// Keep `owner`'s file for it, or drop it on the owner's tombstone.
    ///
    /// The bytes are staged inside the store, never the folder: nothing a
    /// person using this device looks at ever shows another device's files.
    fn take_hold(
        &mut self,
        owner: &DeviceId,
        version: &FileVersion,
        source: &mut dyn ContentSource,
        stats: &mut PlanStats,
    ) -> Result<()> {
        if !qurb_sync::is_safe_path(&version.path) {
            return Err(Error::UnsafePath { path: version.path.clone() });
        }
        let Content::File { hash, size } = &version.content else {
            self.store_mut().unhold(version, owner)?;
            return Ok(());
        };
        let content = blake3::Hash::from(*hash);

        // Named by the content, so an attempt cut off part-way is picked up by
        // the next one rather than started again.
        let staging = self.store().root().join(format!("holding-{}.incoming", content.to_hex()));
        // Already here -- a send the owner collected, or the same bytes under
        // another name -- costs a copy, not a transfer.
        self.bring_in(hash, *size, &staging, &version.path, source, stats, &mut NoProgress)?;
        let held = self.store_mut().hold_file(version, owner, &staging).map_err(Error::from);
        let _ = std::fs::remove_file(&staging);
        held?;

        // Told once it is stored, as a delivery is: the owner records this
        // device as holding it, and may free its own copy on the strength of
        // that.
        source.received(hash);
        Ok(())
    }

    /// Take a delivery as an ordinary file in `dir`, outside the folder.
    ///
    /// Nothing about it enters the index as content: it is not scanned, not
    /// counted against the storage limit, and deleting it is the person tidying
    /// their Downloads, not a qurb event. What is kept is the record that it
    /// was taken, so the sender offering it again changes nothing. See
    /// [decision 0037](../../docs/decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md).
    fn take_into_downloads(
        &mut self,
        version: &FileVersion,
        dir: &Path,
        source: &mut dyn ContentSource,
        stats: &mut PlanStats,
        progress: &mut dyn Progress,
    ) -> Result<()> {
        let Content::File { hash, size } = &version.content else { return Ok(()) };
        let content = blake3::Hash::from(*hash);

        // Named as the sender named it, and never over anything already there:
        // this is somebody's Downloads folder, and what is in it is theirs. A
        // name that is taken gets the same treatment as in the folder.
        //
        // A file already there with exactly these bytes is this delivery, put
        // in place by an earlier attempt that stopped before recording it --
        // a crash between the rename and the record. Recorded now rather than
        // written a second time under another name.
        let mut target = None;
        for candidate in [dir.join(&version.path), dir.join(qurb_sync::received_path(version))] {
            if !candidate.exists() {
                target = Some(candidate);
                break;
            }
            if hash_of(&candidate).is_some_and(|existing| existing == content) {
                return self.taken_into_downloads(version, &candidate, source);
            }
        }
        let Some(path) = target else {
            return Err(Error::Io {
                path: dir.join(&version.path),
                source: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "both this name and the name it would be filed under are taken",
                ),
            });
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Io { path: parent.to_path_buf(), source: e })?;
        }

        // Beside the destination and moved into place, as in the folder, so
        // that nobody opens a half-written file. And on disk before the rename:
        // the sender is told it can stop holding this the moment the rename is
        // done, so it has to survive the power going off straight afterwards.
        // Left in place if the fetch is cut off, for the next attempt to carry
        // on from.
        let staging = staging_path(&path);
        self.bring_in(hash, *size, &staging, &version.path, source, stats, progress)?;
        std::fs::File::open(&staging)
            .and_then(|file| file.sync_all())
            .map_err(|e| Error::Io { path: staging.clone(), source: e })?;
        std::fs::rename(&staging, &path).map_err(|e| {
            let _ = std::fs::remove_file(&staging);
            Error::Io { path: path.clone(), source: e }
        })?;

        self.taken_into_downloads(version, &path, source)
    }

    /// Record a delivery that is now in place in Downloads, and say so.
    fn taken_into_downloads(
        &mut self,
        version: &FileVersion,
        path: &Path,
        source: &mut dyn ContentSource,
    ) -> Result<()> {
        let Content::File { hash, size } = &version.content else { return Ok(()) };
        let where_ = path.display().to_string();
        self.store().note_taken(&blake3::Hash::from(*hash), Some(&version.modified_by), &where_)?;
        crate::note(
            self.store(),
            db::Event::Received,
            Some(&version.path),
            Some(*size),
            Some(&version.modified_by),
            // The desktop's notifier recognises a delivery by this wording and
            // reads the path after "saved to"; see `qurb-desktop`'s `notify`.
            Some(&format!("sent to this device; saved to {where_}")),
        );
        source.received(hash);
        Ok(())
    }
}

/// The content hash of a file on disk, or `None` if it cannot be read.
fn hash_of(path: &Path) -> Option<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path).ok()?;
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(hasher.finalize())
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
impl Engine {
    /// Put `hash`'s content in the file at `staging`: from what this device
    /// already holds if it can, or else from `source` -- which carries on from
    /// whatever an earlier attempt that was cut off left in the file.
    ///
    /// Whether the content is here is asked before the file is touched:
    /// opening it to find out would empty it, and with it the part of a
    /// large file that already crossed the network.
    #[allow(clippy::too_many_arguments)]
    fn bring_in(
        &mut self,
        hash: &[u8; 32],
        size: u64,
        staging: &Path,
        label: &str,
        source: &mut dyn ContentSource,
        stats: &mut PlanStats,
        progress: &mut dyn Progress,
    ) -> Result<()> {
        let content = blake3::Hash::from(*hash);
        if self.store().can_read_content(&content)? {
            let mut file = std::fs::File::create(staging)
                .map_err(|e| Error::Io { path: staging.to_path_buf(), source: e })?;
            self.store().read_content_into(&content, &mut file)?;
            return Ok(());
        }
        stats.fetched += 1;
        progress.started(label, size);
        let fetched = source.resume_into(hash, size, staging, &mut *progress);
        progress.finished(label);
        fetched.map(|_| ())
    }
}

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

