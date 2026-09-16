//! Turning a stream of raw filesystem events into settled changes.
//!
//! Raw events are far noisier than the changes a user made. Saving one file in
//! a text editor typically produces a create, several writes, a rename, a
//! delete, and a permissions change — for two paths. Copying a large file
//! produces a write event every few milliseconds for as long as the copy runs.
//! Acting on each event would re-chunk the same file dozens of times and
//! transfer half-written data.
//!
//! So events are collected per path and released only once that path has been
//! quiet for a while. Two rules govern the release:
//!
//! - **Quiet period.** A path is released when no event has touched it for
//!   `quiet`. This is what collapses an editor's burst into one change.
//! - **Maximum hold.** A path is released regardless once `max_hold` has passed
//!   since its first event. Without this, a file being appended to continuously
//!   — a log, a long download, a video being recorded — would never settle and
//!   would never sync at all.
//!
//! This type is deliberately free of I/O and of any clock of its own: the
//! caller passes the current time in. Timing logic that reads the real clock is
//! almost impossible to test without sleeping, and tests that sleep are slow
//! and flaky.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Created or modified. The engine treats both the same way: read the file
    /// and store whatever is there now.
    Upserted,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

impl Change {
    pub fn upserted(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), kind: ChangeKind::Upserted }
    }

    pub fn removed(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), kind: ChangeKind::Removed }
    }
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    kind: ChangeKind,
    first_event: Instant,
    last_event: Instant,
}

#[derive(Debug, Clone, Copy)]
pub struct DebounceConfig {
    pub quiet: Duration,
    pub max_hold: Duration,
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self {
            // Long enough to absorb an editor's save sequence, short enough
            // that sync still feels immediate.
            quiet: Duration::from_millis(400),
            // A file written to continuously still syncs every few seconds
            // rather than never.
            max_hold: Duration::from_secs(5),
        }
    }
}

pub struct Debouncer {
    config: DebounceConfig,
    pending: HashMap<PathBuf, Pending>,
    rescan_requested: bool,
}

impl Debouncer {
    pub fn new(config: DebounceConfig) -> Self {
        Self { config, pending: HashMap::new(), rescan_requested: false }
    }

    /// Record a raw event.
    ///
    /// The most recent event for a path wins: a file created and then deleted
    /// within the quiet period settles as a removal, and one deleted then
    /// recreated — which is how many editors save — settles as an upsert.
    pub fn observe(&mut self, path: impl Into<PathBuf>, kind: ChangeKind, now: Instant) {
        self.pending
            .entry(path.into())
            .and_modify(|p| {
                p.kind = kind;
                p.last_event = now;
            })
            .or_insert(Pending { kind, first_event: now, last_event: now });
    }

    /// Record that the platform dropped events.
    ///
    /// Every backend has a bounded queue, and a large operation — unpacking an
    /// archive, restoring a backup — can overrun it. When that happens the
    /// event stream is no longer a complete description of what changed, and
    /// the only correct response is a full rescan. Silently continuing would
    /// leave files permanently out of sync with no error anywhere.
    pub fn observe_overflow(&mut self) {
        self.rescan_requested = true;
    }

    /// Whether a full rescan is owed, clearing the flag.
    pub fn take_rescan(&mut self) -> bool {
        std::mem::replace(&mut self.rescan_requested, false)
    }

    /// Remove and return every path that has settled.
    pub fn drain_settled(&mut self, now: Instant) -> Vec<Change> {
        let quiet = self.config.quiet;
        let max_hold = self.config.max_hold;

        let ready: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, p)| {
                now.duration_since(p.last_event) >= quiet
                    || now.duration_since(p.first_event) >= max_hold
            })
            .map(|(path, _)| path.clone())
            .collect();

        let mut out = Vec::with_capacity(ready.len());
        for path in ready {
            let p = self.pending.remove(&path).expect("just selected");
            out.push(Change { path, kind: p.kind });
        }
        // Stable order so callers and tests see something deterministic;
        // HashMap iteration order is not.
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Push a path's quiet period back, because it turned out not to be
    /// finished after all.
    ///
    /// The watcher calls this when a file's size or modification time changed
    /// between the last event and the moment it was about to be released — a
    /// write whose event has not arrived yet. Chunking a file mid-write would
    /// store a torn copy, and worse, the memory-mapped read in the storage
    /// layer raises SIGBUS if the file is truncated underneath it.
    pub fn defer(&mut self, path: &Path, now: Instant) {
        if let Some(p) = self.pending.get_mut(path) {
            p.last_event = now;
        } else {
            self.pending.insert(
                path.to_path_buf(),
                Pending { kind: ChangeKind::Upserted, first_event: now, last_event: now },
            );
        }
    }

    /// When the next path becomes eligible, if any are waiting.
    ///
    /// Lets the caller sleep until there is something to do instead of polling.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending
            .values()
            .map(|p| {
                let by_quiet = p.last_event + self.config.quiet;
                let by_hold = p.first_event + self.config.max_hold;
                by_quiet.min(by_hold)
            })
            .min()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn debouncer() -> (Instant, Debouncer) {
        let config = DebounceConfig {
            quiet: Duration::from_millis(400),
            max_hold: Duration::from_secs(5),
        };
        (Instant::now(), Debouncer::new(config))
    }

    #[test]
    fn a_single_event_settles_after_the_quiet_period() {
        let (t0, mut d) = debouncer();
        d.observe("/a.txt", ChangeKind::Upserted, t0);

        assert!(d.drain_settled(at(t0, 399)).is_empty(), "not yet quiet");
        assert_eq!(d.drain_settled(at(t0, 400)), vec![Change::upserted("/a.txt")]);
        assert_eq!(d.pending_count(), 0);
    }

    #[test]
    fn a_burst_of_events_collapses_to_one_change() {
        // What saving a file in an editor actually looks like.
        let (t0, mut d) = debouncer();
        for ms in [0, 5, 12, 13, 40, 41] {
            d.observe("/doc.txt", ChangeKind::Upserted, at(t0, ms));
        }
        assert_eq!(d.pending_count(), 1);

        assert!(d.drain_settled(at(t0, 400)).is_empty(), "quiet counts from the last event");
        assert_eq!(d.drain_settled(at(t0, 441)), vec![Change::upserted("/doc.txt")]);
    }

    #[test]
    fn a_continuously_written_file_is_released_at_the_maximum_hold() {
        // A long download would otherwise never settle and never sync.
        let (t0, mut d) = debouncer();
        for ms in (0..6000).step_by(50) {
            d.observe("/big.iso", ChangeKind::Upserted, at(t0, ms));
        }
        let settled = d.drain_settled(at(t0, 5000));
        assert_eq!(settled, vec![Change::upserted("/big.iso")], "max hold must force a release");
    }

    #[test]
    fn the_last_event_decides_the_kind() {
        let (t0, mut d) = debouncer();

        // Created then deleted: a temp file that came and went.
        d.observe("/tmp-thing", ChangeKind::Upserted, t0);
        d.observe("/tmp-thing", ChangeKind::Removed, at(t0, 10));
        assert_eq!(d.drain_settled(at(t0, 500)), vec![Change::removed("/tmp-thing")]);

        // Deleted then recreated: how many editors save.
        d.observe("/doc.txt", ChangeKind::Removed, at(t0, 600));
        d.observe("/doc.txt", ChangeKind::Upserted, at(t0, 610));
        assert_eq!(d.drain_settled(at(t0, 1100)), vec![Change::upserted("/doc.txt")]);
    }

    #[test]
    fn paths_settle_independently() {
        let (t0, mut d) = debouncer();
        d.observe("/a", ChangeKind::Upserted, t0);
        d.observe("/b", ChangeKind::Upserted, at(t0, 300));

        assert_eq!(d.drain_settled(at(t0, 400)), vec![Change::upserted("/a")]);
        assert_eq!(d.pending_count(), 1);
        assert_eq!(d.drain_settled(at(t0, 700)), vec![Change::upserted("/b")]);
    }

    #[test]
    fn defer_pushes_the_deadline_out() {
        // A file still growing when we were about to read it.
        let (t0, mut d) = debouncer();
        d.observe("/growing.bin", ChangeKind::Upserted, t0);

        d.defer(Path::new("/growing.bin"), at(t0, 400));
        assert!(d.drain_settled(at(t0, 700)).is_empty(), "deferral resets the quiet period");
        assert_eq!(d.drain_settled(at(t0, 800)), vec![Change::upserted("/growing.bin")]);
    }

    #[test]
    fn deferring_an_unknown_path_starts_tracking_it() {
        let (t0, mut d) = debouncer();
        d.defer(Path::new("/appeared.bin"), t0);
        assert_eq!(d.drain_settled(at(t0, 400)), vec![Change::upserted("/appeared.bin")]);
    }

    #[test]
    fn overflow_requests_a_rescan_once() {
        let (_t0, mut d) = debouncer();
        assert!(!d.take_rescan());

        d.observe_overflow();
        assert!(d.take_rescan(), "the flag is reported");
        assert!(!d.take_rescan(), "and cleared, so one overflow means one rescan");
    }

    #[test]
    fn next_deadline_is_the_soonest_release() {
        let (t0, mut d) = debouncer();
        assert!(d.next_deadline().is_none());

        d.observe("/late", ChangeKind::Upserted, at(t0, 200));
        d.observe("/early", ChangeKind::Upserted, t0);
        assert_eq!(d.next_deadline(), Some(at(t0, 400)), "the earliest quiet expiry");
    }

    #[test]
    fn next_deadline_accounts_for_the_maximum_hold() {
        let (t0, mut d) = debouncer();
        d.observe("/busy", ChangeKind::Upserted, t0);
        // Kept alive past where max_hold, not quiet, becomes the binding limit.
        for ms in (0..4900).step_by(100) {
            d.observe("/busy", ChangeKind::Upserted, at(t0, ms));
        }
        assert_eq!(d.next_deadline(), Some(at(t0, 5000)));
    }

    #[test]
    fn settled_output_is_ordered() {
        let (t0, mut d) = debouncer();
        for p in ["/c", "/a", "/b"] {
            d.observe(p, ChangeKind::Upserted, t0);
        }
        let settled = d.drain_settled(at(t0, 400));
        let paths: Vec<_> = settled.iter().map(|c| c.path.to_str().unwrap()).collect();
        assert_eq!(paths, vec!["/a", "/b", "/c"]);
    }
}
