//! The sync engine: what a filesystem change means, and what to do about it.
//!
//! [`qurb_watcher`] reports that something changed. [`qurb_storage`] can store
//! and remove files. This crate is the part in between — the decisions.
//!
//! ```text
//!   startup ──► reconcile   walk the disk, compare with the index,
//!                           store what is new, tombstone what is gone
//!
//!   running ──► apply       act on settled changes from the watcher
//!
//!   overflow ─► reconcile   the event stream stopped describing reality,
//!                           so fall back to comparing everything
//! ```
//!
//! # Two rules that shape everything here
//!
//! **A file is not re-read unless it looks different.** The watcher delivers
//! changes at least once, and a full reconciliation revisits every file, so the
//! same path arrives repeatedly with nothing having changed. Comparing size and
//! modification time against the index first turns that from a full read into a
//! stat.
//!
//! **One bad file must not stop the others.** A file with no read permission, or
//! one deleted between being reported and being read, is recorded as a failure
//! and the run continues. An engine that aborts a sync because of one
//! unreadable file leaves everything else unsynced for a reason the user cannot
//! see.

pub mod error;
pub mod peer;
pub mod repair;
pub mod role;

pub use error::{Error, FileFailure, Result};
pub use peer::{ContentSource, NoContent, NoProgress, PlanStats, Progress, StoreSource};
pub use repair::RepairStats;
pub use role::{PinSet, Role};

use qurb_storage::db;
use qurb_storage::Store;
use qurb_watcher::{Change, ChangeKind, Event, IgnoreRules, Watcher};
use std::path::{Path, PathBuf};

/// Write down that something happened, and carry on if it cannot be written.
///
/// History is worth having and is never worth failing an operation over: a
/// sync that worked must not be reported as failed because the note about it
/// did not land. Every recording site in this crate goes through here so that
/// the decision is made once.
fn note(
    store: &Store,
    kind: db::Event,
    path: Option<&str>,
    size: Option<u64>,
    device: Option<&qurb_sync::DeviceId>,
    detail: Option<&str>,
) {
    if let Err(e) = store.db().record(kind, path, size, device, detail) {
        tracing::debug!(error = %e, "could not write down what happened");
    }
}

/// What a run of the engine did.
#[derive(Debug, Default)]
pub struct SyncStats {
    /// Files read and stored.
    pub stored: usize,
    /// Files skipped because size and modification time matched the index.
    pub unchanged: usize,
    /// Paths tombstoned.
    pub deleted: usize,
    pub bytes_written: u64,
    /// Files skipped because a differently-spelled file already claimed their
    /// logical path. See [`Engine::reconcile`].
    pub collided: usize,
    /// Per-file failures. The run continued past each of these.
    pub failures: Vec<FileFailure>,
}

impl SyncStats {
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }

    fn record(&mut self, path: &Path, error: Error) {
        tracing::warn!(path = %path.display(), %error, "file failed, continuing");
        self.failures.push(FileFailure { path: path.to_path_buf(), error });
    }

    /// Write the failures down, now that the run is over.
    ///
    /// Separate from [`SyncStats::record`] because that runs deep inside a
    /// borrow of the store and this needs one of its own. Called by whoever
    /// owns both.
    fn write_failures(&self, store: &Store) {
        for failure in &self.failures {
            note(
                store,
                db::Event::Failed,
                Some(&failure.path.to_string_lossy()),
                None,
                None,
                Some(&failure.error.to_string()),
            );
        }
    }

    fn merge(&mut self, other: SyncStats) {
        self.stored += other.stored;
        self.unchanged += other.unchanged;
        self.deleted += other.deleted;
        self.bytes_written += other.bytes_written;
        self.failures.extend(other.failures);
    }
}

pub struct Engine {
    root: PathBuf,
    store: Store,
    ignore: IgnoreRules,
    fold_case: bool,
    role: Role,
    workers: usize,
    /// Where a file sent to this device goes, if not into the folder.
    ///
    /// Set on a desktop, where a delivery is an ordinary file in Downloads that
    /// qurb stops tracking once it is written. `None` files it in the folder,
    /// privately, which is what a phone does. See
    /// [decision 0037](../../docs/decisions/0037-a-file-sent-to-a-desktop-is-an-ordinary-file.md).
    downloads: Option<PathBuf>,
}

impl Engine {
    /// `root` is the directory being synced. `store` may live inside it, as
    /// long as `ignore` excludes it — see [`IgnoreRules::with_store_dir`].
    pub fn new(root: impl Into<PathBuf>, store: Store, ignore: IgnoreRules) -> Self {
        let root = root.into();
        // Probed rather than assumed from the platform: macOS can be formatted
        // either way, and a network mount can be anything regardless of host.
        let fold_case = qurb_watcher::is_case_insensitive(&root);

        // Attach the folder to the store here, so that no caller has to
        // remember to. A syncing device's files *are* its payload store, and a
        // store that does not know its folder keeps a second encrypted copy of
        // every file — correct, and twice the size it should be. Doing it at
        // the one place that knows both is the difference between a rule and a
        // convention.
        let store = store.in_tree(&root);

        Self {
            root,
            store,
            ignore,
            fold_case,
            role: Role::Syncing,
            workers: default_workers(),
            downloads: None,
        }
    }

    /// A device that holds content without a directory behind it.
    ///
    /// `root` is only where the store lives; nothing is ever written under it.
    /// See [`Role::Replica`] for the two behaviours this switches off and why
    /// leaving either on would destroy data rather than merely waste space.
    pub fn replica(root: impl Into<PathBuf>, store: Store, pins: PinSet) -> Self {
        Self {
            root: root.into(),
            store,
            ignore: IgnoreRules::new(),
            fold_case: false,
            role: Role::Replica(pins),
            workers: default_workers(),
            downloads: None,
        }
    }

    /// How many files to store at once during a bulk pass.
    ///
    /// One disables threading entirely, which is what the tests want when they
    /// are measuring something else.
    pub fn set_workers(&mut self, workers: usize) {
        self.workers = workers.max(1);
    }

    pub fn role(&self) -> &Role {
        &self.role
    }

    /// File deliveries into `dir` as ordinary files, rather than into the
    /// folder. The caller has checked that `dir` does not overlap the folder:
    /// a delivery written inside it would be scanned into the shared area and
    /// advertised to every device.
    pub fn set_downloads(&mut self, dir: Option<PathBuf>) {
        self.downloads = dir;
    }

    pub fn downloads(&self) -> Option<&Path> {
        self.downloads.as_deref()
    }

    /// Whether two paths differing only in case are treated as a collision.
    ///
    /// Defaults to what this filesystem actually does. Worth overriding to
    /// `true` on a case-sensitive machine that syncs with a case-insensitive
    /// one: the hazard belongs to the *fleet*, not to the local disk. A Linux
    /// desktop holding both `README` and `readme` will destroy one of them the
    /// moment an iPhone joins, and the resulting deletion propagates back.
    pub fn set_fold_case(&mut self, fold: bool) {
        self.fold_case = fold;
    }

    pub fn folds_case(&self) -> bool {
        self.fold_case
    }

    /// Live paths this device holds that a case-insensitive filesystem could
    /// not keep apart, grouped.
    ///
    /// Always worth reporting even where it is currently harmless, because the
    /// harm arrives with the next device rather than with the next write.
    pub fn case_collisions(&self) -> Result<Vec<Vec<String>>> {
        Ok(qurb_watcher::case_collisions(&self.store.db().live_paths()?))
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// The directory being synced.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Compare the whole tree against the index and make them agree.
    ///
    /// Run at startup, because anything that changed while the process was not
    /// running produced no event; and after the watcher reports dropped events,
    /// because the stream is no longer a complete description of what happened.
    ///
    /// Deliberately blunt. It re-examines every file rather than trying to work
    /// out what it missed, which is what makes it a safe recovery path: it
    /// needs no record of where the gap began.
    pub fn reconcile(&mut self) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        // A replica has no directory to compare against. Walking one anyway
        // would find nothing and conclude that every file it holds had been
        // deleted -- then propagate those tombstones to every device that
        // trusts it. Doing nothing is not a shortcut here; it is the whole
        // point of the role.
        if self.role.is_replica() {
            tracing::debug!("replica: nothing to reconcile against");
            return Ok(stats);
        }

        let mut entries = qurb_watcher::scan(&self.root, &self.ignore)?;

        // Filenames that differ on disk but mean the same path once normalised
        // -- `café` spelled two ways, which Linux keeps apart and macOS does
        // not. The index is keyed by the normalised path and can hold only one,
        // so indexing both would silently drop whichever came second and leave
        // the two devices disagreeing about that path's contents.
        //
        // The already-normalised spelling wins, and byte order breaks any
        // remaining tie -- see `qurb_watcher::scan`. It has to be a rule that
        // depends only on the filenames, because two devices resolving this
        // separately must reach the same answer. The rest are skipped and
        // counted; renaming a user's file is not a decision to make for them.
        let collisions = qurb_watcher::normalization_collisions(&entries);
        if !collisions.is_empty() {
            let mut skip: Vec<PathBuf> = Vec::new();
            for group in &collisions {
                let kept = &group[0];
                for other in &group[1..] {
                    tracing::warn!(
                        logical = %kept.logical,
                        kept = %kept.path.display(),
                        skipped = %other.path.display(),
                        "two filenames normalise to one path; syncing only the first"
                    );
                    skip.push(other.path.clone());
                }
            }
            stats.collided = skip.len();
            entries.retain(|e| !skip.contains(&e.path));
        }

        let mut on_disk = Vec::with_capacity(entries.len());

        // Two passes, because they cost completely different things.
        //
        // Deciding whether a file changed is a stat and an index lookup: cheap,
        // and 100k of them take about a second. Storing one that did change is a
        // read, a chunking pass, compression, encryption and an fsync -- mostly
        // waiting on the disk rather than working.
        //
        // So the decision is made here, in order, and the storing is handed to
        // workers. Overlapping the waiting is the whole gain; there is very
        // little computation to parallelise.
        let mut changed = Vec::new();
        for entry in entries {
            on_disk.push(entry.logical.clone());
            match self.looks_unchanged(&entry.logical, entry.size, entry.mtime_ns) {
                Ok(true) => stats.unchanged += 1,
                Ok(false) => changed.push(entry),
                Err(e) => stats.record(&entry.path, e),
            }
        }

        if !changed.is_empty() {
            stats.merge(self.store_many(changed));
        }

        // Anything the index still calls live but the walk did not find is
        // gone. Sorting lets a binary search replace a linear scan per path,
        // which matters on a library with many files.
        on_disk.sort();
        // Everything the folder is supposed to hold, which is the shared area
        // *and* anything sent to this device: both are written into it, and a
        // sweep that looked only at the first would never notice a received
        // file being deleted.
        for logical in self.store.db().folder_paths()? {
            if on_disk.binary_search(&logical).is_ok() {
                continue;
            }

            // Absent because *we* dropped it to stay under the storage cap,
            // not because anyone deleted it. Tombstoning here would turn this
            // device being short of space into a deletion on every other
            // device — the single worst thing the cap could do.
            if self.store.is_materialised(&logical)? == Some(false) {
                continue;
            }

            match self.store.delete_file(&logical) {
                Ok(()) => stats.deleted += 1,
                Err(e) => stats.record(Path::new(&logical), e.into()),
            }
        }

        stats.write_failures(&self.store);
        Ok(stats)
    }

    /// Bring disk use under `limit` by dropping local copies, coldest first.
    ///
    /// Returns what it managed to free and what it could not. Falling short is
    /// a normal outcome, not an error: a device whose content exists nowhere
    /// else has nothing it may safely drop, and the honest answer is to stay
    /// over the limit and say so. Deleting the user's only copy to satisfy a
    /// number they typed into a settings box would be the wrong trade in every
    /// case.
    ///
    /// A limit of zero means no limit.
    /// Free what nothing needs: garbage past `retention`, and chunk-store
    /// copies of bytes the folder already holds.
    ///
    /// The routine every device runs -- the desktop daemon every few minutes,
    /// a phone after each background sync. Before this was shared, only the
    /// daemon collected garbage and nothing ran `reclaim` unless a person typed
    /// it, so a phone kept the chunks of every file it had ever replaced or
    /// deleted: measured on a Galaxy S23, 100.7 MB on disk for 30.9 MB of
    /// files.
    ///
    /// Content held for another device is not released here. That is kept on
    /// purpose until the disk is short (decision 0030), which is
    /// [`enforce_limit`](Self::enforce_limit)'s business.
    pub fn housekeep(&mut self, retention: std::time::Duration) -> Result<Housekeeping> {
        // Sends first: one called off, or collected, lets go of references
        // that collection below can then free (decision 0060).
        let called_off = self.store.check_sends()?;
        let sends_done = self.store.tidy_sends()?;
        // And copies of this device's own files a computer of another person
        // now keeps: the guest chose to keep nothing on the phone (decision
        // 0060).
        self.store.free_kept_by_hosts()?;
        let collected = self.store.gc(retention)?;
        let reclaimed = self.store.reclaim()?;
        let expired = self.store.empty_trash(qurb_storage::TRASH_RETENTION)?;
        Ok(Housekeeping { collected, reclaimed, expired, called_off, sends_done })
    }

    pub fn enforce_limit(&mut self, limit: u64) -> Result<CapStats> {
        let mut stats = CapStats::default();
        if limit == 0 {
            return Ok(stats);
        }

        let usage = self.store.usage()?;
        stats.used_before = usage.total();
        stats.used_after = stats.used_before;
        if stats.used_before <= limit {
            return Ok(stats);
        }

        let mut over = stats.used_before - limit;

        // Somebody else's copy goes before any of this device's own files. A
        // vault entry the recipient already has is pure courtesy storage; the
        // user's own work is not.
        match self.store.release_held_payloads() {
            Ok(released) if released.bytes_reclaimed > 0 => {
                tracing::info!(
                    chunks = released.chunks_removed,
                    freed = released.bytes_reclaimed,
                    "released content held for other devices to stay under the limit"
                );
                stats.released += released.chunks_removed;
                stats.freed += released.bytes_reclaimed;
                over = over.saturating_sub(released.bytes_reclaimed);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "releasing held content failed"),
        }

        // Then Recently deleted, oldest first: files somebody already chose to
        // delete go before local copies of files nobody has.
        if over > 0 {
            match self.store.empty_trash_by(over) {
                Ok(emptied) if emptied > 0 => {
                    tracing::info!(freed = emptied, "emptied recently deleted files to stay under the limit");
                    stats.emptied += emptied;
                    stats.freed += emptied;
                    over = over.saturating_sub(emptied);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "emptying recently deleted files failed"),
            }
        }

        for (path, size, _) in self.store.evictable()? {
            if over == 0 {
                break;
            }
            match self.store.evict(&path) {
                Ok(freed) => {
                    tracing::info!(path = %path, freed, "dropped a local copy to stay under the limit");
                    stats.dropped += 1;
                    stats.freed += freed;
                    over = over.saturating_sub(freed.max(size));
                }
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "could not drop this one");
                    stats.refused += 1;
                }
            }
        }

        stats.used_after = self.store.usage()?.total();
        stats.still_over = stats.used_after.saturating_sub(limit);
        Ok(stats)
    }

    /// Act on settled changes from the watcher.
    pub fn apply(&mut self, changes: &[Change]) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        for change in changes {
            let Some(logical) = qurb_watcher::logical_path(&self.root, &change.path) else {
                // Outside the watched tree, or a path we cannot represent.
                continue;
            };

            let result = match change.kind {
                ChangeKind::Upserted => self.apply_upsert(&change.path, &logical),
                ChangeKind::Removed => self.apply_removal(&logical),
            };

            match result {
                Ok(outcome) => stats.merge(outcome),
                Err(e) => stats.record(&change.path, e),
            }
        }

        stats.write_failures(&self.store);
        Ok(stats)
    }

    fn apply_upsert(&mut self, path: &Path, logical: &str) -> Result<SyncStats> {
        let meta = match std::fs::metadata(path) {
            Ok(m) if m.is_file() => m,
            // Gone, or turned into a directory, between being reported and
            // being read. The watcher delivers at least once and cannot
            // promise the file still exists.
            _ => return self.apply_removal(logical),
        };
        self.store_if_changed(path, logical, meta.len(), mtime_ns(&meta))
    }

    /// Store a file unless the index already agrees with what is on disk.
    ///
    /// # The heuristic, stated plainly
    ///
    /// Matching size and modification time is taken as proof the content is
    /// unchanged. That is not strictly true: a file edited in place, keeping
    /// its length, within the same timestamp tick would slip through. The
    /// window is small and the alternative is reading every file on every
    /// reconciliation, which on a large library is the difference between a
    /// startup that takes a second and one that takes minutes.
    ///
    /// rsync and git make the same trade. The backstop is
    /// [`Store::verify`](qurb_storage::Store::verify), which re-reads
    /// everything and is meant to run occasionally rather than on every change.
    /// Whether the index already agrees with what is on disk.
    ///
    /// See `store_if_changed` for what this trades away.
    fn looks_unchanged(&self, logical: &str, size: u64, mtime_ns: i64) -> Result<bool> {
        // Either kind of file the folder holds. A received file that looked
        // unknown here would be read and stored on every scan.
        Ok(match self.store.db().in_folder(logical)? {
            Some(existing) => {
                existing.deleted_at.is_none()
                    && existing.size == size
                    && existing.mtime_ns == mtime_ns
            }
            None => false,
        })
    }

    /// Store many files at once, across several threads.
    ///
    /// Each worker opens its own connection to the same store. SQLite permits
    /// one writer at a time, so the index updates still happen one after
    /// another — but they are short, and everything around them is not. The
    /// reading, chunking, compressing, encrypting and fsyncing overlap, which is
    /// where the time goes.
    ///
    /// Failures are collected rather than raised: one unreadable file must not
    /// take the rest of the run with it.
    fn store_many(&mut self, entries: Vec<qurb_watcher::ScanEntry>) -> SyncStats {
        let workers = self.workers.min(entries.len()).max(1);
        if workers == 1 {
            // Not worth a thread, and this keeps small runs on the simple path.
            let mut stats = SyncStats::default();
            for entry in entries {
                match self.store.put_file(&entry.logical, &entry.path) {
                    Ok(put) => record_put(&mut stats, put),
                    Err(e) => stats.record(&entry.path, e.into()),
                }
            }
            return stats;
        }

        let queue = std::sync::Mutex::new(entries.into_iter());
        let root = self.store.root().to_path_buf();
        let key = self.store.chunk_key();
        // Each worker must agree with the main store about whether the tree
        // supplies payloads. A worker without it would write a second copy of
        // every file it stores, silently undoing the saving for exactly the
        // bulk pass where it matters most.
        let tree = self.store.has_tree().then(|| self.root.clone());
        let collected = std::sync::Mutex::new(SyncStats::default());

        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    // A connection per worker. WAL mode allows it, and the
                    // collector already runs against a live writer, so
                    // concurrent access is a path with tests behind it.
                    let mut store = match Store::open(&root, key.clone())
                        .map(|s| match &tree {
                            Some(root) => s.in_tree(root),
                            None => s,
                        }) {
                        Ok(store) => store,
                        Err(e) => {
                            collected
                                .lock()
                                .expect("stats")
                                .record(&root, e.into());
                            return;
                        }
                    };

                    let mut mine = SyncStats::default();
                    loop {
                        let Some(entry) = queue.lock().expect("queue").next() else { break };
                        match store.put_file(&entry.logical, &entry.path) {
                            Ok(put) => record_put(&mut mine, put),
                            Err(e) => mine.record(&entry.path, e.into()),
                        }
                    }
                    collected.lock().expect("stats").merge(mine);
                });
            }
        });

        collected.into_inner().expect("stats")
    }

    fn store_if_changed(
        &mut self,
        path: &Path,
        logical: &str,
        size: u64,
        mtime_ns: i64,
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        if let Some(existing) = self.store.db().in_folder(logical)? {
            if existing.deleted_at.is_none()
                && existing.size == size
                && existing.mtime_ns == mtime_ns
            {
                stats.unchanged += 1;
                return Ok(stats);
            }
        }

        let put = self.store.put_file(logical, path)?;
        if put.unchanged {
            // The content hash matched after all: the file was touched, or
            // rewritten with identical bytes. No chunks moved.
            stats.unchanged += 1;
        } else {
            stats.stored += 1;
            stats.bytes_written += put.bytes_written;
        }
        Ok(stats)
    }

    /// Tombstone a removed path and everything the index holds beneath it.
    ///
    /// The watcher cannot say whether a vanished path was a file or a
    /// directory — it is gone either way, and there is nothing left to
    /// inspect. The index is what remembers, so the question becomes "what did
    /// we know about at or under this path", which it can answer.
    fn apply_removal(&mut self, logical: &str) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        for path in self.store.db().folder_paths_under(logical)? {
            // Evicting a file removes it from the folder, and the watcher
            // reports that like any other removal. The index was marked before
            // the unlink precisely so this check can tell the two apart.
            if self.store.is_materialised(&path)? == Some(false) {
                continue;
            }

            match self.store.delete_file(&path) {
                Ok(()) => stats.deleted += 1,
                // Already gone: another change in the same batch covered it.
                Err(qurb_storage::Error::NotFound { .. }) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(stats)
    }

    /// Reconcile, then follow the watcher until it stops.
    ///
    /// # Runtime requirement
    ///
    /// Storage work is synchronous and can take a long time — chunking and
    /// hashing a large file is seconds of CPU. It runs inside
    /// [`tokio::task::block_in_place`], which needs the multi-threaded
    /// scheduler. On a current-thread runtime this will panic; call
    /// [`Engine::reconcile`] and [`Engine::apply`] directly instead, which is
    /// what the tests do.
    pub async fn run(&mut self, mut watcher: Watcher) -> Result<()> {
        let initial = tokio::task::block_in_place(|| self.reconcile())?;
        tracing::info!(
            stored = initial.stored,
            unchanged = initial.unchanged,
            deleted = initial.deleted,
            failures = initial.failures.len(),
            "initial reconciliation complete"
        );

        while let Some(event) = watcher.next().await {
            let stats = match event {
                Event::Changes(changes) => {
                    tokio::task::block_in_place(|| self.apply(&changes))?
                }
                Event::RescanRequired => {
                    tracing::warn!("watcher dropped events, reconciling from scratch");
                    tokio::task::block_in_place(|| self.reconcile())?
                }
            };

            if !stats.is_clean() {
                tracing::warn!(failures = stats.failures.len(), "some files did not sync");
            }
        }

        Ok(())
    }
}

fn record_put(stats: &mut SyncStats, put: qurb_storage::PutStats) {
    if put.unchanged {
        // The content hash matched after all: the file was touched, or
        // rewritten with identical bytes. No chunks moved.
        stats.unchanged += 1;
    } else {
        stats.stored += 1;
        stats.bytes_written += put.bytes_written;
    }
}

/// Enough threads to keep the disk busy, and not so many that they queue up
/// behind SQLite's single writer.
///
/// Four, from measurement rather than from the core count. On a 12-core machine
/// with NVMe storage, indexing 20,000 files:
///
/// ```text
///   1 worker    487 files/s
///   2 workers   698
///   4 workers   830-888
///   8 workers   864
/// ```
///
/// The gain comes from overlapping waits, not from computation — chunking,
/// hashing and encryption together are under a tenth of the time. So it stops
/// improving once the disk has enough requests in flight, and past that the
/// threads only queue behind the one writer SQLite allows.
///
/// Repeat runs either side of four varied by more than the difference between
/// them, so this is the middle of a flat region rather than a sharp optimum. A
/// spinning disk would want fewer.
fn default_workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 4)
}

fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// How long deleted and superseded content is kept before it is collected.
///
/// A week: long enough to notice a mistake over a weekend, short enough that a
/// device does not carry a month of things nobody wants. Content still
/// referenced by a live file is never touched, whatever its age.
pub const RETENTION: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// What [`Engine::housekeep`] freed.
#[derive(Debug, Default, Clone)]
pub struct Housekeeping {
    /// Garbage past the retention window.
    pub collected: qurb_storage::GcStats,
    /// Chunk-store copies of bytes the folder already holds.
    pub reclaimed: qurb_storage::GcStats,
    /// Files and bytes that had been in Recently deleted past its retention.
    pub expired: (usize, u64),
    /// Sends whose file changed or went before they were collected, and so
    /// were not sent (decision 0060).
    pub called_off: Vec<qurb_storage::CalledOff>,
    /// Sends collected or taken back, whose files qurb stopped reading.
    pub sends_done: usize,
}

impl Housekeeping {
    pub fn bytes_freed(&self) -> u64 {
        self.collected.bytes_reclaimed + self.reclaimed.bytes_reclaimed + self.expired.1
    }
}

/// What enforcing a storage limit achieved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapStats {
    pub used_before: u64,
    pub used_after: u64,
    /// Files whose local copy was dropped.
    pub dropped: usize,
    /// Bytes that left the disk.
    pub freed: u64,
    /// Candidates the store declined to drop, having found them unsafe.
    pub refused: usize,
    /// Chunks dropped that were held only for another device, which has them.
    /// Released before any of this device's own files.
    pub released: usize,
    /// Bytes of Recently deleted emptied early, after held content and before
    /// any live file.
    pub emptied: u64,
    /// Bytes still over the limit after doing everything permitted. Non-zero
    /// means the device holds content nothing else has, and is keeping it.
    pub still_over: u64,
}

impl CapStats {
    /// Whether the device is now within its limit.
    pub fn within(&self) -> bool {
        self.still_over == 0
    }
}
