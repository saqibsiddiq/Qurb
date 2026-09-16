//! Native filesystem watching, wired to the debouncer.
//!
//! Uses `inotify` on Linux, `FSEvents` on macOS, and `ReadDirectoryChangesW` on
//! Windows via the `notify` crate, so there is no polling.

use crate::debounce::{Change, ChangeKind, DebounceConfig, Debouncer};
use crate::error::{Error, Result};
use crate::ignore::IgnoreRules;
use crate::scan;
use notify::{EventKind, RecursiveMode, Watcher as _};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::mpsc;

/// What the watcher reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Paths that have settled. Absolute; use [`scan::logical_path`] to convert.
    ///
    /// A `Removed` path may name a file or a directory that no longer exists,
    /// and there is no way to tell which after the fact. The caller must treat
    /// removal as covering everything it knows about beneath that path — the
    /// index is what remembers which files those were.
    Changes(Vec<Change>),

    /// Events were dropped and the stream is no longer complete. The caller
    /// must run a full [`scan`](crate::scan::scan) and reconcile.
    RescanRequired,
}

/// Watches a directory tree and reports settled changes.
///
/// # Delivery guarantee
///
/// **At least once, not exactly once.** A path can be reported more than once
/// for the same underlying change — most often when a file surfaces both from
/// walking a newly created directory and from its own event arriving
/// afterwards, but also because platforms themselves duplicate events.
///
/// Exactly-once is not achievable here and not worth chasing. Deciding that a
/// reported file has not really changed requires knowing what was last stored
/// for it, and the index already knows that. A second cache in the watcher
/// would duplicate that knowledge and could disagree with it.
///
/// So the consumer must be idempotent. Storing a file whose content is
/// unchanged is already a no-op in the storage layer, which makes a duplicate
/// cost a re-read rather than a correctness problem.
pub struct Watcher {
    /// Dropping this stops the platform watcher, so it is kept alive here even
    /// though nothing calls it.
    _inner: notify::RecommendedWatcher,
    events: mpsc::UnboundedReceiver<Event>,
}

impl Watcher {
    /// Begin watching `root` recursively.
    pub fn start(root: &Path, ignore: IgnoreRules, config: DebounceConfig) -> Result<Self> {
        let (raw_tx, raw_rx) = mpsc::unbounded_channel::<RawEvent>();
        let (out_tx, out_rx) = mpsc::unbounded_channel::<Event>();

        let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            // Runs on the platform watcher's own thread. Send and return: any
            // work done here delays the next event and makes an overflow more
            // likely.
            let msg = match res {
                Ok(event) if event.need_rescan() => RawEvent::Overflow,
                Ok(event) => RawEvent::Notify(event),
                // An error from the backend means events may have been lost,
                // and a rescan is the only safe interpretation.
                Err(e) => {
                    tracing::warn!(error = %e, "filesystem watch error, forcing rescan");
                    RawEvent::Overflow
                }
            };
            let _ = raw_tx.send(msg);
        })
        .map_err(|source| Error::Start { path: root.to_path_buf(), source })?;

        inner
            .watch(root, RecursiveMode::Recursive)
            .map_err(|source| Error::Start { path: root.to_path_buf(), source })?;

        tokio::spawn(run(raw_rx, out_tx, ignore, config));

        Ok(Self { _inner: inner, events: out_rx })
    }

    /// Wait for the next batch. Returns `None` once the watcher has stopped.
    pub async fn next(&mut self) -> Option<Event> {
        self.events.recv().await
    }
}

enum RawEvent {
    Notify(notify::Event),
    Overflow,
}

/// Size and modification time, used to tell "finished writing" from "paused".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Snapshot {
    size: u64,
    mtime_ns: i64,
}

fn snapshot(path: &Path) -> Option<Snapshot> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    Some(Snapshot { size: meta.len(), mtime_ns: scan::mtime_ns(&meta) })
}

async fn run(
    mut raw: mpsc::UnboundedReceiver<RawEvent>,
    out: mpsc::UnboundedSender<Event>,
    ignore: IgnoreRules,
    config: DebounceConfig,
) {
    let mut debouncer = Debouncer::new(config);
    let mut snapshots: HashMap<PathBuf, Snapshot> = HashMap::new();

    loop {
        // Sleep until either something arrives or the soonest pending path
        // becomes eligible. Without the deadline the last change in a quiet
        // period would sit unreported until unrelated activity woke us.
        let deadline = debouncer.next_deadline();
        let got_event = match deadline {
            Some(at) => tokio::select! {
                event = raw.recv() => match event {
                    Some(e) => { handle(e, &mut debouncer, &ignore, &mut snapshots); true }
                    None => return,
                },
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(at)) => false,
            },
            None => match raw.recv().await {
                Some(e) => {
                    handle(e, &mut debouncer, &ignore, &mut snapshots);
                    true
                }
                None => return,
            },
        };

        // Drain everything queued rather than looping once per event: a burst
        // arrives faster than the select above can cycle.
        if got_event {
            while let Ok(event) = raw.try_recv() {
                handle(event, &mut debouncer, &ignore, &mut snapshots);
            }
        }

        if debouncer.take_rescan() && out.send(Event::RescanRequired).is_err() {
            return;
        }

        let now = Instant::now();
        let mut settled = Vec::new();
        for change in debouncer.drain_settled(now) {
            match change.kind {
                ChangeKind::Removed => {
                    snapshots.remove(&change.path);
                    settled.push(change);
                }
                ChangeKind::Upserted => match std::fs::metadata(&change.path) {
                    // Gone between the event and now.
                    Err(_) => {
                        snapshots.remove(&change.path);
                        settled.push(Change::removed(change.path));
                    }

                    // A directory. Two things are true and both need handling.
                    //
                    // It is not a file, so it must not be reported as one --
                    // and certainly not as a removal, which is what treating
                    // "cannot stat as a file" as deletion would do.
                    //
                    // More importantly, recursive watching has a race: the
                    // platform watch for a new subdirectory is installed after
                    // the directory exists, so anything written into it in
                    // that window produces no event at all. Unpacking an
                    // archive or cloning a repository into the synced folder
                    // hits this constantly. Walking the directory here closes
                    // the gap.
                    //
                    // Discovered files are fed back through the debouncer
                    // rather than emitted directly, so they still get the
                    // quiet period and the stability check, and so a file that
                    // also produced its own event is not reported twice.
                    Ok(meta) if meta.is_dir() => {
                        snapshots.remove(&change.path);
                        match scan::scan(&change.path, &ignore) {
                            Ok(entries) => {
                                for entry in entries {
                                    if let Some(s) = snapshot(&entry.path) {
                                        snapshots.insert(entry.path.clone(), s);
                                    }
                                    debouncer.observe(entry.path, ChangeKind::Upserted, now);
                                }
                            }
                            Err(e) => {
                                // Could not enumerate it, so we cannot know
                                // what is inside. A rescan is the honest
                                // response.
                                tracing::warn!(
                                    path = %change.path.display(), error = %e,
                                    "could not walk new directory, requesting rescan"
                                );
                                debouncer.observe_overflow();
                            }
                        }
                    }

                    // An ordinary file. If it changed since the event that
                    // queued it, a write is still in flight whose event has
                    // not arrived. Reading it now could store a half-written
                    // copy, and the memory-mapped read in the storage layer
                    // raises SIGBUS if the file is truncated underneath it.
                    Ok(meta) => {
                        let current =
                            Snapshot { size: meta.len(), mtime_ns: scan::mtime_ns(&meta) };
                        match snapshots.get(&change.path) {
                            Some(previous) if *previous == current => {
                                snapshots.remove(&change.path);
                                settled.push(change);
                            }
                            _ => {
                                snapshots.insert(change.path.clone(), current);
                                debouncer.defer(&change.path, now);
                            }
                        }
                    }
                },
            }
        }

        if !settled.is_empty() && out.send(Event::Changes(settled)).is_err() {
            return;
        }
    }
}

fn handle(
    event: RawEvent,
    debouncer: &mut Debouncer,
    ignore: &IgnoreRules,
    snapshots: &mut HashMap<PathBuf, Snapshot>,
) {
    let now = Instant::now();
    let event = match event {
        RawEvent::Overflow => {
            debouncer.observe_overflow();
            return;
        }
        RawEvent::Notify(e) => e,
    };

    for (path, kind) in classify(&event) {
        if ignore.is_ignored(&path) {
            continue;
        }
        if kind == ChangeKind::Upserted {
            // Record what the file looks like now, so the stability check at
            // release time has something to compare against.
            if let Some(s) = snapshot(&path) {
                snapshots.insert(path.clone(), s);
            }
        }
        debouncer.observe(path, kind, now);
    }
}

/// Map a platform event onto the changes it implies.
///
/// A rename carries both paths, and produces two changes: the old path is gone
/// and the new one has appeared.
fn classify(event: &notify::Event) -> Vec<(PathBuf, ChangeKind)> {
    use notify::event::{ModifyKind, RenameMode};

    match &event.kind {
        EventKind::Remove(_) => {
            event.paths.iter().map(|p| (p.clone(), ChangeKind::Removed)).collect()
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            event.paths.iter().map(|p| (p.clone(), ChangeKind::Removed)).collect()
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
            let mut out = Vec::new();
            if let Some(from) = event.paths.first() {
                out.push((from.clone(), ChangeKind::Removed));
            }
            if let Some(to) = event.paths.get(1) {
                out.push((to.clone(), ChangeKind::Upserted));
            }
            out
        }
        // Reads change nothing worth syncing.
        EventKind::Access(_) => Vec::new(),
        // Creates, writes, metadata changes, rename-to, and anything the
        // backend could not classify. Treating an unknown event as a possible
        // write costs one stat; ignoring it would lose data.
        _ => event.paths.iter().map(|p| (p.clone(), ChangeKind::Upserted)).collect(),
    }
}
