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
pub use peer::{ContentSource, NoContent, PlanStats, StoreSource};
pub use repair::RepairStats;
pub use role::{PinSet, Role};

use qurb_storage::Store;
use qurb_watcher::{Change, ChangeKind, Event, IgnoreRules, Watcher};
use std::path::{Path, PathBuf};

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
}

impl Engine {
    /// `root` is the directory being synced. `store` may live inside it, as
    /// long as `ignore` excludes it — see [`IgnoreRules::with_store_dir`].
    pub fn new(root: impl Into<PathBuf>, store: Store, ignore: IgnoreRules) -> Self {
        let root = root.into();
        // Probed rather than assumed from the platform: macOS can be formatted
        // either way, and a network mount can be anything regardless of host.
        let fold_case = qurb_watcher::is_case_insensitive(&root);
        Self { root, store, ignore, fold_case, role: Role::Syncing, workers: default_workers() }
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

        let entries = qurb_watcher::scan(&self.root, &self.ignore)?;
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
        for logical in self.store.db().live_paths()? {
            if on_disk.binary_search(&logical).is_err() {
                match self.store.delete_file(&logical) {
                    Ok(()) => stats.deleted += 1,
                    Err(e) => stats.record(Path::new(&logical), e.into()),
                }
            }
        }

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
        Ok(match self.store.db().file_by_path(logical)? {
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
        let collected = std::sync::Mutex::new(SyncStats::default());

        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    // A connection per worker. WAL mode allows it, and the
                    // collector already runs against a live writer, so
                    // concurrent access is a path with tests behind it.
                    let mut store = match Store::open(&root, key.clone()) {
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

        if let Some(existing) = self.store.db().file_by_path(logical)? {
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

        for path in self.store.db().live_paths_under(logical)? {
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
